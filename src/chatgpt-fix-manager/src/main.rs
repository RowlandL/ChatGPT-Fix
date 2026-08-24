use std::ffi::OsStr;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{ExitCode, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

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
        [command, option, root]
            if command == OsStr::new("ntc-ensure") && option == OsStr::new("--fixture-root") =>
        {
            run_ntc_ensure(Path::new(root))
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
    eprintln!("       {PRODUCT_NAME} ntc-ensure --fixture-root <path>");
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

/// `ChatGPT-Fix-Manager ntc-ensure --fixture-root <path>`.
///
/// Idempotent token-overlay assurance: if the app.asar COPY already carries
/// the token-cost overlay (a `app.asar.pre-ntc` backup exists and
/// `resources/native-token-cost/` is inside the archive), do nothing and
/// report `already_injected`. Otherwise run the same injection as
/// `ntc-reapply`. This is the hook a post-install / pre-launch step calls so
/// the overlay survives a fresh install without touching the installer
/// binary itself.
fn run_ntc_ensure(root: &Path) -> ExitCode {
    let canonical_root = match validate_ntc_fixture_root(root) {
        Ok(root) => root,
        Err(error) => {
            eprintln!("ntc_ensure_failed: {error}");
            return ExitCode::from(3);
        }
    };
    let in_asar = canonical_root.join("app.asar");
    if let Err(error) = validate_ntc_file_path(&canonical_root, &in_asar, true) {
        eprintln!("ntc_ensure_failed: {error}");
        return ExitCode::from(3);
    }
    match ntc_commit_is_current(&canonical_root) {
        Err(error) => {
            eprintln!("ntc_ensure_failed: invalid NTC transaction state: {error}");
            return ExitCode::from(4);
        }
        Ok(false) => {}
        Ok(true) => {
            println!(
                "{{\"schema\":\"chatgpt_fix.ntc_ensure.v1\",\"action\":\"already_injected\",\"baseline\":\"{}\"}}",
                canonical_root.display().to_string().replace('\\', "/")
            );
            return ExitCode::SUCCESS;
        }
    }

    // A backup with no valid commit receipt may describe an interrupted or
    // externally modified transaction. Do not inject over that unknown state.
    if canonical_root.join("app.asar.pre-ntc").exists()
        || canonical_root.join("app.asar.ntc-new").exists()
        || canonical_root.join(NTC_COMMIT_FILE).exists()
        || canonical_root.join(NTC_COMMIT_TEMP_FILE).exists()
        || canonical_root.join(NTC_RESTORE_FILE).exists()
    {
        eprintln!(
            "ntc_ensure_failed: NTC state does not match app.asar; refusing to overwrite externally changed data. Inspect app.asar.pre-ntc and ntc-commit.json, restore the intended app.asar manually, then remove the stale transaction files."
        );
        return ExitCode::from(4);
    }
    // Not injected: run the reapply path (which handles userscript fetch,
    // node invocation, backup and atomic swap).
    run_ntc_reapply(&canonical_root)
}

/// `ChatGPT-Fix-Manager ntc-reapply --fixture-root <path>`.
///
/// Upstream of the token-cost userscript (the version we verified against).
/// Download is best-effort at runtime; the plugin is optional and we never
/// redistribute the script inside the release — this is a local fetch.
const NTC_USERSCRIPT_URL: &str = "https://raw.githubusercontent.com/Tianzora/codex-token-cost/v0.7.9/scripts/codex-live-token-cost.js";
const NTC_COMMIT_SCHEMA: &str = "chatgpt_fix.ntc_commit.v1";
const NTC_COMMIT_FILE: &str = "ntc-commit.json";
const NTC_COMMIT_TEMP_FILE: &str = "ntc-commit.json.tmp";
const NTC_RESTORE_FILE: &str = "app.asar.ntc-restore";
const NTC_OWNERSHIP_MARKER: &str = "chatgpt-fix.fixture";
const NTC_OWNERSHIP_MARKER_CONTENT: &[u8] = b"launcher-owned-v1";
const NTC_TIMEOUT_DEFAULT_SECS: u64 = 60;
const NTC_TIMEOUT_MAX_SECS: u64 = 3_600;
const NTC_USERSCRIPT_DOWNLOAD_TIMEOUT_SECS: u64 = 60;
const NTC_USERSCRIPT_MAX_BYTES: u64 = 4 * 1024 * 1024;
const NTC_PROCESS_OUTPUT_MAX_BYTES: u64 = 1024 * 1024;
const NTC_PROCESS_TERMINATION_TIMEOUT_SECS: u64 = 5;
static NTC_TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Resolve only the absolute inbox Windows PowerShell helper. High-integrity
/// manager actions must never execute a PATH-controlled helper.
fn find_powershell() -> Result<PathBuf, String> {
    let path =
        PathBuf::from(std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".to_owned()))
            .join("System32")
            .join("WindowsPowerShell")
            .join("v1.0")
            .join("powershell.exe");
    if path.is_file() {
        Ok(path)
    } else {
        Err(format!(
            "Windows PowerShell helper not found at {}",
            path.display()
        ))
    }
}

/// Ensure the token-cost userscript exists at `dst`:
///
/// 1) already present;
/// 2) a known local copy (user deployment / our cache);
/// 3) downloaded from the upstream GitHub tag.
///
/// Returns true when the file is available afterwards. Every candidate is
/// staged to a unique sibling, checked, flushed, and atomically renamed. Uses
/// bounded PowerShell for the fetch so the Rust code stays dependency-free.
fn ensure_userscript(root: &Path, dst: &Path) -> Result<bool, String> {
    let overlay = dst
        .parent()
        .ok_or_else(|| "userscript path has no parent directory".to_owned())?;
    validate_or_create_ntc_directory(root, overlay)?;
    validate_ntc_file_path(root, dst, false)?;
    if dst.exists() {
        validate_userscript(root, dst)?;
        return Ok(true);
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
        if !is_usable_userscript(&candidate)? {
            continue;
        }
        publish_local_userscript(root, &candidate, dst)?;
        return Ok(true);
    }
    if let Some(parent) = dst.parent() {
        validate_or_create_ntc_directory(root, parent)?;
    }
    // Download is skipped when CHATGPT_FIX_NTC_NO_DOWNLOAD is set (tests,
    // explicit offline preference). The caller's fail-closed check then
    // reports the missing userscript.
    if std::env::var_os("CHATGPT_FIX_NTC_NO_DOWNLOAD").is_some() {
        return Ok(false);
    }
    let temp_path = unique_sibling_temp(dst, "download")?;
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp_path)
        .map_err(|error| {
            format!(
                "cannot reserve temporary userscript {}: {error}",
                temp_path.display()
            )
        })?;
    let quoted = temp_path.to_string_lossy().replace('\'', "''");
    let result = (|| {
        let ps = find_powershell()?;
        let mut command = std::process::Command::new(&ps);
        command.args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            &format!(
                "Invoke-WebRequest -Uri '{}' -OutFile '{}' -UseBasicParsing",
                NTC_USERSCRIPT_URL, quoted
            ),
        ]);
        let output = run_process_with_timeout(
            command,
            Duration::from_secs(NTC_USERSCRIPT_DOWNLOAD_TIMEOUT_SECS),
            "PowerShell userscript download",
        )?;
        if !output.status.success() {
            return Ok(false);
        }
        validate_userscript(root, &temp_path)?;
        sync_file(&temp_path)?;
        fs::rename(&temp_path, dst).map_err(|error| {
            format!(
                "cannot atomically publish userscript {}: {error}",
                dst.display()
            )
        })?;
        validate_userscript(root, dst)?;
        Ok(true)
    })();
    if temp_path.exists() {
        let _ = fs::remove_file(&temp_path);
    }
    result
}

fn publish_local_userscript(root: &Path, source: &Path, dst: &Path) -> Result<(), String> {
    let temp_path = unique_sibling_temp(dst, "copy")?;
    let result = (|| {
        let mut input = fs::File::open(source).map_err(|error| {
            format!("cannot open local userscript {}: {error}", source.display())
        })?;
        let mut output = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp_path)
            .map_err(|error| {
                format!(
                    "cannot create temporary userscript {}: {error}",
                    temp_path.display()
                )
            })?;
        std::io::copy(&mut input, &mut output).map_err(|error| {
            format!(
                "cannot stage local userscript {}: {error}",
                source.display()
            )
        })?;
        output
            .sync_all()
            .map_err(|error| format!("cannot flush {}: {error}", temp_path.display()))?;
        drop(output);
        validate_userscript(root, &temp_path)?;
        fs::rename(&temp_path, dst).map_err(|error| {
            format!(
                "cannot atomically publish userscript {}: {error}",
                dst.display()
            )
        })?;
        validate_userscript(root, dst)
    })();
    if result.is_err() && temp_path.exists() {
        let _ = fs::remove_file(&temp_path);
    }
    result
}

fn unique_sibling_temp(dst: &Path, operation: &str) -> Result<PathBuf, String> {
    let parent = dst
        .parent()
        .ok_or_else(|| "userscript path has no parent directory".to_owned())?;
    let name = dst
        .file_name()
        .ok_or_else(|| "userscript path has no file name".to_owned())?
        .to_string_lossy();
    for _ in 0..32 {
        let serial = NTC_TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let candidate = parent.join(format!(
            ".{name}.tmp-{operation}-{}-{serial}",
            std::process::id()
        ));
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    Err("cannot allocate a unique temporary userscript path".to_owned())
}

fn is_usable_userscript(path: &Path) -> Result<bool, String> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(format!("cannot inspect {}: {error}", path.display())),
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Ok(false);
    }
    if metadata.len() == 0 || metadata.len() > NTC_USERSCRIPT_MAX_BYTES {
        return Ok(false);
    }
    let bytes = fs::read(path)
        .map_err(|error| format!("cannot read userscript {}: {error}", path.display()))?;
    Ok(!bytes.contains(&0) && bytes.iter().any(|byte| !byte.is_ascii_whitespace()))
}

fn validate_userscript(root: &Path, path: &Path) -> Result<(), String> {
    validate_ntc_file_path(root, path, true)?;
    if !is_usable_userscript(path)? {
        return Err(format!(
            "userscript must be a non-empty regular text file no larger than {} bytes: {}",
            NTC_USERSCRIPT_MAX_BYTES,
            path.display()
        ));
    }
    Ok(())
}

/// Resolve the packaged native-token-cost injector. Production installs keep
/// the Manager at `<install>/bin/` and the injector at `<install>/scripts/`,
/// so this derives the path from the running executable rather than the build
/// workspace. `CHATGPT_FIX_NTC_INJECT_SCRIPT` is an explicit test/operations
/// override and must name an existing regular file.
fn resolve_ntc_inject_script_with_override(
    executable: &Path,
    override_path: Option<std::ffi::OsString>,
) -> Result<PathBuf, String> {
    let script = match override_path {
        Some(override_path) if !override_path.is_empty() => PathBuf::from(override_path),
        Some(_) => {
            return Err(
                "CHATGPT_FIX_NTC_INJECT_SCRIPT is set but empty; expected an injector file path"
                    .to_owned(),
            );
        }
        None => {
            let bin_dir = executable.parent().ok_or_else(|| {
                format!(
                    "cannot resolve Manager bin directory from executable {}",
                    executable.display()
                )
            })?;
            let install_root = bin_dir.parent().ok_or_else(|| {
                format!(
                    "cannot resolve Manager install root from executable {}",
                    executable.display()
                )
            })?;
            install_root
                .join("scripts")
                .join("inject-native-token-cost.js")
        }
    };

    if !script.is_file() {
        return Err(format!(
            "inject script is not an existing regular file: {}",
            script.display()
        ));
    }
    Ok(script)
}

fn resolve_ntc_inject_script(executable: &Path) -> Result<PathBuf, String> {
    resolve_ntc_inject_script_with_override(
        executable,
        std::env::var_os("CHATGPT_FIX_NTC_INJECT_SCRIPT"),
    )
}

/// Re-applies the native token-cost overlay to a launcher-owned fixture.
/// The commit marker is deliberately written last: it is the sole proof that
/// the backup, replacement, and resulting hashes were all verified.
fn run_ntc_reapply(root: &Path) -> ExitCode {
    let root = match validate_ntc_fixture_root(root) {
        Ok(root) => root,
        Err(error) => {
            eprintln!("ntc_reapply_failed: {error}");
            return ExitCode::from(3);
        }
    };

    let in_asar = root.join("app.asar");
    let userscript = root.join("ntc-overlay").join("codex-live-token-cost.js");
    let out_asar = root.join("app.asar.ntc-new");
    let backup_path = root.join("app.asar.pre-ntc");
    let marker_path = root.join(NTC_COMMIT_FILE);
    let marker_temp_path = root.join(NTC_COMMIT_TEMP_FILE);
    let restore_path = root.join(NTC_RESTORE_FILE);

    if let Err(error) = validate_ntc_file_path(&root, &in_asar, true) {
        eprintln!("ntc_reapply_failed: {error}");
        return ExitCode::from(3);
    }
    for path in [
        &backup_path,
        &marker_path,
        &out_asar,
        &marker_temp_path,
        &restore_path,
    ] {
        if let Err(error) = validate_ntc_file_path(&root, path, false) {
            eprintln!("ntc_reapply_failed: {error}");
            return ExitCode::from(4);
        }
    }
    if backup_path.exists()
        || marker_path.exists()
        || out_asar.exists()
        || marker_temp_path.exists()
        || restore_path.exists()
    {
        eprintln!("ntc_reapply_failed: existing NTC transaction state is not safe to replace");
        return ExitCode::from(4);
    }
    let userscript_ready = match ensure_userscript(&root, &userscript) {
        Ok(ready) => ready,
        Err(error) => {
            eprintln!("ntc_reapply_failed: {error}");
            return ExitCode::from(4);
        }
    };
    if !userscript_ready {
        eprintln!(
            "ntc_reapply_failed: userscript unavailable locally and download is disabled or failed in {}",
            root.display()
        );
        return ExitCode::from(3);
    }

    let executable = match std::env::current_exe() {
        Ok(path) => path,
        Err(error) => {
            eprintln!("ntc_reapply_failed: cannot locate manager executable: {error}");
            return ExitCode::from(4);
        }
    };
    let script = match resolve_ntc_inject_script(&executable) {
        Ok(path) => path,
        Err(error) => {
            eprintln!("ntc_reapply_failed: {error}");
            return ExitCode::from(4);
        }
    };
    if !script.is_file() {
        eprintln!(
            "ntc_reapply_failed: inject script is no longer a regular file: {}",
            script.display()
        );
        return ExitCode::from(4);
    }

    let timeout = match ntc_timeout() {
        Ok(timeout) => timeout,
        Err(error) => {
            eprintln!("ntc_reapply_failed: {error}");
            return ExitCode::from(4);
        }
    };
    let node = std::env::var("CHATGPT_FIX_NODE").unwrap_or_else(|_| "node".to_owned());
    let mut command = std::process::Command::new(&node);
    if let Ok(node_path) = std::env::var("NODE_PATH") {
        command.env("NODE_PATH", node_path);
    }
    command
        .arg(&script)
        .arg(&in_asar)
        .arg(&userscript)
        .arg(&out_asar)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let unpacked_dir = root.join("app.asar.unpacked");
    if let Err(error) = validate_ntc_directory_path(&root, &unpacked_dir, false) {
        eprintln!("ntc_reapply_failed: {error}");
        return ExitCode::from(4);
    }
    if unpacked_dir.is_dir() {
        command.arg(&unpacked_dir);
    }

    let output = match run_node_with_timeout(command, timeout) {
        Ok(output) => output,
        Err(error) => {
            let _ = fs::remove_file(&out_asar);
            eprintln!("ntc_reapply_failed: {error}");
            return ExitCode::from(4);
        }
    };
    if !output.status.success() {
        let _ = fs::remove_file(&out_asar);
        eprintln!(
            "ntc_reapply_failed: inject script exit {}: {}",
            output.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&output.stderr).trim()
        );
        return ExitCode::from(4);
    }

    let (before, after) = match parse_ntc_receipt(String::from_utf8_lossy(&output.stdout).trim()) {
        Some(value) => value,
        None => {
            let _ = fs::remove_file(&out_asar);
            eprintln!("ntc_reapply_failed: unexpected inject output");
            return ExitCode::from(4);
        }
    };
    let before = match chatgpt_fix_core::Sha256Digest::parse(&before) {
        Ok(digest) => digest.to_string(),
        Err(_) => {
            let _ = fs::remove_file(&out_asar);
            eprintln!("ntc_reapply_failed: inject receipt has an invalid before_sha256");
            return ExitCode::from(4);
        }
    };
    let after = match chatgpt_fix_core::Sha256Digest::parse(&after) {
        Ok(digest) => digest.to_string(),
        Err(_) => {
            let _ = fs::remove_file(&out_asar);
            eprintln!("ntc_reapply_failed: inject receipt has an invalid after_sha256");
            return ExitCode::from(4);
        }
    };
    let current_before = match ntc_file_hash(&in_asar) {
        Ok(hash) => hash,
        Err(error) => {
            let _ = fs::remove_file(&out_asar);
            eprintln!("ntc_reapply_failed: {error}");
            return ExitCode::from(4);
        }
    };
    let staged_after = match ntc_file_hash(&out_asar) {
        Ok(hash) => hash,
        Err(error) => {
            let _ = fs::remove_file(&out_asar);
            eprintln!("ntc_reapply_failed: {error}");
            return ExitCode::from(4);
        }
    };
    if current_before != before || staged_after != after {
        let _ = fs::remove_file(&out_asar);
        eprintln!("ntc_reapply_failed: inject receipt hashes do not match staged artifacts");
        return ExitCode::from(4);
    }

    if let Err(error) = fs::copy(&in_asar, &backup_path) {
        let _ = fs::remove_file(&out_asar);
        eprintln!("ntc_reapply_failed: cannot back up app.asar: {error}");
        return ExitCode::from(4);
    }
    let manifest = chatgpt_fix_core::NtcManifestV1 {
        artifact: "app.asar".to_owned(),
        before_sha256: before.clone(),
        after_sha256: after.clone(),
        backup_ref: backup_path.to_string_lossy().replace('\\', "/"),
        generation: 1,
        reapply_state: chatgpt_fix_core::NtcReapplyState::Clean,
        helper_health: "reapply-ok".to_owned(),
        created_at_utc: chatgpt_fix_core::utc_now_rfc3339(),
    };
    let manifest_json = match manifest.to_json() {
        Ok(json) => json,
        Err(error) => {
            let _ = fs::remove_file(&backup_path);
            let _ = fs::remove_file(&out_asar);
            eprintln!("invalid ntc manifest: {error}");
            return ExitCode::from(4);
        }
    };

    if !matches!(ntc_file_hash(&backup_path), Ok(actual) if actual == before) {
        let _ = fs::remove_file(&backup_path);
        let _ = fs::remove_file(&out_asar);
        eprintln!("ntc_reapply_failed: backup hash did not verify; transaction state cleaned");
        return ExitCode::from(4);
    }
    if let Err(error) = fs::rename(&out_asar, &in_asar) {
        let cleanup =
            cleanup_uncommitted_ntc(&in_asar, &backup_path, &out_asar, &marker_path, &before);
        eprintln!("ntc_reapply_failed: cannot atomically replace app.asar: {error}; {cleanup}");
        return ExitCode::from(4);
    }
    if !matches!(ntc_file_hash(&in_asar), Ok(actual) if actual == after) {
        let recovery = recover_ntc_transaction(
            &root,
            &in_asar,
            &backup_path,
            &out_asar,
            &marker_path,
            &before,
            &after,
        );
        eprintln!("ntc_reapply_failed: committed app.asar hash did not verify; {recovery}");
        return ExitCode::from(4);
    }
    if let Err(error) = write_ntc_commit(&root, &before, &after) {
        let recovery = recover_ntc_transaction(
            &root,
            &in_asar,
            &backup_path,
            &out_asar,
            &marker_path,
            &before,
            &after,
        );
        eprintln!("ntc_reapply_failed: cannot commit NTC receipt: {error}; {recovery}");
        return ExitCode::from(4);
    }

    println!("{manifest_json}");
    ExitCode::SUCCESS
}

fn cleanup_uncommitted_ntc(
    in_asar: &Path,
    backup_path: &Path,
    out_asar: &Path,
    marker_path: &Path,
    before: &str,
) -> String {
    let current_is_original = matches!(ntc_file_hash(in_asar), Ok(actual) if actual == before);
    if !current_is_original {
        return "current app.asar changed unexpectedly; backup preserved for manual recovery"
            .to_owned();
    }
    let _ = fs::remove_file(out_asar);
    let _ = fs::remove_file(marker_path);
    let _ = fs::remove_file(marker_path.with_extension("json.tmp"));
    match fs::remove_file(backup_path) {
        Ok(()) => "original app.asar remained intact and temporary transaction state was cleaned"
            .to_owned(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            "original app.asar remained intact and no backup cleanup was needed".to_owned()
        }
        Err(error) => {
            format!("original app.asar remained intact but backup cleanup failed: {error}")
        }
    }
}

fn recover_ntc_transaction(
    root: &Path,
    in_asar: &Path,
    backup_path: &Path,
    out_asar: &Path,
    marker_path: &Path,
    before: &str,
    after: &str,
) -> String {
    let backup_hash = ntc_file_hash(backup_path).ok();
    let current_hash = ntc_file_hash(in_asar).ok();
    if backup_hash.as_deref() != Some(before) {
        return "backup hash no longer matches the original; transaction files preserved for manual recovery".to_owned();
    }
    if current_hash.as_deref() == Some(before) {
        return cleanup_uncommitted_ntc(in_asar, backup_path, out_asar, marker_path, before);
    }
    if current_hash.as_deref() != Some(after) {
        return "current app.asar was externally modified; backup and transaction files preserved for manual recovery".to_owned();
    }

    let restore_path = root.join(NTC_RESTORE_FILE);
    if let Err(error) = validate_ntc_file_path(root, &restore_path, false) {
        return format!(
            "automatic recovery refused an unsafe restore path: {error}; backup preserved at {}",
            backup_path.display()
        );
    }
    if restore_path.exists() {
        return format!(
            "automatic recovery found pre-existing state at {}; backup preserved for manual recovery",
            restore_path.display()
        );
    }
    let restore_result = (|| -> Result<(), String> {
        fs::copy(backup_path, &restore_path)
            .map_err(|error| format!("cannot stage original app.asar for recovery: {error}"))?;
        sync_file(&restore_path)?;
        if ntc_file_hash(&restore_path).ok().as_deref() != Some(before) {
            return Err("staged recovery hash does not match the original".to_owned());
        }
        fs::rename(&restore_path, in_asar)
            .map_err(|error| format!("cannot atomically restore original app.asar: {error}"))?;
        if ntc_file_hash(in_asar).ok().as_deref() != Some(before) {
            return Err("restored app.asar hash does not match the original".to_owned());
        }
        Ok(())
    })();

    match restore_result {
        Ok(()) => {
            let _ = fs::remove_file(backup_path);
            let _ = fs::remove_file(out_asar);
            let _ = fs::remove_file(marker_path);
            let _ = fs::remove_file(root.join(NTC_COMMIT_TEMP_FILE));
            "original app.asar restored and temporary transaction state cleaned".to_owned()
        }
        Err(error) => {
            let _ = fs::remove_file(&restore_path);
            format!(
                "automatic recovery failed: {error}; backup preserved at {}",
                backup_path.display()
            )
        }
    }
}

fn sync_file(path: &Path) -> Result<(), String> {
    fs::OpenOptions::new()
        .write(true)
        .open(path)
        .and_then(|file| file.sync_all())
        .map_err(|error| format!("cannot flush {}: {error}", path.display()))
}

struct NtcProcessOutput {
    status: std::process::ExitStatus,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

fn validate_ntc_fixture_root(root: &Path) -> Result<PathBuf, String> {
    let metadata = fs::symlink_metadata(root)
        .map_err(|error| format!("fixture root cannot be read: {error}"))?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(
            "fixture root must be a non-symlink directory owned by the launcher".to_owned(),
        );
    }
    let canonical_root = fs::canonicalize(root)
        .map_err(|error| format!("fixture root cannot be canonicalized: {error}"))?;
    let normalized = canonical_root
        .to_string_lossy()
        .replace('/', "\\")
        .to_ascii_lowercase();
    if normalized.contains("\\windowsapps\\") {
        return Err("refusing to modify an official WindowsApps package".to_owned());
    }
    let ownership_marker = canonical_root.join(NTC_OWNERSHIP_MARKER);
    let marker = fs::symlink_metadata(&ownership_marker).map_err(|_| {
        "fixture root is not launcher-owned (chatgpt-fix.fixture is missing)".to_owned()
    })?;
    if !marker.is_file() || marker.file_type().is_symlink() {
        return Err("fixture root ownership marker must be a regular file".to_owned());
    }
    let marker_content = fs::read(&ownership_marker)
        .map_err(|error| format!("fixture root ownership marker cannot be read: {error}"))?;
    if marker_content != NTC_OWNERSHIP_MARKER_CONTENT {
        return Err(
            "fixture root ownership marker must contain exactly launcher-owned-v1".to_owned(),
        );
    }
    Ok(canonical_root)
}

fn validate_ntc_parent(root: &Path, path: &Path) -> Result<(), String> {
    let canonical_root = fs::canonicalize(root).map_err(|error| {
        format!(
            "cannot canonicalize fixture root {}: {error}",
            root.display()
        )
    })?;
    let parent = path
        .parent()
        .ok_or_else(|| format!("{} has no parent directory", path.display()))?;
    let canonical_parent = fs::canonicalize(parent)
        .map_err(|error| format!("cannot canonicalize parent of {}: {error}", path.display()))?;
    if !canonical_parent.starts_with(&canonical_root) {
        return Err(format!(
            "{} escapes the launcher-owned fixture root",
            path.display()
        ));
    }
    Ok(())
}

fn validate_ntc_file_path(root: &Path, path: &Path, required: bool) -> Result<(), String> {
    validate_ntc_parent(root, path)?;
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => Err(format!(
            "{} must be a regular non-symlink file",
            path.display()
        )),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound && !required => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            Err(format!("{} not found", path.display()))
        }
        Err(error) => Err(format!("cannot inspect {}: {error}", path.display())),
    }
}

fn validate_ntc_directory_path(root: &Path, path: &Path, required: bool) -> Result<(), String> {
    validate_ntc_parent(root, path)?;
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => Err(format!(
            "{} must be a non-symlink directory",
            path.display()
        )),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound && !required => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            Err(format!("{} not found", path.display()))
        }
        Err(error) => Err(format!("cannot inspect {}: {error}", path.display())),
    }
}

fn validate_or_create_ntc_directory(root: &Path, path: &Path) -> Result<(), String> {
    if !path.exists() {
        validate_ntc_parent(root, path)?;
        fs::create_dir(path)
            .map_err(|error| format!("cannot create {}: {error}", path.display()))?;
    }
    validate_ntc_directory_path(root, path, true)
}

fn ntc_timeout() -> Result<Duration, String> {
    let value = match std::env::var("CHATGPT_FIX_NTC_TIMEOUT_SECS") {
        Ok(value) => value,
        Err(std::env::VarError::NotPresent) => {
            return Ok(Duration::from_secs(NTC_TIMEOUT_DEFAULT_SECS));
        }
        Err(error) => return Err(format!("cannot read CHATGPT_FIX_NTC_TIMEOUT_SECS: {error}")),
    };
    let seconds = value.parse::<u64>().map_err(|_| {
        "CHATGPT_FIX_NTC_TIMEOUT_SECS must be an integer number of seconds".to_owned()
    })?;
    if !(1..=NTC_TIMEOUT_MAX_SECS).contains(&seconds) {
        return Err(format!(
            "CHATGPT_FIX_NTC_TIMEOUT_SECS must be in 1..={NTC_TIMEOUT_MAX_SECS}"
        ));
    }
    Ok(Duration::from_secs(seconds))
}

fn run_node_with_timeout(
    command: std::process::Command,
    timeout: Duration,
) -> Result<NtcProcessOutput, String> {
    run_process_with_timeout(command, timeout, "node")
}

fn run_process_with_timeout(
    mut command: std::process::Command,
    timeout: Duration,
    label: &str,
) -> Result<NtcProcessOutput, String> {
    let serial = NTC_TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let prefix = std::env::temp_dir().join(format!(
        "chatgpt-fix-manager-{}-{}-{}",
        std::process::id(),
        label,
        serial
    ));
    let stdout_path = prefix.with_extension("stdout");
    let stderr_path = prefix.with_extension("stderr");
    let result = (|| {
        let stdout_file = fs::File::create(&stdout_path)
            .map_err(|error| format!("cannot capture {label} stdout: {error}"))?;
        let stderr_file = fs::File::create(&stderr_path)
            .map_err(|error| format!("cannot capture {label} stderr: {error}"))?;
        command
            .stdout(Stdio::from(stdout_file))
            .stderr(Stdio::from(stderr_file));
        let mut child = command
            .spawn()
            .map_err(|error| format!("cannot run {label}: {error}"))?;
        let started = Instant::now();
        let status = loop {
            if let Some(status) = child
                .try_wait()
                .map_err(|error| format!("cannot poll {label}: {error}"))?
            {
                break status;
            }
            if started.elapsed() >= timeout {
                let tree_result = terminate_process_tree(child.id());
                if tree_result.is_err() {
                    let _ = child.kill();
                }
                let cleanup_started = Instant::now();
                let exited = loop {
                    match child.try_wait() {
                        Ok(Some(_)) => break true,
                        Ok(None)
                            if cleanup_started.elapsed()
                                < Duration::from_secs(NTC_PROCESS_TERMINATION_TIMEOUT_SECS) =>
                        {
                            std::thread::sleep(Duration::from_millis(25));
                        }
                        Ok(None) => break false,
                        Err(_) => break false,
                    }
                };
                let tree_detail = tree_result
                    .err()
                    .map(|error| format!("; process-tree termination failed: {error}"))
                    .unwrap_or_default();
                let exit_detail = if exited {
                    String::new()
                } else {
                    format!(
                        "; process exit could not be confirmed within {} seconds",
                        NTC_PROCESS_TERMINATION_TIMEOUT_SECS
                    )
                };
                return Err(format!(
                    "{label} timed out after {} seconds{tree_detail}{exit_detail}",
                    timeout.as_secs()
                ));
            }
            std::thread::sleep(Duration::from_millis(25));
        };
        Ok(NtcProcessOutput {
            status,
            stdout: read_capped_output(&stdout_path, label, "stdout")?,
            stderr: read_capped_output(&stderr_path, label, "stderr")?,
        })
    })();
    let _ = fs::remove_file(&stdout_path);
    let _ = fs::remove_file(&stderr_path);
    result
}

fn read_capped_output(path: &Path, label: &str, stream: &str) -> Result<Vec<u8>, String> {
    let metadata =
        fs::metadata(path).map_err(|error| format!("cannot inspect {label} {stream}: {error}"))?;
    if metadata.len() > NTC_PROCESS_OUTPUT_MAX_BYTES {
        return Err(format!(
            "{label} {stream} exceeded {} bytes",
            NTC_PROCESS_OUTPUT_MAX_BYTES
        ));
    }
    fs::read(path).map_err(|error| format!("cannot read {label} {stream}: {error}"))
}

fn terminate_process_tree(pid: u32) -> Result<(), String> {
    #[cfg(windows)]
    {
        let mut killer = std::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| format!("cannot terminate timed-out node: {error}"))?;
        let started = Instant::now();
        let status = loop {
            if let Some(status) = killer
                .try_wait()
                .map_err(|error| format!("cannot poll taskkill: {error}"))?
            {
                break status;
            }
            if started.elapsed() >= Duration::from_secs(NTC_PROCESS_TERMINATION_TIMEOUT_SECS) {
                let _ = killer.kill();
                return Err(format!(
                    "taskkill did not exit within {} seconds",
                    NTC_PROCESS_TERMINATION_TIMEOUT_SECS
                ));
            }
            std::thread::sleep(Duration::from_millis(25));
        };
        if !status.success() {
            return Err(format!(
                "taskkill failed while terminating timed-out node (exit {})",
                status.code().unwrap_or(-1)
            ));
        }
    }
    #[cfg(not(windows))]
    {
        let mut killer = std::process::Command::new("kill")
            .args(["-KILL", &pid.to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| format!("cannot terminate timed-out node: {error}"))?;
        let started = Instant::now();
        let status = loop {
            if let Some(status) = killer
                .try_wait()
                .map_err(|error| format!("cannot poll kill: {error}"))?
            {
                break status;
            }
            if started.elapsed() >= Duration::from_secs(NTC_PROCESS_TERMINATION_TIMEOUT_SECS) {
                let _ = killer.kill();
                return Err(format!(
                    "kill did not exit within {} seconds",
                    NTC_PROCESS_TERMINATION_TIMEOUT_SECS
                ));
            }
            std::thread::sleep(Duration::from_millis(25));
        };
        if !status.success() {
            return Err(format!(
                "kill failed while terminating timed-out node (exit {})",
                status.code().unwrap_or(-1)
            ));
        }
    }
    Ok(())
}

fn ntc_file_hash(path: &Path) -> Result<String, String> {
    let bytes =
        fs::read(path).map_err(|error| format!("cannot hash {}: {error}", path.display()))?;
    Ok(chatgpt_fix_core::sha256_bytes(&bytes).to_string())
}

fn ntc_commit_is_current(root: &Path) -> Result<bool, String> {
    let app_path = root.join("app.asar");
    let backup_path = root.join("app.asar.pre-ntc");
    let marker_path = root.join(NTC_COMMIT_FILE);
    let staged_path = root.join("app.asar.ntc-new");
    let temp_path = root.join(NTC_COMMIT_TEMP_FILE);
    let restore_path = root.join(NTC_RESTORE_FILE);
    for path in [
        &app_path,
        &backup_path,
        &marker_path,
        &staged_path,
        &temp_path,
        &restore_path,
    ] {
        validate_ntc_file_path(root, path, false)?;
    }
    if !backup_path.exists()
        || !marker_path.exists()
        || staged_path.exists()
        || temp_path.exists()
        || restore_path.exists()
    {
        return Ok(false);
    }
    let Some((before, after)) = read_ntc_commit(root)? else {
        return Ok(false);
    };
    Ok(ntc_file_hash(&backup_path)? == before && ntc_file_hash(&app_path)? == after)
}

fn read_ntc_commit(root: &Path) -> Result<Option<(String, String)>, String> {
    let path = root.join(NTC_COMMIT_FILE);
    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("cannot read {}: {error}", path.display())),
    };
    let text =
        std::str::from_utf8(&bytes).map_err(|_| "NTC commit receipt is not UTF-8".to_owned())?;
    let fields =
        parse_ntc_commit_json(text).ok_or_else(|| "NTC commit receipt is invalid".to_owned())?;
    if fields.0 != NTC_COMMIT_SCHEMA || fields.1 != "app.asar" {
        return Err("NTC commit receipt has an unexpected schema or artifact".to_owned());
    }
    let before = chatgpt_fix_core::Sha256Digest::parse(&fields.2)
        .map_err(|_| "NTC commit receipt has an invalid before_sha256".to_owned())?;
    let after = chatgpt_fix_core::Sha256Digest::parse(&fields.3)
        .map_err(|_| "NTC commit receipt has an invalid after_sha256".to_owned())?;
    Ok(Some((before.to_string(), after.to_string())))
}

fn write_ntc_commit(root: &Path, before: &str, after: &str) -> Result<(), String> {
    let final_path = root.join(NTC_COMMIT_FILE);
    let temp_path = root.join(NTC_COMMIT_TEMP_FILE);
    let receipt = format!(
        "{{\"schema\":\"{NTC_COMMIT_SCHEMA}\",\"artifact\":\"app.asar\",\"before_sha256\":\"{before}\",\"after_sha256\":\"{after}\"}}"
    );
    if final_path.exists() {
        return Err("NTC commit receipt already exists; refusing to replace it".to_owned());
    }
    if temp_path.exists() {
        return Err(
            "temporary NTC commit receipt already exists; refusing to replace it".to_owned(),
        );
    }
    let result = (|| -> Result<(), String> {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp_path)
            .map_err(|error| format!("cannot create temporary NTC receipt: {error}"))?;
        file.write_all(receipt.as_bytes())
            .map_err(|error| format!("cannot write temporary NTC receipt: {error}"))?;
        file.sync_all()
            .map_err(|error| format!("cannot flush temporary NTC receipt: {error}"))?;
        drop(file);
        fs::rename(&temp_path, &final_path)
            .map_err(|error| format!("cannot atomically finalize NTC receipt: {error}"))
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp_path);
    }
    result
}

/// The commit receipt has a deliberately small, closed JSON grammar. This
/// rejects malformed JSON, duplicate keys, unexpected keys, and escape forms.
fn parse_ntc_commit_json(input: &str) -> Option<(String, String, String, String)> {
    let input = input.trim();
    let body = input.strip_prefix('{')?.strip_suffix('}')?.trim();
    let mut fields = std::collections::BTreeMap::new();
    for entry in body.split(',') {
        let (key, value) = entry.split_once(':')?;
        let key = key.trim().strip_prefix('\"')?.strip_suffix('\"')?;
        let value = value.trim().strip_prefix('\"')?.strip_suffix('\"')?;
        if key.is_empty()
            || value.contains('\"')
            || value.contains('\\')
            || fields.insert(key, value).is_some()
        {
            return None;
        }
    }
    if fields.len() != 4 {
        return None;
    }
    Some((
        fields.remove("schema")?.to_owned(),
        fields.remove("artifact")?.to_owned(),
        fields.remove("before_sha256")?.to_owned(),
        fields.remove("after_sha256")?.to_owned(),
    ))
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

#[cfg(test)]
mod tests {
    use super::{
        NTC_COMMIT_FILE, NTC_COMMIT_TEMP_FILE, NTC_RESTORE_FILE, cleanup_uncommitted_ntc,
        ntc_file_hash, read_ntc_commit, recover_ntc_transaction,
        resolve_ntc_inject_script_with_override, run_node_with_timeout, validate_ntc_parent,
        write_ntc_commit,
    };
    use std::ffi::OsString;
    use std::fs;
    use std::path::PathBuf;
    use std::process::{Command, Stdio};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{Duration, Instant};

    static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn isolated_root(name: &str) -> PathBuf {
        let serial = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "chatgpt-fix-manager-{name}-{}-{serial}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("create isolated test root");
        root
    }

    #[test]
    fn resolves_injector_from_isolated_install_layout() {
        let root = isolated_root("inject-layout");
        let executable = root.join("bin").join("ChatGPT-Fix-Manager.exe");
        let script = root.join("scripts").join("inject-native-token-cost.js");
        fs::create_dir_all(executable.parent().expect("executable parent"))
            .expect("create bin directory");
        fs::create_dir_all(script.parent().expect("script parent"))
            .expect("create scripts directory");
        fs::write(&executable, b"manager").expect("write manager executable fixture");
        fs::write(&script, b"// injector").expect("write injector fixture");

        // The default is derived from the executable's parent, so this
        // mirrors an installed layout without platform-specific separators.
        assert_eq!(
            resolve_ntc_inject_script_with_override(&executable, None)
                .expect("resolve packaged injector"),
            script
        );
    }

    #[test]
    fn injector_override_rejects_a_directory() {
        let root = isolated_root("inject-override");
        let executable = root.join("bin").join("manager");
        let override_dir = root.join("override-dir");
        fs::create_dir_all(&override_dir).expect("create override directory");

        let error = resolve_ntc_inject_script_with_override(
            &executable,
            Some(override_dir.into_os_string()),
        )
        .expect_err("directory must not be accepted as an injector");

        assert!(error.contains("existing regular file"), "error: {error}");
    }

    #[test]
    fn injector_override_accepts_an_existing_regular_file() {
        let root = isolated_root("inject-file-override");
        let executable = root.join("bin").join("manager");
        let override_file = root.join("custom-injector.js");
        fs::write(&override_file, b"// custom injector").expect("write override fixture");

        assert_eq!(
            resolve_ntc_inject_script_with_override(
                &executable,
                Some(override_file.clone().into_os_string()),
            )
            .expect("accept file override"),
            override_file
        );
    }

    #[test]
    fn injector_override_rejects_an_empty_value() {
        let executable = PathBuf::from("install/bin/manager");
        let error = resolve_ntc_inject_script_with_override(&executable, Some(OsString::new()))
            .expect_err("empty override must be rejected");

        assert!(error.contains("set but empty"), "error: {error}");
    }

    #[test]
    fn cleanup_uncommitted_ntc_removes_only_new_state_when_original_is_intact() {
        let root = isolated_root("cleanup-intact");
        let in_asar = root.join("app.asar");
        let backup = root.join("app.asar.pre-ntc");
        let out = root.join("app.asar.ntc-new");
        let marker = root.join(NTC_COMMIT_FILE);
        let temp = root.join(NTC_COMMIT_TEMP_FILE);
        fs::write(&in_asar, b"before").expect("write original");
        fs::write(&backup, b"before").expect("write backup");
        fs::write(&out, b"after").expect("write staged output");
        fs::write(&marker, b"marker").expect("write marker");
        fs::write(&temp, b"temporary").expect("write temp marker");
        let before = ntc_file_hash(&in_asar).expect("hash original");

        let result = cleanup_uncommitted_ntc(&in_asar, &backup, &out, &marker, &before);

        assert!(result.contains("cleaned"), "result: {result}");
        assert_eq!(fs::read(&in_asar).expect("read original"), b"before");
        for path in [&backup, &out, &marker, &temp] {
            assert!(
                !path.exists(),
                "transaction path remains: {}",
                path.display()
            );
        }
    }

    #[test]
    fn cleanup_uncommitted_ntc_preserves_backup_when_original_drifted() {
        let root = isolated_root("cleanup-drift");
        let in_asar = root.join("app.asar");
        let backup = root.join("app.asar.pre-ntc");
        let out = root.join("app.asar.ntc-new");
        let marker = root.join(NTC_COMMIT_FILE);
        let temp = root.join(NTC_COMMIT_TEMP_FILE);
        fs::write(&in_asar, b"drifted").expect("write drifted app");
        fs::write(&backup, b"before").expect("write backup");
        fs::write(&out, b"after").expect("write staged output");
        fs::write(&marker, b"marker").expect("write marker");
        fs::write(&temp, b"temporary").expect("write temp marker");

        let result =
            cleanup_uncommitted_ntc(&in_asar, &backup, &out, &marker, "00".repeat(32).as_str());

        assert!(result.contains("manual recovery"), "result: {result}");
        for path in [&in_asar, &backup, &out, &marker, &temp] {
            assert!(
                path.exists(),
                "transaction path was removed: {}",
                path.display()
            );
        }
    }

    #[test]
    fn recover_ntc_transaction_restores_before_and_cleans_state() {
        let root = isolated_root("recover");
        let in_asar = root.join("app.asar");
        let backup = root.join("app.asar.pre-ntc");
        let out = root.join("app.asar.ntc-new");
        let marker = root.join(NTC_COMMIT_FILE);
        let temp = root.join(NTC_COMMIT_TEMP_FILE);
        fs::write(&in_asar, b"after").expect("write modified app");
        fs::write(&backup, b"before").expect("write backup");
        fs::write(&out, b"staged").expect("write staged output");
        fs::write(&marker, b"marker").expect("write marker");
        fs::write(&temp, b"temporary").expect("write temp marker");
        let before = ntc_file_hash(&backup).expect("hash before");
        let after = ntc_file_hash(&in_asar).expect("hash after");

        let result =
            recover_ntc_transaction(&root, &in_asar, &backup, &out, &marker, &before, &after);

        assert!(result.contains("restored"), "result: {result}");
        assert_eq!(fs::read(&in_asar).expect("read restored app"), b"before");
        for path in [&backup, &out, &marker, &temp, &root.join(NTC_RESTORE_FILE)] {
            assert!(
                !path.exists(),
                "transaction path remains: {}",
                path.display()
            );
        }
    }

    #[test]
    fn write_ntc_commit_round_trips_and_rejects_preexisting_temp() {
        let root = isolated_root("commit");
        let before = "11".repeat(32);
        let after = "22".repeat(32);

        write_ntc_commit(&root, &before, &after).expect("write receipt");
        assert_eq!(
            read_ntc_commit(&root).expect("read receipt"),
            Some((before.clone(), after.clone()))
        );
        assert!(!root.join(NTC_COMMIT_TEMP_FILE).exists());

        let second = write_ntc_commit(&root, &before, &after).expect_err("duplicate receipt");
        assert!(second.contains("already exists"), "error: {second}");

        let other_root = isolated_root("commit-temp");
        fs::write(other_root.join(NTC_COMMIT_TEMP_FILE), b"preserve").expect("write old temp");
        let error = write_ntc_commit(&other_root, &before, &after).expect_err("old temp");
        assert!(error.contains("temporary"), "error: {error}");
        assert_eq!(
            fs::read(other_root.join(NTC_COMMIT_TEMP_FILE)).expect("read old temp"),
            b"preserve"
        );
    }

    #[test]
    fn validate_ntc_parent_rejects_a_canonical_sibling() {
        let parent = isolated_root("parent-boundary");
        let root = parent.join("owned");
        let sibling = parent.join("sibling");
        fs::create_dir_all(&root).expect("create owned root");
        fs::create_dir_all(&sibling).expect("create sibling");

        let error = validate_ntc_parent(&root, &sibling.join("app.asar"))
            .expect_err("canonical sibling must be rejected");

        assert!(error.contains("escapes"), "error: {error}");
    }

    #[cfg(windows)]
    #[test]
    fn process_output_collection_is_not_held_open_by_a_descendant() {
        let system_root = std::env::var("SystemRoot").expect("SystemRoot");
        let powershell =
            PathBuf::from(system_root).join("System32/WindowsPowerShell/v1.0/powershell.exe");
        let mut command = Command::new(powershell);
        command
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "Start-Process -FilePath powershell.exe -ArgumentList '-NoProfile -NonInteractive -Command Start-Sleep -Seconds 3' -NoNewWindow",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let started = Instant::now();
        let output = run_node_with_timeout(command, Duration::from_secs(5))
            .expect("short-lived parent process");

        assert!(output.status.success(), "status: {:?}", output.status);
        assert!(
            started.elapsed() < Duration::from_millis(1_500),
            "output collection waited on inherited pipe handles for {:?}",
            started.elapsed()
        );
    }
}
