use std::fs;
use std::path::PathBuf;

use chatgpt_fix_core::{OWNERSHIP_SCHEMA, OwnershipState, OwnershipV1, observe_process_tree};

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn fixture_root() -> PathBuf {
    repository_root().join("tests/ownership-fixtures")
}

#[test]
fn observe_produces_report_only_receipt() {
    let ownership = observe_process_tree(&fixture_root()).expect("observe must succeed");

    assert_eq!(ownership.state, OwnershipState::Observed);
    assert_eq!(ownership.launch_id, "fixture-ownership-1");
    assert_eq!(ownership.root_pid, 1000);
    assert_eq!(ownership.job_members.len(), 4);
    assert_eq!(
        ownership.breakaway_events.len(),
        1,
        "one process broke away"
    );
    assert_eq!(ownership.breakaway_events[0].pid, 1002);
    assert_eq!(ownership.breakaway_events[0].event, "breakaway_detected");
    assert!(
        ownership.exit_report.contains("processes_observed=4"),
        "exit report must summarize: {}",
        ownership.exit_report
    );
    assert!(
        ownership.exit_report.contains("report-only"),
        "must be report-only: {}",
        ownership.exit_report
    );
}

#[test]
fn observe_roundtrips_through_schema() {
    let ownership = observe_process_tree(&fixture_root()).expect("observe must succeed");
    let json = ownership.to_json().expect("to_json");
    assert!(json.contains(OWNERSHIP_SCHEMA));
    let parsed = OwnershipV1::from_json(json.as_bytes()).expect("from_json");
    assert_eq!(parsed, ownership);
}

#[test]
fn observe_rejects_missing_manifest() {
    let root = std::env::temp_dir().join(format!("chatgpt-fix-p5-empty-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("create empty dir");
    let err = observe_process_tree(&root).expect_err("missing manifest must fail");
    assert_eq!(err.code, "tree_manifest_unreadable");
}

#[test]
fn observe_rejects_wrong_schema() {
    let root =
        std::env::temp_dir().join(format!("chatgpt-fix-p5-badschema-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("create dir");
    fs::write(
        root.join("process-tree.json"),
        r#"{"schema":"chatgpt_fix.other","launch_id":"x","root_pid":1,"processes":[]}"#,
    )
    .expect("write manifest");
    let err = observe_process_tree(&root).expect_err("wrong schema must fail");
    assert_eq!(err.code, "schema_mismatch");
}
