use std::fs;
use std::path::Path;

use crate::json::JsonParser;
use crate::{
    ContractError, MaintenanceItem, MaintenancePlanV1, NtcManifestV1, NtcReapplyState, sha256_bytes,
};

const MAINTENANCE_MANIFEST: &str = "maintenance.json";
const NTC_EVIDENCE: &str = "ntc-evidence.json";

fn contract_error(code: &'static str, field: &str, message: impl Into<String>) -> ContractError {
    ContractError::new(code, field, message)
}

/// Generate a read-only maintenance dry-run plan
/// (`chatgpt_fix.maintenance_plan.v1`).
///
/// The fixture root must contain a `maintenance.json` manifest listing
/// candidate maintenance actions. Only actions whose scope is in the
/// A1/A4a/A5/A6-authorized range become would-do `items`; anything requiring
/// A7 (.codex), A8 (Setup/privilege/signing), or A9 (publication) is placed in
/// `blocked` and never listed as would-do. No file is modified.
pub fn generate_maintenance_plan(fixture_root: &Path) -> Result<MaintenancePlanV1, ContractError> {
    let manifest_path = fixture_root.join(MAINTENANCE_MANIFEST);
    let bytes = fs::read(&manifest_path).map_err(|error| {
        contract_error(
            "maintenance_manifest_unreadable",
            "maintenance.json",
            format!("maintenance manifest cannot be read: {error}"),
        )
    })?;
    let parsed = JsonParser::new(&bytes)?.parse_top_level()?;
    let _obj = parsed.as_object()?;

    let schema = parsed.field("schema")?.as_str()?;
    if schema != "chatgpt_fix.maintenance_fixture.v1" {
        return Err(contract_error(
            "schema_mismatch",
            "schema",
            format!("expected chatgpt_fix.maintenance_fixture.v1, got {schema}"),
        ));
    }

    let actions_value = parsed.field("actions")?;
    let actions_array = actions_value.as_array()?;
    let mut items = Vec::new();
    let mut blocked = Vec::new();

    for entry in actions_array {
        let action = entry.field("action")?.as_str()?.to_owned();
        let scope = entry.field("scope")?.as_str()?.to_owned();
        let requires = entry.field("requires")?.as_str()?.to_owned();
        let would_do = entry.field("would_do")?.as_str()?.to_owned();

        // Only A1/A4a/A5/A6-range scopes may be listed as would-do. Anything
        // requiring a higher gate is blocked (fail closed: never suggested).
        match requires.as_str() {
            "A1" | "A4a" | "A5" | "A6" => items.push(MaintenanceItem {
                action,
                scope,
                would_do,
            }),
            other => blocked.push(format!("{action}:{scope}:{other}")),
        }
    }

    let plan = MaintenancePlanV1 {
        plan_id: format!(
            "p8-maintenance-{}",
            crate::utc_now_rfc3339()
                .replace([':', '-'], "")
                .replace('T', "-")
                .replace('Z', "")
        ),
        dry_run: true,
        items,
        blocked,
        state: "dry_run".to_owned(),
        created_at_utc: crate::utc_now_rfc3339(),
    };
    plan.validate()?;
    Ok(plan)
}

/// Register native token-cost delegated evidence
/// (`chatgpt_fix.ntc_manifest.v1`).
///
/// Read-only: the fixture must contain `ntc-evidence.json` with the
/// NTC-NATIVE-20260801 delegated facts (artifact, before/after hashes, backup
/// ref, generation, reapply state, helper health). P8 never re-packs
/// app.asar, never mutates the helper task, and never touches `.codex`.
pub fn register_ntc_manifest(fixture_root: &Path) -> Result<NtcManifestV1, ContractError> {
    let evidence_path = fixture_root.join(NTC_EVIDENCE);
    let bytes = fs::read(&evidence_path).map_err(|error| {
        contract_error(
            "ntc_evidence_unreadable",
            "ntc-evidence.json",
            format!("NTC evidence cannot be read: {error}"),
        )
    })?;
    let parsed = JsonParser::new(&bytes)?.parse_top_level()?;
    let _obj = parsed.as_object()?;

    let schema = parsed.field("schema")?.as_str()?;
    if schema != "chatgpt_fix.ntc_evidence.v1" {
        return Err(contract_error(
            "schema_mismatch",
            "schema",
            format!("expected chatgpt_fix.ntc_evidence.v1, got {schema}"),
        ));
    }

    let get_str = |key: &str| -> Result<String, ContractError> {
        Ok(parsed.field(key)?.as_str()?.to_owned())
    };

    let artifact = get_str("artifact")?;
    let before_sha256 = get_str("before_sha256")?;
    let after_sha256 = get_str("after_sha256")?;
    let backup_ref = get_str("backup_ref")?;
    let generation = parsed.field("generation")?.as_i64()?;
    if generation < 0 {
        return Err(contract_error(
            "invalid_value",
            "generation",
            "must be non-negative",
        ));
    }
    let reapply_state = NtcReapplyState::from_json_str(get_str("reapply_state")?.as_str())?;
    let helper_health = get_str("helper_health")?;

    // Delegated evidence integrity: verify the before/after hashes match the
    // real bytes if the artifact file is available next to the evidence.
    let artifact_path = fixture_root.join("app.asar");
    if artifact_path.exists() {
        let bytes = fs::read(&artifact_path).map_err(|error| {
            contract_error(
                "ntc_artifact_unreadable",
                "app.asar",
                format!("NTC artifact cannot be read: {error}"),
            )
        })?;
        let actual = sha256_bytes(&bytes).as_str().to_owned();
        if actual != after_sha256 {
            return Err(contract_error(
                "ntc_hash_mismatch",
                "after_sha256",
                "registered app.asar hash does not match the present artifact (fail closed)",
            ));
        }
    }

    let manifest = NtcManifestV1 {
        artifact,
        before_sha256,
        after_sha256,
        backup_ref,
        generation: generation as u64,
        reapply_state,
        helper_health,
        created_at_utc: crate::utc_now_rfc3339(),
    };
    manifest.validate()?;
    Ok(manifest)
}
