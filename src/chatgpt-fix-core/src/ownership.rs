use std::fs;
use std::path::Path;

use crate::json::JsonParser;
use crate::{BreakawayEvent, ContractError, JobMemberEntry, OwnershipState, OwnershipV1};

/// The process-tree fixture manifest inside an ownership fixture root.
const TREE_MANIFEST: &str = "process-tree.json";

fn contract_error(code: &'static str, field: &str, message: impl Into<String>) -> ContractError {
    ContractError::new(code, field, message)
}

/// Observe a synthetic process tree and produce a report-only ownership
/// receipt (`chatgpt_fix.ownership.v1`).
///
/// The fixture root must contain a `process-tree.json` manifest with:
/// ```json
/// {
///   "schema": "chatgpt_fix.process_tree.v1",
///   "launch_id": "fixture-1",
///   "root_pid": 1000,
///   "processes": [
///     {"pid": 1000, "name": "ChatGPT.exe", "in_job": true, "breakaway": false},
///     {"pid": 1001, "name": "uv.exe", "in_job": true, "breakaway": false},
///     {"pid": 1002, "name": "node.exe", "in_job": true, "breakaway": true}
///   ]
/// }
/// ```
///
/// Observation is strictly read-only: the launcher never closes the Job,
/// never terminates, and never performs graceful shutdown.
pub fn observe_process_tree(fixture_root: &Path) -> Result<OwnershipV1, ContractError> {
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

    let processes_value = parsed.field("processes")?;
    let processes_array = processes_value.as_array()?;
    let mut job_members = Vec::with_capacity(processes_array.len());
    let mut breakaway_events = Vec::new();
    let mut outside_job = 0_u64;
    let mut breakaway_count = 0_u64;

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

        job_members.push(JobMemberEntry {
            pid: pid as u64,
            name: name.clone(),
            in_job,
            breakaway,
        });
        if !in_job {
            outside_job += 1;
        }
        if breakaway {
            breakaway_count += 1;
            breakaway_events.push(BreakawayEvent {
                pid: pid as u64,
                name,
                event: "breakaway_detected".to_owned(),
            });
        }
    }

    let exit_report = format!(
        "processes_observed={} outside_job={} breakaway={} (report-only; no action taken)",
        job_members.len(),
        outside_job,
        breakaway_count
    );

    let ownership = OwnershipV1 {
        launch_id,
        root_pid: root_pid as u64,
        job_members,
        breakaway_events,
        exit_report,
        state: OwnershipState::Observed,
        created_at_utc: crate::utc_now_rfc3339(),
    };
    ownership.validate()?;
    Ok(ownership)
}
