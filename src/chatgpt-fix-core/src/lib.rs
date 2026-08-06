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

pub use doctor::{DOCTOR_SCHEMA, DoctorFinding, DoctorLevel, DoctorStatus, DoctorV2};
pub use error::ContractError;
pub use fixture::{FIXTURE_SCHEMA, FixtureError, plan_fixture};
pub use path::{SafeRelativePath, Sha256Digest};
pub use probe::{
    A2_P2_ENV, inspect_live_json, inspect_probe_json, plan_from_probe, plan_live_json,
    plan_probe_json,
};
pub use schema::{
    BASELINE_SCHEMA, BaselineV2, LAUNCH_SCHEMA, LIVE_INSPECTION_SCHEMA, LaunchV1, LiveInspectionV1,
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
