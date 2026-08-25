// The launcher is a Windows GUI-subsystem binary: double-clicking the
// shortcut must NOT flash a console window. It is invoked by the Start Menu
// shortcut with NO arguments, in which case it launches the active baseline
// from the default program root.
#![windows_subsystem = "windows"]

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use chatgpt_fix_core::{LaunchV1, PlanDecision, ReceiptV1, SafeRelativePath, sha256_bytes};

const PRODUCT_NAME: &str = "ChatGPT-Fix-Launcher";

/// Default program root: `%LOCALAPPDATA%\Programs\ChatGPT-Fix`.
fn default_program_root() -> Option<PathBuf> {
    std::env::var("LOCALAPPDATA")
        .ok()
        .filter(|v| !v.is_empty())
        .map(|base| PathBuf::from(base).join("Programs").join("ChatGPT-Fix"))
}

fn main() -> ExitCode {
    let arguments = std::env::args_os().skip(1).collect::<Vec<_>>();

    match arguments.as_slice() {
        // No arguments (double-click from the shortcut): launch the active
        // baseline from the default program root.
        [] => match default_program_root() {
            Some(root) => run_live_launch(&root),
            None => {
                eprintln!("launch_failed: LOCALAPPDATA is not set; cannot determine program root");
                ExitCode::from(4)
            }
        },
        [argument] if argument == OsStr::new("--version") => {
            chatgpt_fix_core::run_version_command(PRODUCT_NAME)
        }
        [command, option, root]
            if command == OsStr::new("dry-run") && option == OsStr::new("--fixture-root") =>
        {
            dry_run_fixture(Path::new(root))
        }
        [command, option, root]
            if command == OsStr::new("observe") && option == OsStr::new("--fixture-root") =>
        {
            observe_fixture(Path::new(root))
        }
        [command, option, root]
            if command == OsStr::new("shutdown") && option == OsStr::new("--fixture-root") =>
        {
            shutdown_fixture(Path::new(root))
        }
        [command, option, root]
            if command == OsStr::new("maintenance-plan")
                && option == OsStr::new("--fixture-root") =>
        {
            run_maintenance_plan(Path::new(root))
        }
        [command, option, root]
            if command == OsStr::new("launch") && option == OsStr::new("--live") =>
        {
            run_live_launch(Path::new(root))
        }
        _ => {
            print_usage();
            ExitCode::from(2)
        }
    }
}

/// `ChatGPT-Fix-Launcher launch --live <program-root>`.
///
/// Reads `<program-root>/current.json` (chatgpt_fix.pointer.v1), resolves the
/// baseline root, and launches `ChatGPT.exe` from that baseline. If the
/// pointer is missing or its baseline is gone, falls back to discovering the
/// official OpenAI.Codex package location via `Get-AppxPackage` so a package
/// update (new version directory) never breaks the launch. Writes a launch
/// receipt to stdout. A4a-authorized.
fn run_live_launch(program_root: &Path) -> ExitCode {
    if let Err(error) = chatgpt_fix_core::sanitize_codex_config() {
        // A stale optional config entry must never prevent the official app
        // from starting; the warning remains available to a console caller.
        eprintln!("config_sanitize_warning: {error}");
    }
    let (launch, spawned_pid) = match chatgpt_fix_core::launch_from_pointer(program_root) {
        Ok(pair) => pair,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::from(3);
        }
    };
    match launch.to_json() {
        Ok(json) => {
            println!("{json}");
            // Resident mode (plan P4/P5): when we actually spawned a new
            // instance, hold the Job and stay alive until the root child
            // exits. This keeps Job ownership for the app's whole lifetime.
            // The single-instance reuse path returns None and exits quickly.
            if let Some(pid) = spawned_pid {
                #[cfg(windows)]
                {
                    let _ = chatgpt_fix_core::win32::wait_for_process(pid);
                }
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("launch serialize failed: {error}");
            ExitCode::from(4)
        }
    }
}

/// `ChatGPT-Fix-Launcher observe --fixture-root <path>`.
///
/// Report-only ownership observation: parses the synthetic process tree and
/// emits a `chatgpt_fix.ownership.v1` receipt. No Job close, no graceful
/// shutdown, no terminate.
fn observe_fixture(root: &Path) -> ExitCode {
    match chatgpt_fix_core::observe_process_tree(root) {
        Ok(ownership) => match ownership.to_json() {
            Ok(json) => {
                println!("{json}");
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("invalid_ownership at {}: {error}", root.display());
                ExitCode::from(3)
            }
        },
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(3)
        }
    }
}

/// `ChatGPT-Fix-Launcher shutdown --fixture-root <path>`.
///
/// Controlled shutdown of an owned tree (A5-owned-shutdown scope): parses the
/// synthetic process tree and emits a `chatgpt_fix.shutdown.v1` receipt. Only
/// reconciled, in-Job, non-breakaway PIDs are handled; breakaway and
/// outside-Job PIDs are excluded; any suspected hit fails closed.
fn shutdown_fixture(root: &Path) -> ExitCode {
    match chatgpt_fix_core::shutdown_process_tree(root) {
        Ok(shutdown) => match shutdown.to_json() {
            Ok(json) => {
                println!("{json}");
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("invalid_shutdown at {}: {error}", root.display());
                ExitCode::from(3)
            }
        },
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(3)
        }
    }
}

fn dry_run_fixture(root: &Path) -> ExitCode {
    let plan = match chatgpt_fix_core::plan_fixture(root) {
        Ok(plan) => plan,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::from(3);
        }
    };

    if plan.decision != PlanDecision::Ready {
        eprintln!(
            "fixture_plan_rejected at {}: {}",
            root.join("chatgpt-fix.fixture").display(),
            plan.errors.join(",")
        );
        return ExitCode::from(3);
    }

    match render_dry_run(&plan) {
        Ok((launch_json, receipt_json)) => {
            println!("{launch_json}");
            println!("{receipt_json}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("invalid_dry_run at {}: {error}", root.display());
            ExitCode::from(3)
        }
    }
}

fn render_dry_run(plan: &chatgpt_fix_core::PlanV1) -> Result<(String, String), String> {
    let baseline = plan
        .baseline
        .as_ref()
        .ok_or_else(|| "ready plan is missing its baseline".to_owned())?;
    let baseline_id = SafeRelativePath::parse(&baseline.baseline_id)
        .map_err(|error| format!("invalid baseline id: {error}"))?;
    let executable =
        SafeRelativePath::parse(&format!("baselines/{}/ChatGPT.exe", baseline_id.as_str()))
            .map_err(|error| format!("invalid executable path: {error}"))?;
    let plan_json = plan
        .to_json()
        .map_err(|error| format!("invalid plan: {error}"))?;

    let launch = LaunchV1 {
        launch_id: format!("fixture-{}", plan.fixture_id),
        generation: 1,
        baseline_id,
        executable,
        would_start: false,
        reason: Some("p1_offline_dry_run".to_owned()),
    };
    launch
        .validate()
        .map_err(|error| format!("invalid launch: {error}"))?;

    let receipt = ReceiptV1 {
        operation: "launch".to_owned(),
        status: "dry-run".to_owned(),
        plan_sha256: sha256_bytes(plan_json.as_bytes()),
        artifact: None,
        artifact_sha256: None,
        source_commit: option_env!("CHATGPT_FIX_SOURCE_COMMIT")
            .unwrap_or("unrecorded")
            .to_owned(),
        toolchain: "rustc-1.97.1-x86_64-pc-windows-msvc".to_owned(),
        signing_status: "unsigned".to_owned(),
    };
    receipt
        .validate()
        .map_err(|error| format!("invalid receipt: {error}"))?;

    let launch_json = launch
        .to_json()
        .map_err(|error| format!("invalid launch: {error}"))?;
    let receipt_json = receipt
        .to_json()
        .map_err(|error| format!("invalid receipt: {error}"))?;
    Ok((launch_json, receipt_json))
}

fn print_usage() {
    eprintln!("Usage: {PRODUCT_NAME} [no args: launch active baseline]");
    eprintln!("       {PRODUCT_NAME} --version");
    eprintln!("       {PRODUCT_NAME} dry-run --fixture-root <path>");
    eprintln!("       {PRODUCT_NAME} observe --fixture-root <path>");
    eprintln!("       {PRODUCT_NAME} shutdown --fixture-root <path>");
    eprintln!("       {PRODUCT_NAME} maintenance-plan --fixture-root <path>");
    eprintln!("       {PRODUCT_NAME} launch --live <path>");
}

/// `ChatGPT-Fix-Launcher maintenance-plan --fixture-root <path>`.
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
