use std::fs;
use std::path::PathBuf;

use chatgpt_fix_core::{
    SHUTDOWN_SCHEMA, ShutdownMode, ShutdownState, ShutdownV1, shutdown_process_tree,
};

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn fixture_root() -> PathBuf {
    repository_root().join("tests/shutdown-fixtures")
}

#[test]
fn graceful_shutdown_closes_clean_owned_tree() {
    let shutdown = shutdown_process_tree(&fixture_root().join("graceful-clean"))
        .expect("shutdown must succeed");

    assert_eq!(shutdown.state, ShutdownState::Closed);
    assert_eq!(shutdown.shutdown_mode, ShutdownMode::Graceful);
    assert_eq!(shutdown.launch_id, "fixture-shutdown-graceful");
    assert_eq!(shutdown.root_pid, 2000);
    assert_eq!(shutdown.handled_pids, vec![2000, 2001, 2002]);
    assert!(shutdown.excluded_pids.is_empty());
    assert!(shutdown.suspected_pids.is_empty());
}

#[test]
fn job_close_closes_clean_owned_tree() {
    let shutdown = shutdown_process_tree(&fixture_root().join("job-close-clean"))
        .expect("shutdown must succeed");

    assert_eq!(shutdown.state, ShutdownState::Closed);
    assert_eq!(shutdown.shutdown_mode, ShutdownMode::JobClose);
    assert_eq!(shutdown.root_pid, 2100);
    assert_eq!(shutdown.handled_pids, vec![2100, 2101]);
    assert!(shutdown.excluded_pids.is_empty());
    assert!(shutdown.suspected_pids.is_empty());
}

#[test]
fn breakaway_and_outside_job_are_excluded_never_handled() {
    let shutdown = shutdown_process_tree(&fixture_root().join("breakaway-excluded"))
        .expect("shutdown must succeed");

    assert_eq!(shutdown.state, ShutdownState::Closed);
    // Only the reconciled, in-Job members are handled.
    assert_eq!(shutdown.handled_pids, vec![2200, 2201]);
    // Breakaway and outside-Job processes are proven not owned.
    assert_eq!(shutdown.excluded_pids, vec![2202, 2203]);
    assert!(shutdown.suspected_pids.is_empty());
}

#[test]
fn handle_inheritance_alone_never_authorizes_shutdown() {
    let shutdown = shutdown_process_tree(&fixture_root().join("handle-inheritance"))
        .expect("shutdown must succeed");

    assert_eq!(shutdown.state, ShutdownState::Closed);
    // The owned in-Job process (even with inherited handles) is handled.
    assert_eq!(shutdown.handled_pids, vec![2400, 2401]);
    // An external process that inherited the launcher's handles is excluded:
    // inherited handles never authorize a shutdown.
    assert_eq!(shutdown.excluded_pids, vec![2402]);
    assert!(shutdown.suspected_pids.is_empty());
}

#[test]
fn pid_reuse_fails_closed() {
    let shutdown =
        shutdown_process_tree(&fixture_root().join("pid-reuse")).expect("shutdown must succeed");

    assert_eq!(shutdown.state, ShutdownState::Failed);
    // No PID is handled on a suspected tree.
    assert!(shutdown.handled_pids.is_empty());
    assert_eq!(shutdown.suspected_pids, vec![2301]);
}

#[test]
fn pid_reuse_with_excluded_flags_still_fails_closed() {
    // Regression: a duplicated PID that also carries breakaway=true or
    // in_job=false must still be suspected (fail closed), never excluded
    // silently. The conditional order must check pid_reuse FIRST.
    let shutdown = shutdown_process_tree(&fixture_root().join("pid-reuse-excluded"))
        .expect("shutdown must succeed");

    assert_eq!(shutdown.state, ShutdownState::Failed);
    assert!(shutdown.handled_pids.is_empty());
    // The reused PID is suspected even though it is also breakaway/outside-Job.
    assert!(shutdown.suspected_pids.contains(&2301));
}

#[test]
fn job_close_fails_closed_when_root_not_handled() {
    // Regression: JobClose must verify the owned root is in handled_pids;
    // if the fixture excludes the root, the shutdown must be Failed and the
    // root recorded as suspected (the close cannot be proven to cover it).
    let shutdown = shutdown_process_tree(&fixture_root().join("job-close-root-excluded"))
        .expect("shutdown must succeed");

    assert_eq!(shutdown.state, ShutdownState::Failed);
    assert!(
        shutdown.suspected_pids.contains(&2600),
        "root must be suspected when it cannot be proven handled: {:?}",
        shutdown.suspected_pids
    );
}

#[test]
fn reconciliation_failure_fails_closed() {
    let shutdown = shutdown_process_tree(&fixture_root().join("reconciliation-failed"))
        .expect("shutdown must succeed");

    assert_eq!(shutdown.state, ShutdownState::Failed);
    // The whole owned tree is suspected when the ledger does not reconcile.
    assert!(shutdown.handled_pids.is_empty());
    assert_eq!(shutdown.suspected_pids, vec![2500, 2501]);
}

#[test]
fn shutdown_roundtrips_through_schema() {
    let shutdown = shutdown_process_tree(&fixture_root().join("graceful-clean"))
        .expect("shutdown must succeed");
    let json = shutdown.to_json().expect("to_json");
    assert!(json.contains(SHUTDOWN_SCHEMA));
    let parsed = ShutdownV1::from_json(json.as_bytes()).expect("from_json");
    assert_eq!(parsed, shutdown);
}

#[test]
fn failed_shutdown_roundtrips_through_schema() {
    let shutdown =
        shutdown_process_tree(&fixture_root().join("pid-reuse")).expect("shutdown must succeed");
    let json = shutdown.to_json().expect("to_json");
    assert!(json.contains(SHUTDOWN_SCHEMA));
    let parsed = ShutdownV1::from_json(json.as_bytes()).expect("from_json");
    assert_eq!(parsed, shutdown);
}

#[test]
fn shutdown_rejects_missing_manifest() {
    let root = std::env::temp_dir().join(format!("chatgpt-fix-p6-empty-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("create empty dir");
    let err = shutdown_process_tree(&root).expect_err("missing manifest must fail");
    assert_eq!(err.code, "tree_manifest_unreadable");
}

#[test]
fn shutdown_rejects_wrong_schema() {
    let root =
        std::env::temp_dir().join(format!("chatgpt-fix-p6-badschema-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("create dir");
    fs::write(
        root.join("process-tree.json"),
        r#"{"schema":"chatgpt_fix.other","launch_id":"x","root_pid":1,"processes":[]}"#,
    )
    .expect("write manifest");
    let err = shutdown_process_tree(&root).expect_err("wrong schema must fail");
    assert_eq!(err.code, "schema_mismatch");
}
