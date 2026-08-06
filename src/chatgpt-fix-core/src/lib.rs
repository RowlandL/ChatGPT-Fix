use std::ffi::OsStr;
use std::process::ExitCode;

mod doctor;
mod error;
mod fixture;
mod json;
mod path;
mod probe;
mod schema;
mod sha256;

pub use doctor::{DoctorStatus, DoctorLevel, DoctorFinding, DoctorV2, DOCTOR_SCHEMA};
pub use error::ContractError;
pub use fixture::{FIXTURE_SCHEMA, FixtureError, plan_fixture};
pub use path::{SafeRelativePath, Sha256Digest};
pub use probe::{inspect_probe_json, plan_from_probe, plan_probe_json};
pub use schema::{
    BASELINE_SCHEMA, BaselineV2, LAUNCH_SCHEMA, LaunchV1, LIVE_INSPECTION_SCHEMA, LiveInspectionV1,
    PLAN_SCHEMA, PlanAction, PlanActionKind, PlanDecision, PlanV1, RECEIPT_SCHEMA, ReceiptV1,
};
pub use sha256::sha256_bytes;

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
