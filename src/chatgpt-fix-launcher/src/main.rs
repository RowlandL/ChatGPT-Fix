use std::ffi::OsStr;
use std::path::Path;
use std::process::ExitCode;

use chatgpt_fix_core::{LaunchV1, PlanDecision, ReceiptV1, SafeRelativePath, sha256_bytes};

const PRODUCT_NAME: &str = "ChatGPT-Fix-Launcher";

fn main() -> ExitCode {
    let arguments = std::env::args_os().skip(1).collect::<Vec<_>>();

    match arguments.as_slice() {
        [argument] if argument == OsStr::new("--version") => {
            chatgpt_fix_core::run_version_command(PRODUCT_NAME)
        }
        [command, option, root]
            if command == OsStr::new("dry-run") && option == OsStr::new("--fixture-root") =>
        {
            dry_run_fixture(Path::new(root))
        }
        [command, option, _]
            if command == OsStr::new("launch") && option == OsStr::new("--live") =>
        {
            eprintln!("live_launch_forbidden: P1 launcher does not start programs");
            ExitCode::from(4)
        }
        _ => {
            print_usage();
            ExitCode::from(2)
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
    eprintln!("Usage: {PRODUCT_NAME} --version");
    eprintln!("       {PRODUCT_NAME} dry-run --fixture-root <path>");
    eprintln!("       {PRODUCT_NAME} launch --live <path>");
}
