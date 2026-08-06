use std::path::Path;

use crate::fixture::FixtureError;
use crate::{
    BaselineV2, LiveInspectionV1, PlanAction, PlanActionKind, PlanDecision, PlanV1,
    SafeRelativePath,
};

const EXPECTED_ARCHITECTURE: &str = "x64";
const EXPECTED_PUBLISHER: &str = "CN=OpenAI";
const EXPECTED_PUBLISHER_ID: &str = "2p2nqsd0c76g0";
const LIVE_FIXTURE_ID: &str = "live-readonly";
const PROBE_SOURCE_NAME: &str = "probe-output.json";

/// Read a probe JSON file, parse it as `LiveInspectionV1`, validate it,
/// and return the canonical inspection.
pub fn inspect_probe_json(path: &Path) -> Result<LiveInspectionV1, FixtureError> {
    let bytes = std::fs::read(path).map_err(|error| {
        FixtureError::new(
            "probe_json_unreadable",
            path,
            format!("probe JSON cannot be read: {error}"),
        )
    })?;

    let inspection = LiveInspectionV1::from_json(&bytes).map_err(|error| {
        FixtureError::new(
            "probe_json_invalid",
            path,
            format!("probe JSON is invalid: {error}"),
        )
    })?;

    inspection.validate().map_err(|error| {
        FixtureError::new(
            "probe_validation_failed",
            path,
            format!("probe validation failed: {error}"),
        )
    })?;

    // P2 identity checks (beyond the schema-level validate).
    if inspection.identity_before != inspection.identity_after {
        return Err(FixtureError::new(
            "identity_changed_during_probe",
            path,
            "package identity changed during probe",
        ));
    }
    if inspection.hash_before != inspection.hash_after {
        return Err(FixtureError::new(
            "hash_changed_during_probe",
            path,
            "package hash changed during probe",
        ));
    }

    Ok(inspection)
}

/// Create a canonical `PlanV1` from a validated `LiveInspectionV1`.
///
/// The plan's `fixture_id` is always `"live-readonly"`. If the inspection
/// passes all acceptance checks (architecture, publisher, publisher_id,
/// version structure), the plan is `Ready` with a baseline and actions.
/// Otherwise the plan is `Rejected` with the first failing reason.
pub fn plan_from_probe(inspection: &LiveInspectionV1) -> Result<PlanV1, FixtureError> {
    // Check architecture.
    if inspection.architecture != EXPECTED_ARCHITECTURE {
        return Ok(rejected_plan("architecture_mismatch"));
    }

    // Check publisher.
    if inspection.publisher != EXPECTED_PUBLISHER {
        return Ok(rejected_plan("unknown_publisher"));
    }

    // Check publisher_id.
    if inspection.publisher_id != EXPECTED_PUBLISHER_ID {
        return Ok(rejected_plan("unknown_publisher_id"));
    }

    // Check version has four numeric parts.
    let version_parts: Vec<&str> = inspection.version.split('.').collect();
    if version_parts.len() != 4 || version_parts.iter().any(|p| p.parse::<u32>().is_err()) {
        return Ok(rejected_plan("invalid_version"));
    }

    // Check identity consistency (already validated in inspect_probe_json,
    // but double-check for safety).
    if inspection.identity_before != inspection.identity_after {
        return Ok(rejected_plan("identity_changed"));
    }

    // Build the baseline from the inspection data.
    let source = SafeRelativePath::parse(PROBE_SOURCE_NAME).map_err(|error| {
        FixtureError::new(
            "invalid_generated_source",
            PROBE_SOURCE_NAME,
            format!("cannot form probe source path: {error}"),
        )
    })?;

    let baseline = BaselineV2 {
        baseline_id: LIVE_FIXTURE_ID.to_owned(),
        package_full_name: inspection.package_full_name.clone(),
        version: inspection.version.clone(),
        architecture: inspection.architecture.clone(),
        publisher: inspection.publisher.clone(),
        source,
        bytes: inspection.primary_bytes,
        sha256: inspection.primary_sha256.clone(),
    };

    // Build the live-readonly actions (same pattern as fixture ready_plan
    // but with the fixed fixture_id).
    let baseline_root = format!("baselines/{LIVE_FIXTURE_ID}");
    let actions = vec![
        action(PlanActionKind::WouldCopy, &baseline_root)?,
        action(
            PlanActionKind::WouldWrite,
            &format!("{baseline_root}/chatgpt-fix-baseline.json"),
        )?,
        action(PlanActionKind::WouldSwitch, "current.json")?,
        action(PlanActionKind::WouldStart, "launcher/ChatGPT.exe")?,
        action(PlanActionKind::WouldTerminate, "owned-processes")?,
    ];

    let plan = PlanV1 {
        fixture_id: LIVE_FIXTURE_ID.to_owned(),
        decision: PlanDecision::Ready,
        baseline: Some(baseline),
        actions,
        errors: Vec::new(),
    };

    plan.validate().map_err(|error| {
        FixtureError::new(
            "invalid_generated_plan",
            LIVE_FIXTURE_ID,
            format!("generated plan violates its contract: {error}"),
        )
    })?;

    Ok(plan)
}

/// Read a probe JSON file, parse/validate it, and produce a canonical plan.
pub fn plan_probe_json(path: &Path) -> Result<PlanV1, FixtureError> {
    let inspection = inspect_probe_json(path)?;
    plan_from_probe(&inspection)
}

fn action(kind: PlanActionKind, target: &str) -> Result<PlanAction, FixtureError> {
    let target = SafeRelativePath::parse(target).map_err(|error| {
        FixtureError::new(
            "invalid_generated_target",
            target,
            format!("cannot form a safe action target: {error}"),
        )
    })?;
    Ok(PlanAction {
        kind,
        target,
        execute: false,
    })
}

fn rejected_plan(code: &'static str) -> PlanV1 {
    PlanV1 {
        fixture_id: LIVE_FIXTURE_ID.to_owned(),
        decision: PlanDecision::Rejected,
        baseline: None,
        actions: Vec::new(),
        errors: vec![code.to_owned()],
    }
}