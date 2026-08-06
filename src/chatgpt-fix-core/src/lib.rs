use std::ffi::OsStr;
use std::process::ExitCode;

mod doctor;
mod error;
mod fixture;
mod generation;
mod json;
mod ownership;
mod path;
mod probe;
mod schema;
mod sha256;
mod shutdown;
mod staging;

pub use doctor::{DOCTOR_SCHEMA, DoctorFinding, DoctorLevel, DoctorStatus, DoctorV2};
pub use error::ContractError;
pub use fixture::{FIXTURE_SCHEMA, FixtureError, plan_fixture};
pub use generation::{activate, parse_shortcut_json, read_pointer, rollback, write_shortcut_json};
pub use ownership::observe_process_tree;
pub use path::{SafeRelativePath, Sha256Digest};
pub use probe::{
    A2_P2_ENV, inspect_live_json, inspect_probe_json, plan_from_probe, plan_live_json,
    plan_probe_json,
};
pub use schema::{
    BASELINE_SCHEMA, BaselineV2, BreakawayEvent, GENERATION_SCHEMA, GenerationState, GenerationV1,
    JobMemberEntry, LAUNCH_SCHEMA, LIVE_INSPECTION_SCHEMA, LaunchV1, LiveInspectionV1,
    OWNERSHIP_SCHEMA, OwnershipState, OwnershipV1, PLAN_SCHEMA, PlanAction, PlanActionKind,
    PlanDecision, PlanV1, RECEIPT_SCHEMA, ReceiptV1, SHUTDOWN_SCHEMA, STAGING_SCHEMA,
    ShortcutBackup, ShutdownMode, ShutdownState, ShutdownV1, StagingFileEntry, StagingState,
    StagingV1,
};
pub use sha256::sha256_bytes;
pub use shutdown::shutdown_process_tree;
pub use staging::{read_staging_state, stage_from_probe, verify_staging};

/// Current UTC time formatted as an RFC 3339 timestamp with Z suffix.
///
/// The runtime crate has no chrono/time dependency; this small formatter is
/// good enough for the staging receipt `created_at_utc` field.
pub fn utc_now_rfc3339() -> String {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // Civil date conversion (days since 1970-01-01).
    let days = seconds / 86_400;
    let day_seconds = seconds % 86_400;
    let hour = day_seconds / 3_600;
    let minute = (day_seconds % 3_600) / 60;
    let second = day_seconds % 60;

    // Howard Hinnant's civil_from_days algorithm.
    let z = days as i64 + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };

    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        y, m, d, hour, minute, second
    )
}

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
