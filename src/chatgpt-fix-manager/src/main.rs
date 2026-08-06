use std::ffi::OsStr;
use std::io::Read;
use std::path::Path;
use std::process::ExitCode;

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
        _ => {
            print_usage();
            ExitCode::from(2)
        }
    }
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
}
