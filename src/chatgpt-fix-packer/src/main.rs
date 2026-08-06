use std::ffi::OsStr;
use std::path::Path;
use std::process::{Command, ExitCode, Stdio};

const PRODUCT_NAME: &str = "ChatGPT-Fix-Packer";

/// The P2 probe script, embedded at compile time.
const PROBE_SCRIPT: &str = include_str!("../../../scripts/chatgpt-fix-p2-probe.ps1");

/// The trailing invocation appended to the script for live mode.
const LIVE_INVOCATION: &str =
    "\nInvoke-ChatGptFixP2Probe -Mode Live | ConvertTo-Json -Compress -Depth 10\n";

fn is_a2_p2_authorized() -> bool {
    std::env::var(chatgpt_fix_core::A2_P2_ENV).is_ok()
}

fn main() -> ExitCode {
    let arguments = std::env::args_os().skip(1).collect::<Vec<_>>();

    match arguments.as_slice() {
        [argument] if argument == OsStr::new("--version") => {
            chatgpt_fix_core::run_version_command(PRODUCT_NAME)
        }
        [command, option, root]
            if command == OsStr::new("plan") && option == OsStr::new("--fixture-root") =>
        {
            print_fixture_plan(Path::new(root))
        }
        [command, option, path]
            if command == OsStr::new("inspect") && option == OsStr::new("--probe-json") =>
        {
            print_inspect_probe_json(Path::new(path))
        }
        [command, option, path]
            if command == OsStr::new("plan") && option == OsStr::new("--probe-json") =>
        {
            print_plan_probe_json(Path::new(path))
        }
        [command, option]
            if command == OsStr::new("inspect") && option == OsStr::new("--live-readonly") =>
        {
            run_live_inspect()
        }
        [command, option]
            if command == OsStr::new("plan") && option == OsStr::new("--live-readonly") =>
        {
            run_live_plan()
        }
        _ => {
            print_usage();
            ExitCode::from(2)
        }
    }
}

fn print_fixture_plan(root: &Path) -> ExitCode {
    match chatgpt_fix_core::plan_fixture(root).and_then(|plan| {
        plan.to_json()
            .map_err(|error| chatgpt_fix_core::FixtureError {
                code: "invalid_generated_plan".to_owned(),
                path: root.to_owned(),
                message: error.to_string(),
            })
    }) {
        Ok(json) => {
            println!("{json}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(3)
        }
    }
}

fn print_inspect_probe_json(path: &Path) -> ExitCode {
    match chatgpt_fix_core::inspect_probe_json(path).and_then(|inspection| {
        inspection
            .to_json()
            .map_err(|error| chatgpt_fix_core::FixtureError {
                code: "invalid_generated_inspection".to_owned(),
                path: path.to_owned(),
                message: error.to_string(),
            })
    }) {
        Ok(json) => {
            println!("{json}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(3)
        }
    }
}

fn print_plan_probe_json(path: &Path) -> ExitCode {
    match chatgpt_fix_core::plan_probe_json(path).and_then(|plan| {
        plan.to_json()
            .map_err(|error| chatgpt_fix_core::FixtureError {
                code: "invalid_generated_plan".to_owned(),
                path: path.to_owned(),
                message: error.to_string(),
            })
    }) {
        Ok(json) => {
            println!("{json}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(3)
        }
    }
}

fn print_usage() {
    eprintln!("Usage: {PRODUCT_NAME} --version");
    eprintln!("       {PRODUCT_NAME} plan --fixture-root <path>");
    eprintln!("       {PRODUCT_NAME} inspect --probe-json <path>");
    eprintln!("       {PRODUCT_NAME} plan --probe-json <path>");
    eprintln!("       {PRODUCT_NAME} inspect --live-readonly");
    eprintln!("       {PRODUCT_NAME} plan --live-readonly");
}

/// Run a live pwsh probe and return the canonical inspection JSON.
fn run_live_inspect() -> ExitCode {
    if !is_a2_p2_authorized() {
        eprintln!("live inspect requires A2-P2 authorization");
        return ExitCode::from(2);
    }

    let probe_bytes = run_live_probe().map_err(|err| {
        eprintln!("{err}");
        ExitCode::from(3)
    });
    let probe_bytes = match probe_bytes {
        Ok(b) => b,
        Err(code) => return code,
    };

    match chatgpt_fix_core::inspect_live_json(&probe_bytes) {
        Ok(inspection) => match inspection.to_json() {
            Ok(json) => {
                println!("{json}");
                ExitCode::SUCCESS
            }
            Err(err) => {
                eprintln!("inspection serialize failed: {err}");
                ExitCode::from(4)
            }
        },
        Err(err) => {
            eprintln!("{err}");
            ExitCode::from(3)
        }
    }
}

/// Run a live pwsh probe and return the canonical plan JSON.
fn run_live_plan() -> ExitCode {
    if !is_a2_p2_authorized() {
        eprintln!("live plan requires A2-P2 authorization");
        return ExitCode::from(2);
    }

    let probe_bytes = run_live_probe().map_err(|err| {
        eprintln!("{err}");
        ExitCode::from(3)
    });
    let probe_bytes = match probe_bytes {
        Ok(b) => b,
        Err(code) => return code,
    };

    match chatgpt_fix_core::plan_live_json(&probe_bytes) {
        Ok(plan) => match plan.to_json() {
            Ok(json) => {
                println!("{json}");
                ExitCode::SUCCESS
            }
            Err(err) => {
                eprintln!("plan serialize failed: {err}");
                ExitCode::from(4)
            }
        },
        Err(err) => {
            eprintln!("{err}");
            ExitCode::from(3)
        }
    }
}

/// Spawn pwsh.exe, pass the probe script as a -Command argument,
/// and return stdout bytes.
fn run_live_probe() -> Result<Vec<u8>, String> {
    let full_script = format!("{PROBE_SCRIPT}{LIVE_INVOCATION}");
    let child = Command::new("pwsh.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", &full_script])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("failed to spawn pwsh.exe: {e}"))?;

    let output = child
        .wait_with_output()
        .map_err(|e| format!("failed to read pwsh output: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "pwsh probe failed (exit {})\nstderr: {}",
            output.status.code().unwrap_or(-1),
            stderr
        ));
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    if !stderr.is_empty() {
        return Err(format!("pwsh probe produced unexpected stderr: {stderr}"));
    }

    if output.stdout.is_empty() {
        return Err("pwsh probe produced no stdout".to_owned());
    }

    Ok(output.stdout)
}
