use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crate::json::JsonParser;
use crate::{ContractError, ShutdownMode, ShutdownState, ShutdownV1};

/// The process-tree fixture manifest inside a shutdown fixture root.
const TREE_MANIFEST: &str = "process-tree.json";

fn contract_error(code: &'static str, field: &str, message: impl Into<String>) -> ContractError {
    ContractError::new(code, field, message)
}

/// A single process entry as parsed from the fixture manifest.
struct TreeEntry {
    pid: u64,
    name: String,
    in_job: bool,
    breakaway: bool,
    handle_inherited: bool,
}

/// Shut down a synthetic owned tree and produce a `chatgpt_fix.shutdown.v1`
/// receipt.
///
/// The fixture root must contain a `process-tree.json` manifest (same shape as
/// the P5 ownership fixture) with an optional top-level `shutdown_mode`
/// (`graceful` or `job_close`; default `graceful`), an optional top-level
/// `reconciled` flag (default true), and optional per-process
/// `handle_inherited` fields.
///
/// Classification (permanent invariant 4):
/// - `handled`: root + owned members that are in the Job, never broke away,
///   are reconciled against the launch ledger, and have no PID-reuse
///   ambiguity. These are the only PIDs that may be shut down.
/// - `excluded`: breakaway members and outside-Job processes (proven not
///   owned; never terminated).
/// - `suspected`: reconciliation failure (ledger mismatch) or PID reuse. Any
///   suspected hit fails the shutdown closed: no PID is handled.
///
/// `graceful` simulates signaling the owned root and waiting for the owned
/// tree to exit (`state=closed` when every handled PID has exited). `job_close`
/// simulates closing the Job Object, terminating the owned tree (`state=closed`).
pub fn shutdown_process_tree(fixture_root: &Path) -> Result<ShutdownV1, ContractError> {
    let manifest_path = fixture_root.join(TREE_MANIFEST);
    let bytes = fs::read(&manifest_path).map_err(|error| {
        contract_error(
            "tree_manifest_unreadable",
            "process-tree.json",
            format!("process tree manifest cannot be read: {error}"),
        )
    })?;
    let parsed = JsonParser::new(&bytes)?.parse_top_level()?;
    let _obj = parsed.as_object()?;

    let schema = parsed.field("schema")?.as_str()?;
    if schema != "chatgpt_fix.process_tree.v1" {
        return Err(contract_error(
            "schema_mismatch",
            "schema",
            format!("expected chatgpt_fix.process_tree.v1, got {schema}"),
        ));
    }

    let launch_id = parsed.field("launch_id")?.as_str()?.to_owned();
    let root_pid = parsed.field("root_pid")?.as_i64()?;
    if root_pid <= 0 {
        return Err(contract_error(
            "invalid_value",
            "root_pid",
            "must be greater than zero",
        ));
    }

    // The desired shutdown mode comes from the fixture so the frozen CLI
    // `shutdown --fixture-root <path>` stays exactly as specified.
    let mode = match parsed.field_opt("shutdown_mode") {
        Some(value) => ShutdownMode::from_json_str(value.as_str()?)?,
        None => ShutdownMode::Graceful,
    };

    // The fixture is reconciled against the launch ledger unless explicitly
    // marked otherwise. A false value models a reconciliation failure, which
    // makes the owned tree unverifiable and therefore fail closed.
    let reconciled = match parsed.field_opt("reconciled") {
        Some(value) => value.as_bool()?,
        None => true,
    };

    let processes_value = parsed.field("processes")?;
    let processes_array = processes_value.as_array()?;
    let mut entries = Vec::with_capacity(processes_array.len());

    for entry in processes_array {
        let pid = entry.field("pid")?.as_i64()?;
        if pid <= 0 {
            return Err(contract_error(
                "invalid_value",
                "processes.pid",
                "must be greater than zero",
            ));
        }
        let name = entry.field("name")?.as_str()?.to_owned();
        let in_job = entry.field("in_job")?.as_bool()?;
        let breakaway = entry.field("breakaway")?.as_bool()?;
        // Handle inheritance is metadata only: a process that inherited the
        // launcher's stdout/stderr handles is still only "owned" when it is a
        // reconciled Job member. Inherited handles alone never authorize a
        // shutdown.
        let handle_inherited = match entry.field_opt("handle_inherited") {
            Some(value) => value.as_bool()?,
            None => false,
        };
        entries.push(TreeEntry {
            pid: pid as u64,
            name,
            in_job,
            breakaway,
            handle_inherited,
        });
    }

    // PID reuse detection: the same PID must not appear more than once in the
    // observed tree (a fresh process reusing a stale PID would be ambiguous).
    let mut pid_seen: HashMap<u64, usize> = HashMap::new();
    let mut pid_reuse: Vec<u64> = Vec::new();
    for entry in &entries {
        match pid_seen.get(&entry.pid) {
            Some(_) => pid_reuse.push(entry.pid),
            None => {
                pid_seen.insert(entry.pid, 1);
            }
        }
    }
    pid_reuse.sort_unstable();
    pid_reuse.dedup();

    let mut handled_pids = Vec::new();
    let mut excluded_pids = Vec::new();
    let mut suspected_pids = Vec::new();

    for entry in &entries {
        if entry.name.trim().is_empty() {
            return Err(contract_error(
                "invalid_value",
                "processes.name",
                "must not be empty",
            ));
        }
        // Handle-inheritance negative case: a process that inherited the
        // launcher's stdout/stderr handles (`handle_inherited`) is external
        // whenever it is not a Job member — inherited handles alone never
        // prove ownership and never authorize a shutdown.
        let handle_inherited_external = entry.handle_inherited && !entry.in_job;
        let proven_external = entry.breakaway || !entry.in_job;
        if proven_external || handle_inherited_external {
            // Proven not owned (breakaway, outside the Job, or a
            // handle-inheriting external). Never handled.
            excluded_pids.push(entry.pid);
        } else if !reconciled || pid_reuse.contains(&entry.pid) {
            // Ownership cannot be proven: ledger reconciliation failed or the
            // PID is ambiguous. Fail closed.
            suspected_pids.push(entry.pid);
        } else {
            // Explicitly owned, in the Job, reconciled, unambiguous.
            handled_pids.push(entry.pid);
        }
    }
    handled_pids.sort_unstable();
    handled_pids.dedup();
    excluded_pids.sort_unstable();
    excluded_pids.dedup();
    suspected_pids.sort_unstable();
    suspected_pids.dedup();

    // Fail closed: any suspected hit aborts the shutdown before any PID is
    // acted on, regardless of mode.
    if !suspected_pids.is_empty() {
        let receipt = ShutdownV1 {
            launch_id,
            root_pid: root_pid as u64,
            shutdown_mode: mode,
            handled_pids: Vec::new(),
            excluded_pids,
            suspected_pids,
            state: ShutdownState::Failed,
            created_at_utc: crate::utc_now_rfc3339(),
        };
        receipt.validate()?;
        return Ok(receipt);
    }

    let state = match mode {
        ShutdownMode::Graceful => {
            // Graceful shutdown: signal the owned root, then verify every
            // handled PID exited before reporting closed.
            if handled_pids.contains(&(root_pid as u64)) {
                ShutdownState::Closed
            } else {
                // The owned root must be part of the handled set; if the
                // fixture contradicts that, fail closed.
                ShutdownState::Failed
            }
        }
        ShutdownMode::JobClose => {
            // Job close: closing the Job terminates every owned member at once.
            ShutdownState::Closed
        }
    };

    let receipt = ShutdownV1 {
        launch_id,
        root_pid: root_pid as u64,
        shutdown_mode: mode,
        handled_pids,
        excluded_pids,
        suspected_pids: Vec::new(),
        state,
        created_at_utc: crate::utc_now_rfc3339(),
    };
    receipt.validate()?;
    Ok(receipt)
}
