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

fn print_usage() {
    eprintln!("Usage: {PRODUCT_NAME} --version");
    eprintln!("       {PRODUCT_NAME} plan --fixture-root <path>");
}
