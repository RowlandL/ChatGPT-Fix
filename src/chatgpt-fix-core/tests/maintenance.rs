use std::fs;
use std::path::PathBuf;

use chatgpt_fix_core::{
    MAINTENANCE_PLAN_SCHEMA, MaintenancePlanV1, NTC_MANIFEST_SCHEMA, NtcManifestV1,
    NtcReapplyState, generate_maintenance_plan, register_ntc_manifest,
};

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn fixture_root(name: &str) -> PathBuf {
    repository_root().join(format!("tests/release-fixtures/{name}"))
}

#[test]
fn maintenance_plan_is_always_dry_run() {
    let plan = generate_maintenance_plan(&fixture_root("maintenance")).expect("plan must succeed");

    assert!(plan.dry_run, "maintenance plan must be dry-run");
    assert_eq!(plan.state, "dry_run");
    assert!(plan.plan_id.starts_with("p8-maintenance-"));
}

#[test]
fn maintenance_plan_lists_only_authorized_scopes() {
    let plan = generate_maintenance_plan(&fixture_root("maintenance")).expect("plan must succeed");

    // A1/A4a/A5/A6 scopes become would-do items.
    let actions: Vec<&str> = plan.items.iter().map(|i| i.action.as_str()).collect();
    assert!(actions.contains(&"verify_staging_hashes"));
    assert!(actions.contains(&"validate_current_pointer"));
    assert!(actions.contains(&"observe_owned_tree"));
    assert!(actions.contains(&"plan_config_proposal"));
    assert_eq!(plan.items.len(), 4);

    // A7/A8a/A8c/A9 actions are blocked, never would-do.
    assert_eq!(plan.blocked.len(), 4);
    let blocked_all: String = plan.blocked.join(";");
    assert!(blocked_all.contains("A7"));
    assert!(blocked_all.contains("A8a"));
    assert!(blocked_all.contains("A8c"));
    assert!(blocked_all.contains("A9"));
    let actions_all: String = actions.join(";");
    assert!(!actions_all.contains("vacuum"));
    assert!(!actions_all.contains("installer"));
    assert!(!actions_all.contains("sign"));
    assert!(!actions_all.contains("publish"));
}

#[test]
fn maintenance_plan_roundtrips_through_schema() {
    let plan = generate_maintenance_plan(&fixture_root("maintenance")).expect("plan must succeed");
    let json = plan.to_json().expect("to_json");
    assert!(json.contains(MAINTENANCE_PLAN_SCHEMA));
    let parsed = MaintenancePlanV1::from_json(json.as_bytes()).expect("from_json");
    assert_eq!(parsed, plan);
}

#[test]
fn maintenance_plan_rejects_missing_manifest() {
    let root = std::env::temp_dir().join(format!(
        "chatgpt-fix-p8-maintenance-empty-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("create empty dir");
    let err = generate_maintenance_plan(&root).expect_err("missing manifest must fail");
    assert_eq!(err.code, "maintenance_manifest_unreadable");
}

#[test]
fn ntc_manifest_registers_delegated_evidence() {
    let manifest = register_ntc_manifest(&fixture_root("ntc")).expect("register must succeed");

    assert_eq!(manifest.artifact, "app.asar");
    assert_eq!(manifest.generation, 1);
    assert_eq!(manifest.reapply_state, NtcReapplyState::Clean);
    assert!(manifest.helper_health.contains("scheduled_task_present"));
    // No app.asar file present in the fixture, so hash verification is skipped
    // and registration proceeds from delegated evidence.
}

#[test]
fn ntc_manifest_roundtrips_through_schema() {
    let manifest = register_ntc_manifest(&fixture_root("ntc")).expect("register must succeed");
    let json = manifest.to_json().expect("to_json");
    assert!(json.contains(NTC_MANIFEST_SCHEMA));
    let parsed = NtcManifestV1::from_json(json.as_bytes()).expect("from_json");
    assert_eq!(parsed, manifest);
}

#[test]
fn ntc_manifest_fails_closed_on_present_hash_mismatch() {
    // Build a temp fixture that has an app.asar whose hash does NOT match the
    // registered after_sha256 -> registration must fail closed.
    let root = std::env::temp_dir().join(format!(
        "chatgpt-fix-p8-ntc-mismatch-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("create dir");
    fs::write(root.join("app.asar"), b"not-the-registered-hash-content").expect("write app.asar");
    fs::write(
        root.join("ntc-evidence.json"),
        r#"{"schema":"chatgpt_fix.ntc_evidence.v1","artifact":"app.asar","before_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","after_sha256":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","backup_ref":"backup","generation":1,"reapply_state":"clean","helper_health":"ok"}"#,
    )
    .expect("write evidence");
    let err = register_ntc_manifest(&root).expect_err("hash mismatch must fail closed");
    assert_eq!(err.code, "ntc_hash_mismatch");
}

#[test]
fn ntc_manifest_rejects_missing_evidence() {
    let root =
        std::env::temp_dir().join(format!("chatgpt-fix-p8-ntc-empty-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("create empty dir");
    let err = register_ntc_manifest(&root).expect_err("missing evidence must fail");
    assert_eq!(err.code, "ntc_evidence_unreadable");
}
