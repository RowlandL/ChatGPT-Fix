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

fn print_usage() {
    eprintln!("Usage: {PRODUCT_NAME} --version");
    eprintln!("       {PRODUCT_NAME} review --fixture-root <path>");
    eprintln!("       {PRODUCT_NAME} review --plan-stdin");
    eprintln!("       {PRODUCT_NAME} activate --baseline <root> [--smoke]");
    eprintln!("       {PRODUCT_NAME} rollback");
    eprintln!("       {PRODUCT_NAME} doctor");
}
