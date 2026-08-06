use std::ffi::OsStr;
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
    eprintln!("       {PRODUCT_NAME} doctor");
}
