use std::process::ExitCode;

const PRODUCT_NAME: &str = "ChatGPT-Fix-Setup";

/// P8 Setup binary (build-only).
///
/// `ChatGPT-Fix-Setup.exe` is a per-user local install/uninstall wrapper that
/// is *built* in P8 (A1 scope) but never *run*: running Setup requires A8a.
/// Until A8a is separately authorized this binary only answers `--version`;
/// any install action is rejected fail-closed.
fn main() -> ExitCode {
    let mut arguments = std::env::args_os().skip(1);
    match arguments.next() {
        Some(argument) if argument == std::ffi::OsStr::new("--version") => match arguments.next() {
            None => {
                println!("{PRODUCT_NAME} {}", env!("CARGO_PKG_VERSION"));
                ExitCode::SUCCESS
            }
            Some(_) => {
                eprintln!("Usage: {PRODUCT_NAME} --version");
                ExitCode::from(2)
            }
        },
        Some(_) | None => {
            // Any other invocation (including an actual install) is blocked:
            // running Setup requires A8a, which is not granted in P8.
            eprintln!("install_forbidden: running Setup requires A8a authorization");
            ExitCode::from(4)
        }
    }
}
