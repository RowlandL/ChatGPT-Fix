use std::ffi::OsStr;
use std::process::ExitCode;

mod error;
mod json;
mod path;
mod schema;

pub use error::ContractError;
pub use path::{SafeRelativePath, Sha256Digest};
pub use schema::{
    BASELINE_SCHEMA, BaselineV2, LAUNCH_SCHEMA, LaunchV1, PLAN_SCHEMA, PlanAction, PlanActionKind,
    PlanDecision, PlanV1, RECEIPT_SCHEMA, ReceiptV1,
};

pub fn run_version_command(product_name: &str) -> ExitCode {
    let mut arguments = std::env::args_os().skip(1);

    match (arguments.next(), arguments.next()) {
        (Some(argument), None) if argument == OsStr::new("--version") => {
            println!("{product_name} {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        _ => {
            eprintln!("Usage: {product_name} --version");
            ExitCode::from(2)
        }
    }
}
