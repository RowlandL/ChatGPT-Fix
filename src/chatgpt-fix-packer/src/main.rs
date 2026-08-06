use std::ffi::OsStr;
use std::path::Path;
use std::process::ExitCode;

const PRODUCT_NAME: &str = "ChatGPT-Fix-Packer";

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
            eprintln!("live inspect requires A2-P2 authorization");
            ExitCode::from(2)
        }
        [command, option]
            if command == OsStr::new("plan") && option == OsStr::new("--live-readonly") =>
        {
            eprintln!("live plan requires A2-P2 authorization");
            ExitCode::from(2)
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
