use std::fs;
use std::path::PathBuf;

use chatgpt_fix_core::{
    CONFIG_PROPOSAL_SCHEMA, ConfigApplyMode, ConfigProposalState, ConfigProposalV1, ConfigScope,
    config_apply, config_plan, config_rollback, parse_config_proposal,
};

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn fixture_root(scope: &str) -> PathBuf {
    repository_root().join(format!("tests/config-fixtures/{scope}"))
}

fn temp_canary(tag: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "chatgpt-fix-p7-canary-{tag}-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    root
}

#[test]
fn plan_produces_proposed_dry_run_receipt() {
    let proposal = config_plan(&fixture_root("mcp"), ConfigScope::Mcp, "backups/test-1")
        .expect("plan must succeed");

    assert_eq!(proposal.state, ConfigProposalState::Proposed);
    assert_eq!(proposal.scope, ConfigScope::Mcp);
    assert_eq!(proposal.apply_mode, ConfigApplyMode::Canary);
    assert!(proposal.proposal_id.starts_with("p7-mcp-"));
    assert_ne!(proposal.current_digest, proposal.proposed_digest);
    assert!(proposal.current_digest.len() == 64);
    assert!(proposal.proposed_digest.len() == 64);
}

#[test]
fn plan_roundtrips_through_schema() {
    let proposal = config_plan(
        &fixture_root("history"),
        ConfigScope::History,
        "backups/test-2",
    )
    .expect("plan must succeed");
    let json = proposal.to_json().expect("to_json");
    assert!(json.contains(CONFIG_PROPOSAL_SCHEMA));
    let parsed = ConfigProposalV1::from_json(json.as_bytes()).expect("from_json");
    assert_eq!(parsed, proposal);
}

#[test]
fn apply_writes_proposed_to_canary_and_marks_applied() {
    let fixture = fixture_root("mcp");
    let canary = temp_canary("apply");
    fs::create_dir_all(&canary).expect("create canary");

    let proposal =
        config_plan(&fixture, ConfigScope::Mcp, "backups/apply-1").expect("plan must succeed");
    let applied = config_apply(&fixture, &canary, &proposal).expect("apply must succeed");

    assert_eq!(applied.state, ConfigProposalState::Applied);
    let written = fs::read_to_string(canary.join("mcp.json")).expect("read canary mcp.json");
    assert!(written.contains(r#""filesystem": {"enabled": true}"#));
}

#[test]
fn apply_digest_mismatch_fails_closed() {
    let fixture = fixture_root("mcp");
    let canary = temp_canary("mismatch");
    fs::create_dir_all(&canary).expect("create canary");

    let mut proposal =
        config_plan(&fixture, ConfigScope::Mcp, "backups/mismatch-1").expect("plan must succeed");
    proposal.proposed_digest = "0".repeat(64);
    let err = config_apply(&fixture, &canary, &proposal).expect_err("digest mismatch must fail");
    assert_eq!(err.code, "digest_mismatch");
    // Fail closed: nothing was written.
    assert!(!canary.join("mcp.json").exists());
}

#[test]
fn apply_non_proposed_state_fails_closed() {
    let fixture = fixture_root("mcp");
    let canary = temp_canary("state");
    fs::create_dir_all(&canary).expect("create canary");

    let mut proposal =
        config_plan(&fixture, ConfigScope::Mcp, "backups/state-1").expect("plan must succeed");
    proposal.state = ConfigProposalState::Applied;
    let err = config_apply(&fixture, &canary, &proposal).expect_err("applied state must fail");
    assert_eq!(err.code, "invalid_state");
}

#[test]
fn rollback_restores_previous_canary_content() {
    let fixture = fixture_root("history");
    let canary = temp_canary("rollback");
    fs::create_dir_all(&canary).expect("create canary");
    // Simulate a pre-existing canary file so backup/rollback round-trips it.
    fs::write(
        canary.join("history.json"),
        r#"{"history": {"persistence": "legacy", "max_bytes": 1}}"#,
    )
    .expect("write legacy canary");

    let proposal = config_plan(&fixture, ConfigScope::History, "backups/rollback-1")
        .expect("plan must succeed");
    let applied = config_apply(&fixture, &canary, &proposal).expect("apply must succeed");
    assert_eq!(applied.state, ConfigProposalState::Applied);

    let rolled_back = config_rollback(&canary, &applied).expect("rollback must succeed");
    assert_eq!(rolled_back.state, ConfigProposalState::RolledBack);
    let restored = fs::read_to_string(canary.join("history.json")).expect("read restored file");
    assert!(
        restored.contains("legacy"),
        "rollback must restore the previous content: {restored}"
    );
}

#[test]
fn rollback_removes_applied_file_when_no_prior_backup() {
    let fixture = fixture_root("codex-home");
    let canary = temp_canary("remove");
    fs::create_dir_all(&canary).expect("create canary");
    // Empty canary: no pre-existing codex_home.json, so no backup file.

    let proposal = config_plan(&fixture, ConfigScope::CodexHome, "backups/remove-1")
        .expect("plan must succeed");
    let applied = config_apply(&fixture, &canary, &proposal).expect("apply must succeed");
    assert!(canary.join("codex_home.json").exists());

    let rolled_back = config_rollback(&canary, &applied).expect("rollback must succeed");
    assert_eq!(rolled_back.state, ConfigProposalState::RolledBack);
    assert!(
        !canary.join("codex_home.json").exists(),
        "applied file must be removed when no prior backup exists"
    );
}

#[test]
fn parse_proposal_from_json_works() {
    let proposal = config_plan(&fixture_root("mcp"), ConfigScope::Mcp, "backups/parse-1")
        .expect("plan must succeed");
    let json = proposal.to_json().expect("to_json");
    let parsed = parse_config_proposal(json.as_bytes()).expect("parse must succeed");
    assert_eq!(parsed, proposal);
}

#[test]
fn parse_proposal_rejects_wrong_schema() {
    let err = parse_config_proposal(
        br#"{"schema":"chatgpt_fix.other","proposal_id":"x","scope":"mcp","current_digest":"a","proposed_digest":"b","backup_path":"c","apply_mode":"canary","state":"proposed","created_at_utc":"2026-08-06T00:00:00Z"}"#,
    )
    .expect_err("wrong schema must fail");
    assert_eq!(err.code, "schema_mismatch");
}
