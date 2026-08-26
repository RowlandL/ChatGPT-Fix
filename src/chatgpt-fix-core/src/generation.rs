use std::fs::{self, File};
use std::io::Write as IoWrite;
use std::path::{Path, PathBuf};

use crate::json::write_string;
use crate::{
    ContractError, GenerationState, GenerationV1, LaunchV1, SafeRelativePath, ShortcutBackup,
    StagingState, StagingV1,
};

/// The current.json pointer file name inside the ChatGPT-Fix program root.
const POINTER_FILE: &str = "current.json";
/// The generation receipt file name inside a generation backup directory.
const GENERATION_FILE: &str = "generation.json";
/// The launch ledger file name inside a generation backup directory.
const LAUNCH_LEDGER_FILE: &str = "launch-ledger.json";

fn contract_error(code: &'static str, field: &str, message: impl Into<String>) -> ContractError {
    ContractError::new(code, field, message)
}

/// A short random hex suffix for generation IDs so two activations in the
/// same second never collide. Uses `std::time::SystemTime` nanoseconds XOR
/// process ID as entropy (no external RNG dependency).
fn random_hex_suffix() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    let pid = std::process::id();
    format!(
        "{:06x}",
        (nanos ^ (pid as u32)).wrapping_mul(0x9e3779b1) & 0xFFFFFF
    )
}

/// Validate that `baseline_root` is an A3-verified immutable baseline
/// (its state.json must exist and carry `state=verified`).
fn require_verified_baseline(baseline_root: &Path) -> Result<(), ContractError> {
    let state_path = baseline_root.join("state.json");
    let bytes = fs::read(&state_path).map_err(|error| {
        contract_error(
            "baseline_state_unreadable",
            "baseline_root",
            format!("baseline state.json cannot be read: {error}"),
        )
    })?;
    let staging = StagingV1::from_json(&bytes).map_err(|error| {
        contract_error(
            "baseline_state_invalid",
            "baseline_root",
            format!("baseline state.json is invalid: {error}"),
        )
    })?;
    if staging.state != StagingState::Verified {
        return Err(contract_error(
            "baseline_not_verified",
            "baseline_root",
            format!("baseline is not verified (state={:?})", staging.state),
        ));
    }
    Ok(())
}

/// Fast-path baseline check for the LAUNCH path: a full `StagingV1` parse
/// walks the entire per-file hash manifest (thousands of entries), which adds
/// startup latency for no launch-time benefit. This version only verifies the
/// schema marker and the `state=verified` flag by substring scan on the raw
/// JSON. `activate` still uses the strict full-parse variant.
///
/// Compatibility: the initial (pre-refactor) installer writes
/// `codex.ntfs.setup-state.v1` + `ExperimentalMitigation:true` instead of the
/// `chatgpt_fix.staging.v1` + `state=verified` pair. Both mark a baseline that
/// was produced by an installer with an immutable copy of the official
/// package, so the launch path accepts either schema. Kept as a substring
/// scan (no full manifest parse) to preserve launch-time latency.
fn require_verified_baseline_fast(baseline_root: &Path) -> Result<(), ContractError> {
    let state_path = baseline_root.join("state.json");
    let bytes = fs::read(&state_path).map_err(|error| {
        contract_error(
            "baseline_state_unreadable",
            "baseline_root",
            format!("baseline state.json cannot be read: {error}"),
        )
    })?;
    let text = String::from_utf8_lossy(&bytes);
    // Whitespace-normalised copy: JSON allows arbitrary whitespace around
    // `:` and `,`, so an installer that writes `"Schema" : "..."` must not
    // fail this fast-path check. Schema marker fields contain no
    // whitespace, so compacting is safe and keeps the scan O(n) (no full
    // manifest parse).
    let compact = text
        .chars()
        .filter(|c| !c.is_ascii_whitespace())
        .collect::<String>();
    // Modern schema (chatgpt_fix.staging.v1) must carry state=verified.
    let staging_v1 = compact.contains("\"schema\":\"chatgpt_fix.staging.v1\"")
        && compact.contains("\"state\":\"verified\"");
    // Initial installer schema (codex.ntfs.setup-state.v1) is verified when
    // the mitigation flag is on. Both cases require an existing state.json
    // (a pointer written by an out-of-band path, e.g. directly at WindowsApps,
    // has neither and is rejected here — the guard that keeps the NTFS-fix
    // launch path intact).
    let setup_state_v1 = compact.contains("\"Schema\":\"codex.ntfs.setup-state.v1\"")
        && compact.contains("\"ExperimentalMitigation\":true");
    if !staging_v1 && !setup_state_v1 {
        return Err(contract_error(
            "baseline_not_verified",
            "baseline_root",
            "baseline state.json is not a verified immutable baseline \
             (expected chatgpt_fix.staging.v1 state=verified or \
             codex.ntfs.setup-state.v1 ExperimentalMitigation=true)",
        ));
    }
    Ok(())
}

/// Atomically replace a pointer file: write to a sibling `.tmp` then
/// `fs::rename`. A failed write leaves the previous pointer untouched.
fn atomic_write_pointer(pointer_path: &Path, content: &str) -> Result<(), ContractError> {
    let tmp_path = pointer_path.with_extension("json.tmp");
    {
        let mut file = File::create(&tmp_path).map_err(|error| {
            contract_error(
                "pointer_write",
                "pointer_path",
                format!("cannot create pointer tmp file: {error}"),
            )
        })?;
        file.write_all(content.as_bytes()).map_err(|error| {
            contract_error(
                "pointer_write",
                "pointer_path",
                format!("cannot write pointer tmp file: {error}"),
            )
        })?;
        file.write_all(b"\n").map_err(|error| {
            contract_error(
                "pointer_write",
                "pointer_path",
                format!("cannot write pointer newline: {error}"),
            )
        })?;
        file.sync_all().map_err(|error| {
            contract_error(
                "pointer_write",
                "pointer_path",
                format!("cannot sync pointer tmp file: {error}"),
            )
        })?;
    }
    fs::rename(&tmp_path, pointer_path).map_err(|error| {
        contract_error(
            "pointer_rename",
            "pointer_path",
            format!("cannot atomically replace pointer: {error}"),
        )
    })?;
    Ok(())
}

fn write_generation_receipt(dir: &Path, generation: &GenerationV1) -> Result<(), ContractError> {
    let json = generation.to_json()?;
    let path = dir.join(GENERATION_FILE);
    let tmp = dir.join(format!("{GENERATION_FILE}.tmp"));
    {
        let mut file = File::create(&tmp).map_err(|error| {
            contract_error(
                "generation_write",
                "generation.json",
                format!("cannot create generation receipt: {error}"),
            )
        })?;
        file.write_all(json.as_bytes()).map_err(|error| {
            contract_error(
                "generation_write",
                "generation.json",
                format!("cannot write generation receipt: {error}"),
            )
        })?;
        file.write_all(b"\n").map_err(|error| {
            contract_error(
                "generation_write",
                "generation.json",
                format!("cannot write generation newline: {error}"),
            )
        })?;
        file.sync_all().map_err(|error| {
            contract_error(
                "generation_write",
                "generation.json",
                format!("cannot sync generation receipt: {error}"),
            )
        })?;
    }
    fs::rename(&tmp, &path).map_err(|error| {
        contract_error(
            "generation_write",
            "generation.json",
            format!("cannot finalize generation receipt: {error}"),
        )
    })?;
    Ok(())
}

/// Build a pointer JSON pointing at the given baseline root.
fn pointer_json(baseline_root: &Path) -> String {
    format!(
        "{{\"schema\":\"chatgpt_fix.pointer.v1\",\"baseline_root\":\"{}\"}}",
        baseline_root.to_string_lossy().replace('\\', "/")
    )
}

/// Activate a verified baseline: atomically switch the pointer and persist
/// the generation transaction (shortcut backup + optional launch ledger).
///
/// Only A4a-authorized paths are written. `program_root` is the
/// `%LOCALAPPDATA%\Programs\ChatGPT-Fix` root that owns `current.json` and
/// the `backups\` directory.
pub fn activate(
    baseline_root: &Path,
    program_root: &Path,
    shortcut: ShortcutBackup,
    smoke: bool,
) -> Result<GenerationV1, ContractError> {
    require_verified_baseline(baseline_root)?;

    // Include a random suffix so two activations in the same second never
    // collide on the same backup directory.
    let generation_id = format!(
        "{}-{}",
        crate::utc_now_rfc3339()
            .replace([':', '-'], "")
            .replace('T', "-"),
        random_hex_suffix()
    );
    let backup_dir = program_root.join("backups").join(&generation_id);
    fs::create_dir_all(&backup_dir).map_err(|error| {
        contract_error(
            "backup_mkdir",
            "backups",
            format!("cannot create generation backup dir: {error}"),
        )
    })?;

    let pointer_path = program_root.join(POINTER_FILE);
    let launch_ledger_path = backup_dir.join(LAUNCH_LEDGER_FILE);
    // Persist an empty launch ledger now; smoke writes entries later.
    write_launch_ledger(&launch_ledger_path, "")?;

    let state = if smoke {
        GenerationState::SmokeStarted
    } else {
        GenerationState::Activated
    };
    let generation = GenerationV1 {
        generation_id: generation_id.clone(),
        baseline_root: baseline_root.to_string_lossy().replace('\\', "/"),
        pointer_path: pointer_path.to_string_lossy().replace('\\', "/"),
        shortcut_backup: shortcut,
        launch_ledger: launch_ledger_path.to_string_lossy().replace('\\', "/"),
        state,
        created_at_utc: crate::utc_now_rfc3339(),
    };
    // Write the receipt FIRST so the pointer switch is the single commit
    // point: a crash after this point leaves a complete, roll-back-able
    // generation record.
    generation.validate()?;
    write_generation_receipt(&backup_dir, &generation)?;

    // Atomically switch the pointer (backup preserved in backup dir).
    let pointer_json = pointer_json(baseline_root);
    atomic_write_pointer(&pointer_path, &pointer_json)?;
    Ok(generation)
}

fn write_launch_ledger(path: &Path, content: &str) -> Result<(), ContractError> {
    let tmp = path.with_extension("json.tmp");
    {
        let mut file = File::create(&tmp).map_err(|error| {
            contract_error(
                "ledger_write",
                "launch-ledger.json",
                format!("cannot create launch ledger: {error}"),
            )
        })?;
        file.write_all(content.as_bytes()).map_err(|error| {
            contract_error(
                "ledger_write",
                "launch-ledger.json",
                format!("cannot write launch ledger: {error}"),
            )
        })?;
        file.write_all(b"\n").map_err(|error| {
            contract_error(
                "ledger_write",
                "launch-ledger.json",
                format!("cannot write launch ledger newline: {error}"),
            )
        })?;
        file.sync_all().map_err(|error| {
            contract_error(
                "ledger_write",
                "launch-ledger.json",
                format!("cannot sync launch ledger: {error}"),
            )
        })?;
    }
    fs::rename(&tmp, path).map_err(|error| {
        contract_error(
            "ledger_write",
            "launch-ledger.json",
            format!("cannot finalize launch ledger: {error}"),
        )
    })?;
    Ok(())
}

/// Roll back the latest generation: restore the previous pointer from the
/// generation receipt and restore the shortcut backup. The backup directory
/// is preserved (never auto-deleted).
pub fn rollback(program_root: &Path) -> Result<GenerationV1, ContractError> {
    let backups_root = program_root.join("backups");
    let generation_id = latest_generation_id(&backups_root)?;
    let backup_dir = backups_root.join(&generation_id);
    let receipt_path = backup_dir.join(GENERATION_FILE);
    let bytes = fs::read(&receipt_path).map_err(|error| {
        contract_error(
            "generation_unreadable",
            "generation.json",
            format!("generation receipt cannot be read: {error}"),
        )
    })?;
    let mut generation = GenerationV1::from_json(&bytes)?;

    // The pointer path in the receipt is informational; the actual pointer
    // must always be the program-root current.json. A tampered receipt
    // cannot redirect the rollback write.
    let pointer_path = program_root.join(POINTER_FILE);
    // Roll back to a null pointer (no baseline active).
    atomic_write_pointer(
        &pointer_path,
        "{\"schema\":\"chatgpt_fix.pointer.v1\",\"baseline_root\":null}\n",
    )?;

    generation.state = GenerationState::RolledBack;
    write_generation_receipt(&backup_dir, &generation)?;
    Ok(generation)
}

/// Find the most recently created generation backup directory.
fn latest_generation_id(backups_root: &Path) -> Result<String, ContractError> {
    let entries = fs::read_dir(backups_root).map_err(|error| {
        contract_error(
            "backups_unreadable",
            "backups",
            format!("cannot read backups directory: {error}"),
        )
    })?;
    let mut ids: Vec<String> = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| {
            contract_error(
                "backups_entry",
                "backups",
                format!("cannot read backups entry: {error}"),
            )
        })?;
        let path = entry.path();
        if path.is_dir()
            && let Some(name) = path.file_name().and_then(|n| n.to_str())
            && path.join(GENERATION_FILE).exists()
        {
            ids.push(name.to_owned());
        }
    }
    ids.sort();
    ids.last()
        .cloned()
        .ok_or_else(|| contract_error("no_generation", "backups", "no generation to roll back"))
}

/// Serialize a launch ledger entry (used by tests to verify persistence).
#[allow(dead_code)]
pub(crate) fn serialize_ledger_entry(pid: u32, created_at_utc: &str, launch_nonce: &str) -> String {
    let mut output = String::from("{\"schema\":\"chatgpt_fix.launch_ledger.v1\",\"entries\":[");
    output.push_str("{\"pid\":");
    output.push_str(&pid.to_string());
    output.push_str(",\"created_at_utc\":");
    write_string(&mut output, created_at_utc);
    output.push_str(",\"launch_nonce\":");
    write_string(&mut output, launch_nonce);
    output.push_str("}]}");
    output
}

/// Read the current pointer file for validation in tests.
pub fn read_pointer(program_root: &Path) -> Result<String, ContractError> {
    let path = program_root.join(POINTER_FILE);
    let bytes = fs::read(&path).map_err(|error| {
        contract_error(
            "pointer_unreadable",
            "current.json",
            format!("pointer cannot be read: {error}"),
        )
    })?;
    String::from_utf8(bytes)
        .map_err(|_| contract_error("pointer_non_utf8", "current.json", "pointer is not UTF-8"))
}

/// Parse a shortcut metadata file (`chatgpt_fix.shortcut.v1`) into a
/// `ShortcutBackup`. Used by the Manager to load A4a shortcut metadata from
/// a baseline's `shortcut-backup.json`.
pub fn parse_shortcut_json(bytes: &[u8]) -> Result<ShortcutBackup, ContractError> {
    use crate::json::JsonParser;
    let parsed = JsonParser::new(bytes)?.parse_top_level()?;
    let _obj = parsed.as_object()?;
    let schema = parsed.field("schema")?.as_str()?;
    if schema != "chatgpt_fix.shortcut.v1" {
        return Err(contract_error(
            "schema_mismatch",
            "schema",
            format!("expected chatgpt_fix.shortcut.v1, got {schema}"),
        ));
    }
    let get_str = |key: &str| -> Result<String, ContractError> {
        Ok(parsed.field(key)?.as_str()?.to_owned())
    };
    let shortcut = ShortcutBackup {
        target_path: get_str("target_path")?,
        arguments: get_str("arguments")?,
        working_directory: get_str("working_directory")?,
        icon_location: get_str("icon_location")?,
    };
    if shortcut.target_path.is_empty() {
        return Err(contract_error(
            "empty_field",
            "target_path",
            "must not be empty",
        ));
    }
    Ok(shortcut)
}

/// Write a shortcut metadata file (`chatgpt_fix.shortcut.v1`) into a
/// baseline root. Used by tests to prepare A4a fixtures.
pub fn write_shortcut_json(
    baseline_root: &Path,
    shortcut: &ShortcutBackup,
) -> Result<(), ContractError> {
    let mut output = String::from("{\"schema\":\"chatgpt_fix.shortcut.v1\"");
    output.push_str(",\"target_path\":");
    write_string(&mut output, &shortcut.target_path);
    output.push_str(",\"arguments\":");
    write_string(&mut output, &shortcut.arguments);
    output.push_str(",\"working_directory\":");
    write_string(&mut output, &shortcut.working_directory);
    output.push_str(",\"icon_location\":");
    write_string(&mut output, &shortcut.icon_location);
    output.push_str("}\n");
    let path = baseline_root.join("shortcut-backup.json");
    let tmp = baseline_root.join("shortcut-backup.json.tmp");
    {
        let mut file = File::create(&tmp).map_err(|error| {
            contract_error(
                "shortcut_write",
                "shortcut-backup.json",
                format!("cannot create shortcut file: {error}"),
            )
        })?;
        file.write_all(output.as_bytes()).map_err(|error| {
            contract_error(
                "shortcut_write",
                "shortcut-backup.json",
                format!("cannot write shortcut file: {error}"),
            )
        })?;
        file.sync_all().map_err(|error| {
            contract_error(
                "shortcut_write",
                "shortcut-backup.json",
                format!("cannot sync shortcut file: {error}"),
            )
        })?;
    }
    fs::rename(&tmp, &path).map_err(|error| {
        contract_error(
            "shortcut_write",
            "shortcut-backup.json",
            format!("cannot finalize shortcut file: {error}"),
        )
    })?;
    Ok(())
}

/// Resolve the stable user-data directory for the isolated app:
/// `<program-root>/profile/user-data`.
///
/// The profile must survive baseline swaps: keeping it under the baseline
/// meant every new baseline started with a fresh profile, silently losing
/// the user's appearance settings, desktop-only preferences and session
/// state. The stable location is shared by every baseline and every
/// launcher entry point.
///
/// One-time migration: on the first launch after this change, the active
/// baseline's existing `profile/user-data` is moved (same-volume rename,
/// instant) into the stable location, preserving the user's current state.
/// A marker file prevents repeated attempts. The migration only runs when
/// the app is actually about to be spawned (never while an instance is
/// running, which would pull the profile out from under it).
///
/// Public so the migration behavior can be unit-tested deterministically
/// (the launch path itself skips migration while an instance is running).
pub fn resolve_user_data_dir(program_root: &Path, baseline: &Path) -> PathBuf {
    let stable = program_root.join("profile").join("user-data");
    let marker = program_root
        .join("profile")
        .join("user-data-migrated-v2.marker");
    if !marker.exists() {
        // `fs::rename` requires the destination parent to exist.
        if let Some(parent) = stable.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let legacy = baseline.join("profile").join("user-data");
        let mut migration_ok = true;
        if legacy.is_dir() {
            if !stable.exists() {
                if let Err(error) = fs::rename(&legacy, &stable) {
                    eprintln!("profile_migration_warning: cannot move legacy profile: {error}");
                    migration_ok = false;
                }
            } else if profile_is_newer(&legacy, &stable) {
                let backup = program_root.join("profile").join("user-data.pre-v1.0.5");
                if backup.exists() {
                    eprintln!(
                        "profile_migration_warning: stable profile backup already exists at {}",
                        backup.display()
                    );
                    migration_ok = false;
                } else if let Err(error) = fs::rename(&stable, &backup) {
                    eprintln!("profile_migration_warning: cannot back up stable profile: {error}");
                    migration_ok = false;
                } else if let Err(error) = fs::rename(&legacy, &stable) {
                    eprintln!("profile_migration_warning: cannot promote legacy profile: {error}");
                    let _ = fs::rename(&backup, &stable);
                    migration_ok = false;
                }
            }
        }
        if let Err(error) = fs::create_dir_all(&stable) {
            eprintln!("profile_migration_warning: cannot create stable profile: {error}");
            migration_ok = false;
        }
        if migration_ok && stable.exists() {
            // Marker prevents re-running the migration on every launch.
            let _ = fs::write(&marker, b"chatgpt-fix-profile-migrated-v2\n");
        }
    }
    stable
}

fn profile_is_newer(candidate: &Path, current: &Path) -> bool {
    newest_profile_mtime(candidate).unwrap_or(std::time::SystemTime::UNIX_EPOCH)
        > newest_profile_mtime(current).unwrap_or(std::time::SystemTime::UNIX_EPOCH)
}

fn newest_profile_mtime(root: &Path) -> Option<std::time::SystemTime> {
    let mut newest = None;
    for entry in fs::read_dir(root).ok()?.flatten() {
        let path = entry.path();
        if let Ok(modified) = entry.metadata().and_then(|metadata| metadata.modified())
            && newest.is_none_or(|value| modified > value)
        {
            newest = Some(modified);
        }
        if entry.file_type().is_ok_and(|file_type| file_type.is_dir())
            && let Some(modified) = newest_profile_mtime(&path)
            && newest.is_none_or(|value| modified > value)
        {
            newest = Some(modified);
        }
    }
    newest
}

/// Launch the active ChatGPT baseline from `<program-root>/current.json`.
///
/// Reads the `chatgpt_fix.pointer.v1` pointer, resolves the baseline root,
/// locates `ChatGPT.exe` (either at the baseline root or in its `app\`
/// subdirectory), and launches it detached. The executable path in the
/// returned `LaunchV1` is a safe relative path under the baseline root.
///
/// A4a-authorized. This is the real launch path (P4-era live launch);
/// it supersedes the P1 fail-closed stub once a pointer exists.
/// Resolve the current pointer and, when the target baseline is verified and
/// no instance is already running, launch the isolated ChatGPT.exe.
///
/// Returns `(launch_receipt, spawned_pid)`: `spawned_pid` is `Some` only when
/// this call actually spawned a new instance (resident launcher mode should
/// then hold the Job and wait for it to exit); it is `None` for the
/// single-instance reuse path and for any fail-closed outcome.
pub fn launch_from_pointer(program_root: &Path) -> Result<(LaunchV1, Option<u32>), ContractError> {
    use std::process::Command;

    let pointer_text = read_pointer(program_root)?;
    let parsed = crate::json::JsonParser::new(pointer_text.as_bytes())?.parse_top_level()?;
    let _obj = parsed.as_object()?;
    let schema = parsed.field("schema")?.as_str()?;
    if schema != "chatgpt_fix.pointer.v1" {
        return Err(contract_error(
            "schema_mismatch",
            "schema",
            format!("expected chatgpt_fix.pointer.v1, got {schema}"),
        ));
    }
    let baseline_value = parsed.field("baseline_root")?;
    // Handle JSON null (rolled back pointer) as a distinct fail-closed case
    // before attempting string conversion.
    let baseline_root = match baseline_value.as_str() {
        Ok(s) => s,
        Err(_) => {
            return Err(contract_error(
                "pointer_null",
                "baseline_root",
                "current.json points at no baseline (null); activate a baseline first",
            ));
        }
    };
    if baseline_root.is_empty() {
        return Err(contract_error(
            "pointer_null",
            "baseline_root",
            "current.json points at no baseline (null); activate a baseline first",
        ));
    }

    // Resolve ChatGPT.exe under the baseline root. Accept either
    // <root>/ChatGPT.exe or <root>/app/ChatGPT.exe (official package layout).
    let baseline = Path::new(baseline_root);
    // Fail-closed: only launch from a verified immutable baseline (state.json
    // with state=verified). A pointer written by an out-of-band path (e.g.
    // pointing directly at WindowsApps) is rejected here — the guard that
    // keeps the NTFS-fix launch path intact. Fast substring check (no full
    // manifest parse) to keep startup latency low.
    require_verified_baseline_fast(baseline)?;
    let mut executable = baseline.join("ChatGPT.exe");
    if !executable.is_file() {
        let nested = baseline.join("app").join("ChatGPT.exe");
        if nested.is_file() {
            executable = nested;
        }
    }
    if !executable.is_file() {
        return Err(contract_error(
            "baseline_executable_missing",
            "ChatGPT.exe",
            format!("no ChatGPT.exe found under baseline root {}", baseline_root),
        ));
    }

    // Single-instance guard: if a ChatGPT.exe process is already running,
    // do NOT spawn another copy (multiple Electron stacks make startup slow
    // and waste memory). Reuse the running instance instead AND bring its
    // window to the foreground so the user sees feedback (instead of the
    // launcher silently doing nothing).
    //
    // Windowless-state recovery: after the user closes the main window the
    // process tree can stay alive in the background (window-all-closed
    // keeps the app resident). Reusing that instance has no visible effect
    // and makes the shortcut appear completely dead. When no visible
    // ChatGPT window exists, the stale instance is closed and a fresh one
    // is spawned below.
    if chatgpt_process_running() {
        let reuse_launch = build_reuse_launch(baseline, &executable)?;
        #[cfg(windows)]
        {
            // Bring the running app window to the foreground so the user
            // sees feedback. The AUMID launch (shell:AppsFolder) only works
            // for MSIX-registered apps; a user-owned baseline copy is not
            // registered, so we enumerate top-level windows instead.
            if crate::win32::activate_chatgpt_window() {
                return Ok((reuse_launch, None));
            }
            // No visible window found: close the stale windowless instance
            // (graceful first, force only after a grace period) and fall
            // through to the spawn path below.
            close_stale_chatgpt_instance();
        }
        #[cfg(not(windows))]
        {
            return Ok((reuse_launch, None));
        }
    }

    // Launch detached: the launcher does not wait for the app to exit.
    // Dropping the Child handle detaches it — the process keeps running
    // independently (on Windows, dropping without wait leaves the child
    // alive; never call kill() here).
    //
    // User data lives in a stable profile under the program root
    // (`<program-root>/profile/user-data`), NOT under the baseline: a
    // baseline swap must never reset the user's appearance/desktop
    // settings or session state. One-time migration from the legacy
    // baseline profile is handled by `resolve_user_data_dir`.
    let profile_dir = resolve_user_data_dir(program_root, baseline);
    let _ = fs::create_dir_all(&profile_dir);
    let mut command = Command::new(&executable);
    command
        .current_dir(baseline)
        .arg(format!("--user-data-dir={}", profile_dir.to_string_lossy()));
    // Diagnostic: capture the app's stderr (GUI exe has no console) so
    // loader/overlay errors (e.g. "[ntc] loader failed") are visible in
    // <baseline>/logs/chatgpt-stderr.log. Rotate at 10 MB (keep the previous
    // generation as .old) so a long-running app cannot grow the file forever.
    let stderr_log = baseline.join("logs").join("chatgpt-stderr.log");
    if let Some(dir) = stderr_log.parent() {
        let _ = fs::create_dir_all(dir);
    }
    const MAX_STDERR_LOG_BYTES: u64 = 10 * 1024 * 1024;
    if let Ok(metadata) = fs::metadata(&stderr_log)
        && metadata.len() > MAX_STDERR_LOG_BYTES
    {
        let _ = fs::rename(
            &stderr_log,
            baseline.join("logs").join("chatgpt-stderr.log.old"),
        );
    }
    if let Ok(file) = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&stderr_log)
    {
        command.stderr(std::process::Stdio::from(file));
    }
    let child = command.spawn().map_err(|error| {
        contract_error(
            "launch_spawn_failed",
            "ChatGPT.exe",
            format!("cannot launch ChatGPT.exe: {error}"),
        )
    })?;
    let spawned_pid = child.id();
    // Job ownership (plan P4/P5): assign the root child into a Job so the
    // launcher owns the app tree for its lifetime. The Job handle is parked
    // in a static (hold_job) and reclaimed by the OS when the launcher exits;
    // KILL_ON_JOB_CLOSE is deliberately not set.
    #[cfg(windows)]
    {
        if let Some(job) = crate::win32::create_job()
            && crate::win32::assign_process_to_job(job, spawned_pid)
        {
            crate::win32::hold_job(job);
        }
    }
    // Detach: the Child is dropped without wait/kill; the app keeps running.
    drop(child);

    let launch = build_reuse_launch(baseline, &executable)?;
    Ok((launch, Some(spawned_pid)))
}

/// Build the single-instance reuse launch receipt for an already-running
/// ChatGPT instance.
fn build_reuse_launch(baseline: &Path, executable: &Path) -> Result<LaunchV1, ContractError> {
    let launch = LaunchV1 {
        launch_id: format!(
            "live-{}",
            crate::utc_now_rfc3339()
                .replace([':', '-'], "")
                .replace('T', "-")
        ),
        generation: 1,
        baseline_id: SafeRelativePath::parse(
            baseline
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .as_ref(),
        )
        .unwrap_or_else(|_| SafeRelativePath::parse("baseline").expect("static safe path")),
        executable: SafeRelativePath::parse(
            executable
                .strip_prefix(baseline)
                .unwrap_or(executable)
                .to_string_lossy()
                .replace('\\', "/")
                .as_str(),
        )
        .map_err(|error| contract_error("invalid_value", "executable", format!("{error}")))?,
        would_start: true,
        reason: None,
    };
    launch.validate()?;
    Ok(launch)
}

/// Best-effort close of a stale windowless ChatGPT instance so the launch
/// path can spawn a fresh one. Graceful (`taskkill /T` sends WM_CLOSE to the
/// tree) first; force-kill only if the processes stay alive after a short
/// grace period (which also lets the Chromium profile lock be released).
fn close_stale_chatgpt_instance() {
    use std::process::Command;
    let _ = console_hidden(Command::new("taskkill"))
        .args(["/IM", "ChatGPT.exe", "/T"])
        .output();
    wait_for_chatgpt_exit(5);
    if chatgpt_process_running() {
        let _ = console_hidden(Command::new("taskkill"))
            .args(["/IM", "ChatGPT.exe", "/T", "/F"])
            .output();
        wait_for_chatgpt_exit(3);
    }
}

/// Configure a child console process so it never flashes a console window.
///
/// The launcher is a `windows_subsystem = "windows"` binary; spawning
/// `tasklist`/`taskkill` from it would otherwise pop a new console window
/// per invocation (very visible during the close/poll loop).
fn console_hidden(mut command: std::process::Command) -> std::process::Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // CREATE_NO_WINDOW
        command.creation_flags(0x0800_0000);
    }
    command
}

/// Poll `chatgpt_process_running` for up to `seconds`, returning early as
/// soon as the processes are gone.
fn wait_for_chatgpt_exit(seconds: u64) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(seconds);
    while chatgpt_process_running() && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
}

/// Returns true when at least one ChatGPT.exe process is already running
/// (single-instance guard). Uses `tasklist` with a GBK-safe byte check so
/// the locale of the output never matters.
fn chatgpt_process_running() -> bool {
    use std::process::Command;
    let output = match console_hidden(Command::new("tasklist"))
        .args(["/FI", "IMAGENAME eq ChatGPT.exe", "/FO", "CSV", "/NH"])
        .output()
    {
        Ok(output) => output,
        Err(_) => return false, // tasklist unavailable: fail open to spawn
    };
    if !output.status.success() {
        return false;
    }
    // Case-insensitive scan for the image name token in the raw bytes.
    let hay = output.stdout.to_ascii_lowercase();
    hay.windows(b"chatgpt.exe".len())
        .any(|w| w == b"chatgpt.exe")
}
