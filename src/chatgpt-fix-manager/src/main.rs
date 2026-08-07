use std::ffi::OsStr;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use chatgpt_fix_core::ShortcutBackup;

const PRODUCT_NAME: &str = "ChatGPT-Fix-Manager";

fn main() -> ExitCode {
    let arguments = std::env::args_os().skip(1).collect::<Vec<_>>();

    match arguments.as_slice() {
        [argument] if argument == OsStr::new("--version") => {
            chatgpt_fix_core::run_version_command(PRODUCT_NAME)
        }
        [command, option, root]
            if command == OsStr::new("review") && option == OsStr::new("--fixture-root") =>
        {
            review_fixture(Path::new(root))
        }
        [command, option]
            if command == OsStr::new("review") && option == OsStr::new("--plan-stdin") =>
        {
            review_plan_stdin()
        }
        [command, option, root]
            if command == OsStr::new("activate") && option == OsStr::new("--baseline") =>
        {
            run_activate(Path::new(root), false)
        }
        [command, option, root, smoke_flag]
            if command == OsStr::new("activate")
                && option == OsStr::new("--baseline")
                && smoke_flag == OsStr::new("--smoke") =>
        {
            run_activate(Path::new(root), true)
        }
        [command] if command == OsStr::new("rollback") => run_rollback(),
        [command, option, scope]
            if command == OsStr::new("config-plan") && option == OsStr::new("--scope") =>
        {
            run_config_plan(Path::new(scope))
        }
        [command, option, proposal, canary_option, canary]
            if command == OsStr::new("config-apply")
                && option == OsStr::new("--proposal")
                && canary_option == OsStr::new("--canary") =>
        {
            run_config_apply(Path::new(proposal), Path::new(canary))
        }
        [command, option, proposal]
            if command == OsStr::new("config-rollback") && option == OsStr::new("--proposal") =>
        {
            run_config_rollback(Path::new(proposal))
        }
        [command, option, root]
            if command == OsStr::new("maintenance-plan")
                && option == OsStr::new("--fixture-root") =>
        {
            run_maintenance_plan(Path::new(root))
        }
        [command] if command == OsStr::new("ntc-health") => run_ntc_health(),
        [command, option, root]
            if command == OsStr::new("ntc-reapply") && option == OsStr::new("--fixture-root") =>
        {
            run_ntc_reapply(Path::new(root))
        }
        [command] if command == OsStr::new("doctor") => {
            eprintln!("doctor v2 requires P6 authorization");
            ExitCode::from(2)
        }
        _ => {
            print_usage();
            ExitCode::from(2)
        }
    }
}

/// `ChatGPT-Fix-Manager activate --baseline <root> [--smoke]`.
///
/// The program root is `%LOCALAPPDATA%\Programs\ChatGPT-Fix`; the shortcut
/// metadata is read from a sibling `shortcut-backup.json` inside the
/// baseline root (A4a fixture contract), so activate stays fully offline
/// and only touches A4a-authorized paths.
fn run_activate(baseline_root: &Path, smoke: bool) -> ExitCode {
    let program_root = match default_program_root() {
        Ok(root) => root,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(3);
        }
    };

    let shortcut = match read_shortcut_backup(baseline_root) {
        Ok(shortcut) => shortcut,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::from(3);
        }
    };

    match chatgpt_fix_core::activate(baseline_root, &program_root, shortcut, smoke) {
        Ok(generation) => match generation.to_json() {
            Ok(json) => {
                println!("{json}");
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("generation serialize failed: {error}");
                ExitCode::from(4)
            }
        },
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(3)
        }
    }
}

/// `ChatGPT-Fix-Manager rollback`.
fn run_rollback() -> ExitCode {
    let program_root = match default_program_root() {
        Ok(root) => root,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(3);
        }
    };
    match chatgpt_fix_core::rollback(&program_root) {
        Ok(generation) => match generation.to_json() {
            Ok(json) => {
                println!("{json}");
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("generation serialize failed: {error}");
                ExitCode::from(4)
            }
        },
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(3)
        }
    }
}

/// Resolve the A4a program root. It can be overridden by
/// `CHATGPT_FIX_PROGRAM_ROOT` for offline fixture testing; otherwise it is
/// `%LOCALAPPDATA%\Programs\ChatGPT-Fix`.
fn default_program_root() -> Result<PathBuf, String> {
    if let Some(override_root) = std::env::var_os("CHATGPT_FIX_PROGRAM_ROOT") {
        return Ok(PathBuf::from(override_root));
    }
    let local_app_data = std::env::var_os("LOCALAPPDATA")
        .ok_or_else(|| "LOCALAPPDATA is not set; cannot resolve program root".to_owned())?;
    Ok(PathBuf::from(local_app_data)
        .join("Programs")
        .join("ChatGPT-Fix"))
}

/// Read the A4a shortcut metadata from `<baseline>/shortcut-backup.json`
/// (schema `chatgpt_fix.shortcut.v1`).
fn read_shortcut_backup(baseline_root: &Path) -> Result<ShortcutBackup, String> {
    let path = baseline_root.join("shortcut-backup.json");
    let bytes = std::fs::read(&path)
        .map_err(|error| format!("shortcut-backup.json cannot be read: {error}"))?;
    let parsed = chatgpt_fix_core::parse_shortcut_json(&bytes)
        .map_err(|error| format!("shortcut-backup.json is invalid: {error}"))?;
    Ok(parsed)
}

fn review_fixture(root: &Path) -> ExitCode {
    let plan = match chatgpt_fix_core::plan_fixture(root) {
        Ok(plan) => plan,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::from(3);
        }
    };

    if let Err(error) = plan.validate() {
        eprintln!("invalid_plan at {}: {error}", root.display());
        return ExitCode::from(3);
    }
    match plan.to_json() {
        Ok(json) => {
            println!("{json}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("invalid_plan at {}: {error}", root.display());
            ExitCode::from(3)
        }
    }
}

fn review_plan_stdin() -> ExitCode {
    // Read all stdin bytes.
    let mut buffer = Vec::new();
    if let Err(error) = std::io::stdin().read_to_end(&mut buffer) {
        eprintln!("stdin_read_error: {error}");
        return ExitCode::from(3);
    }

    // Parse and validate the plan.
    let plan = match chatgpt_fix_core::PlanV1::from_json(&buffer) {
        Ok(plan) => plan,
        Err(error) => {
            eprintln!("invalid_plan: {error}");
            return ExitCode::from(3);
        }
    };

    // Output canonical JSON.
    match plan.to_json() {
        Ok(json) => {
            println!("{json}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("invalid_plan: {error}");
            ExitCode::from(3)
        }
    }
}

/// Resolve the A6 config fixture root. It can be overridden by
/// `CHATGPT_FIX_CONFIG_FIXTURE` for offline fixture testing.
fn config_fixture_root() -> Result<PathBuf, String> {
    std::env::var_os("CHATGPT_FIX_CONFIG_FIXTURE")
        .map(PathBuf::from)
        .ok_or_else(|| {
            "CHATGPT_FIX_CONFIG_FIXTURE is not set; cannot locate config fixture".to_owned()
        })
}

/// `ChatGPT-Fix-Manager config-plan --scope <mcp|codex_home|history>`.
///
/// A6 dry-run: emits a `chatgpt_fix.config_proposal.v1` receipt in `proposed`
/// state. No config file is written. The backup path is derived from
/// `CHATGPT_FIX_CANARY_ROOT` (or a default under the temp directory) so the
/// proposal always carries an independent, rollback-able backup target.
fn run_config_plan(scope_arg: &Path) -> ExitCode {
    let scope_name = match scope_arg.to_str() {
        Some(name) => name,
        None => {
            eprintln!("invalid_scope: scope must be mcp, codex_home, or history");
            return ExitCode::from(3);
        }
    };
    let scope = match scope_name {
        "mcp" => chatgpt_fix_core::ConfigScope::Mcp,
        "codex_home" => chatgpt_fix_core::ConfigScope::CodexHome,
        "history" => chatgpt_fix_core::ConfigScope::History,
        other => {
            eprintln!("invalid_scope: expected mcp, codex_home, or history, got {other}");
            return ExitCode::from(3);
        }
    };

    let fixture = match config_fixture_root() {
        Ok(root) => root,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(3);
        }
    };
    let proposal_id = format!("p7-{scope_name}");
    let backup_path = std::env::var_os("CHATGPT_FIX_CANARY_ROOT")
        .map(|root| PathBuf::from(root).join("backups").join(&proposal_id))
        .unwrap_or_else(|| {
            std::env::temp_dir()
                .join("chatgpt-fix-canary")
                .join("backups")
                .join(&proposal_id)
        });

    match chatgpt_fix_core::config_plan(&fixture, scope, &backup_path.to_string_lossy()) {
        Ok(proposal) => match proposal.to_json() {
            Ok(json) => {
                println!("{json}");
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("config proposal serialize failed: {error}");
                ExitCode::from(4)
            }
        },
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(3)
        }
    }
}

/// `ChatGPT-Fix-Manager config-apply --proposal <path> --canary <root>`.
///
/// A6 canary apply: re-reads the proposed content from the fixture (located by
/// `CHATGPT_FIX_CONFIG_FIXTURE`), verifies the digest matches the proposal
/// (fail closed), backs up any existing canary file, and writes the proposed
/// config to the canary root.
fn run_config_apply(proposal_path: &Path, canary_root: &Path) -> ExitCode {
    let fixture = match config_fixture_root() {
        Ok(root) => root,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(3);
        }
    };
    let bytes = match std::fs::read(proposal_path) {
        Ok(bytes) => bytes,
        Err(error) => {
            eprintln!("proposal cannot be read: {error}");
            return ExitCode::from(3);
        }
    };
    let proposal = match chatgpt_fix_core::parse_config_proposal(&bytes) {
        Ok(proposal) => proposal,
        Err(error) => {
            eprintln!("invalid_proposal: {error}");
            return ExitCode::from(3);
        }
    };
    match chatgpt_fix_core::config_apply(&fixture, canary_root, &proposal) {
        Ok(applied) => match applied.to_json() {
            Ok(json) => {
                // Persist the applied state back to the proposal file so a
                // subsequent config-rollback can read the `applied` state.
                if let Err(error) = write_proposal_atomic(proposal_path, &json) {
                    eprintln!("proposal state persist failed: {error}");
                    return ExitCode::from(4);
                }
                println!("{json}");
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("config proposal serialize failed: {error}");
                ExitCode::from(4)
            }
        },
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(3)
        }
    }
}

/// Atomically persist a proposal JSON to its file using .NET File.Replace
/// semantics (write temp, then replace) so a crash never leaves a torn file.
fn write_proposal_atomic(path: &Path, json: &str) -> Result<(), String> {
    let temp_path = path.with_extension("json.tmp");
    std::fs::write(&temp_path, json)
        .map_err(|error| format!("cannot write proposal temp file: {error}"))?;
    std::fs::rename(&temp_path, path)
        .map_err(|error| format!("cannot replace proposal file: {error}"))?;
    Ok(())
}

/// `ChatGPT-Fix-Manager config-rollback --proposal <path>`.
///
/// A6 rollback: restores the canary from the proposal's independent backup.
/// The canary root is the parent of the `backups/<proposal_id>` directory that
/// `config-plan` recorded in the proposal.
fn run_config_rollback(proposal_path: &Path) -> ExitCode {
    let bytes = match std::fs::read(proposal_path) {
        Ok(bytes) => bytes,
        Err(error) => {
            eprintln!("proposal cannot be read: {error}");
            return ExitCode::from(3);
        }
    };
    let proposal = match chatgpt_fix_core::parse_config_proposal(&bytes) {
        Ok(proposal) => proposal,
        Err(error) => {
            eprintln!("invalid_proposal: {error}");
            return ExitCode::from(3);
        }
    };
    // backup_path = <canary_root>/backups/<proposal_id> -> canary_root is two
    // levels up from the backup directory.
    let backup_dir = PathBuf::from(&proposal.backup_path);
    let canary_root = match (
        backup_dir.parent(),
        backup_dir.parent().and_then(|p| p.parent()),
    ) {
        (Some(_), Some(canary)) => canary.to_path_buf(),
        _ => {
            eprintln!("invalid_backup_path: {}", proposal.backup_path);
            return ExitCode::from(3);
        }
    };
    match chatgpt_fix_core::config_rollback(&canary_root, &proposal) {
        Ok(rolled_back) => match rolled_back.to_json() {
            Ok(json) => {
                println!("{json}");
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("config proposal serialize failed: {error}");
                ExitCode::from(4)
            }
        },
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(3)
        }
    }
}

fn print_usage() {
    eprintln!("Usage: {PRODUCT_NAME} --version");
    eprintln!("       {PRODUCT_NAME} review --fixture-root <path>");
    eprintln!("       {PRODUCT_NAME} review --plan-stdin");
    eprintln!("       {PRODUCT_NAME} activate --baseline <root> [--smoke]");
    eprintln!("       {PRODUCT_NAME} rollback");
    eprintln!("       {PRODUCT_NAME} config-plan --scope <mcp|codex_home|history>");
    eprintln!("       {PRODUCT_NAME} config-apply --proposal <path> --canary <root>");
    eprintln!("       {PRODUCT_NAME} config-rollback --proposal <path>");
    eprintln!("       {PRODUCT_NAME} maintenance-plan --fixture-root <path>");
    eprintln!("       {PRODUCT_NAME} ntc-health");
    eprintln!("       {PRODUCT_NAME} ntc-reapply --fixture-root <path>");
    eprintln!("       {PRODUCT_NAME} doctor");
}

/// `ChatGPT-Fix-Manager ntc-health`.
///
/// Read-only native token-cost helper health check: probes the loopback
/// health endpoint (127.0.0.1:17888) and the Scheduled Task state, emitting
/// a `chatgpt_fix.ntc_health.v1` receipt. Never starts/stops the task.
fn run_ntc_health() -> ExitCode {
    match chatgpt_fix_core::ntc_health_check() {
        Ok(health) => match health.to_json() {
            Ok(json) => {
                println!("{json}");
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("invalid ntc health: {error}");
                ExitCode::from(3)
            }
        },
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(3)
        }
    }
}

/// `ChatGPT-Fix-Manager ntc-reapply --fixture-root <path>`.
///
/// Upstream of the token-cost userscript (the version we verified against).
/// Download is best-effort at runtime; the plugin is optional and we never
/// redistribute the script inside the release — this is a local fetch.
const NTC_USERSCRIPT_URL: &str = "https://raw.githubusercontent.com/Tianzora/codex-token-cost/v0.7.9/scripts/codex-live-token-cost.js";

/// Ensure the token-cost userscript exists at `dst`:
///
/// 1) already present;
/// 2) a known local copy (user deployment / our cache);
/// 3) downloaded from the upstream GitHub tag.
///
/// Returns true when the file is available afterwards. Uses PowerShell for
/// the fetch so the Rust code stays dependency-free; failure is not fatal
/// here (caller fail-closes).
fn ensure_userscript(dst: &Path) -> bool {
    if dst.is_file() {
        return true;
    }
    let local_candidates = [
        std::env::var("USERPROFILE")
            .map(|u| {
                PathBuf::from(u)
                    .join(".codex/tools/codex-token-cost/scripts/codex-live-token-cost.js")
            })
            .ok(),
        std::env::var("LOCALAPPDATA")
            .map(|l| PathBuf::from(l).join("Programs/ChatGPT-Fix/ntc/codex-live-token-cost.js"))
            .ok(),
    ];
    for candidate in local_candidates.into_iter().flatten() {
        if candidate.is_file() {
            if let Some(parent) = dst.parent() {
                let _ = fs::create_dir_all(parent);
            }
            if fs::copy(&candidate, dst).is_ok() {
                return true;
            }
        }
    }
    if let Some(parent) = dst.parent() {
        let _ = fs::create_dir_all(parent);
    }
    // Download is skipped when CHATGPT_FIX_NTC_NO_DOWNLOAD is set (tests,
    // explicit offline preference). The caller's fail-closed check then
    // reports the missing userscript.
    if std::env::var_os("CHATGPT_FIX_NTC_NO_DOWNLOAD").is_some() {
        return false;
    }
    let quoted = dst.to_string_lossy().replace('\'', "''");
    let ok = std::process::Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            &format!(
                "Invoke-WebRequest -Uri '{}' -OutFile '{}' -UseBasicParsing",
                NTC_USERSCRIPT_URL, quoted
            ),
        ])
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    ok && dst.is_file()
}

/// Re-applies the native token-cost overlay to the app.asar COPY inside a
/// launcher-owned baseline dir (never the official package). The fixture dir
/// must contain `app.asar` and `ntc-overlay/userscript.js`. Emits a
/// `chatgpt_fix.ntc_manifest.v1` with before/after hashes.
fn run_ntc_reapply(root: &Path) -> ExitCode {
    let script = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("scripts")
        .join("inject-native-token-cost.js");
    let in_asar = root.join("app.asar");
    let userscript = root.join("ntc-overlay").join("codex-live-token-cost.js");
    let out_asar = root.join("app.asar.ntc-new");

    if !in_asar.is_file() {
        eprintln!(
            "ntc_reapply_failed: app.asar not found in {}",
            root.display()
        );
        return ExitCode::from(3);
    }
    // Auto-fetch the userscript when the overlay file is missing: local copy
    // first, then the upstream GitHub tag. The plugin stays optional — if it
    // is still unavailable the fail-closed check below reports it.
    if !ensure_userscript(&userscript) {
        eprintln!(
            "ntc_reapply_warn: userscript unavailable locally and download failed; reapply will fail closed"
        );
    }
    if !userscript.is_file() {
        eprintln!(
            "ntc_reapply_failed: ntc-overlay/codex-live-token-cost.js not found in {}",
            root.display()
        );
        return ExitCode::from(3);
    }
    if !script.is_file() {
        eprintln!(
            "ntc_reapply_failed: inject script missing at {}",
            script.display()
        );
        return ExitCode::from(4);
    }

    let node = std::env::var("CHATGPT_FIX_NODE").unwrap_or_else(|_| "node".to_owned());
    let mut command = std::process::Command::new(&node);
    // @electron/asar is resolved via NODE_PATH (the isolated node workspace);
    // inherit the caller's NODE_PATH when set.
    if let Ok(node_path) = std::env::var("NODE_PATH") {
        command.env("NODE_PATH", node_path);
    }
    command
        .arg(&script)
        .arg(&in_asar)
        .arg(&userscript)
        .arg(&out_asar);
    // Pass the unpacked natives dir (beside the asar) if present so the
    // extraction can resolve native modules.
    let unpacked_dir = root.join("app.asar.unpacked");
    if unpacked_dir.is_dir() {
        command.arg(&unpacked_dir);
    }
    let output = match command.output() {
        Ok(output) => output,
        Err(error) => {
            eprintln!("ntc_reapply_failed: cannot run node: {error}");
            return ExitCode::from(4);
        }
    };

    if !output.status.success() {
        eprintln!(
            "ntc_reapply_failed: inject script exit {}: {}",
            output.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&output.stderr).trim()
        );
        return ExitCode::from(3);
    }

    // Parse the injection receipt (before/after hashes).
    let stdout = String::from_utf8_lossy(&output.stdout);
    let trimmed = stdout.trim();
    let (before, after) = match parse_ntc_receipt(trimmed) {
        Some(v) => v,
        None => {
            eprintln!("ntc_reapply_failed: unexpected inject output: {trimmed}");
            return ExitCode::from(3);
        }
    };

    // Atomic swap: backup current, then replace the asar with the injected
    // one. rename() can fail with "access denied" on Windows when the target
    // is transiently locked; fall back to copy+remove.
    let backup_path = root.join("app.asar.pre-ntc");
    if in_asar.is_file()
        && !backup_path.exists()
        && let Err(error) = std::fs::copy(&in_asar, &backup_path)
    {
        eprintln!("ntc_reapply_failed: cannot back up app.asar: {error}");
        return ExitCode::from(3);
    }
    let commit = match std::fs::rename(&out_asar, &in_asar) {
        Ok(()) => true,
        Err(rename_error) => {
            match std::fs::copy(&out_asar, &in_asar).and_then(|_| std::fs::remove_file(&out_asar)) {
                Ok(()) => true,
                Err(copy_error) => {
                    eprintln!(
                        "ntc_reapply_failed: cannot commit injected app.asar (rename: {rename_error}; copy: {copy_error})"
                    );
                    return ExitCode::from(3);
                }
            }
        }
    };
    let _ = commit;

    // Emit manifest.
    let manifest = chatgpt_fix_core::NtcManifestV1 {
        artifact: "app.asar".to_owned(),
        before_sha256: before,
        after_sha256: after,
        backup_ref: backup_path.to_string_lossy().replace('\\', "/"),
        generation: 1,
        reapply_state: chatgpt_fix_core::NtcReapplyState::Clean,
        helper_health: "reapply-ok".to_owned(),
        created_at_utc: chatgpt_fix_core::utc_now_rfc3339(),
    };
    match manifest.to_json() {
        Ok(json) => {
            println!("{json}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("invalid ntc manifest: {error}");
            ExitCode::from(3)
        }
    }
}

/// Extract before/after SHA-256 from the inject script JSON receipt.
fn parse_ntc_receipt(output: &str) -> Option<(String, String)> {
    use chatgpt_fix_core::json_parse_ntc_receipt;
    json_parse_ntc_receipt(output)
}

/// `ChatGPT-Fix-Manager maintenance-plan --fixture-root <path>`.
///
/// A1 dry-run: emits a `chatgpt_fix.maintenance_plan.v1` receipt listing only
/// authorized would-do items; A7/A8/A9 actions are marked blocked. No file is
/// modified.
fn run_maintenance_plan(root: &Path) -> ExitCode {
    match chatgpt_fix_core::generate_maintenance_plan(root) {
        Ok(plan) => match plan.to_json() {
            Ok(json) => {
                println!("{json}");
                ExitCode::SUCCESS
            }
            Err(err) => {
                eprintln!("maintenance plan serialize failed: {err}");
                ExitCode::from(4)
            }
        },
        Err(err) => {
            eprintln!("{err}");
            ExitCode::from(3)
        }
    }
}
