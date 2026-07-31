use std::collections::BTreeMap;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

use chatgpt_fix_core::{FIXTURE_SCHEMA, FixtureError, PlanActionKind, PlanDecision, plan_fixture};

fn fixtures_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures")
}

fn fixture(name: &str) -> PathBuf {
    fixtures_root().join(name)
}

fn assert_rejected(name: &str, expected_code: &str) {
    let plan = plan_fixture(&fixture(name)).expect("declarative negatives return a plan");

    assert_eq!(plan.fixture_id, name);
    assert_eq!(plan.decision, PlanDecision::Rejected);
    assert_eq!(plan.baseline, None);
    assert!(plan.actions.is_empty());
    assert_eq!(plan.errors, [expected_code]);
    plan.validate().expect("rejected PlanV1 must be valid");
}

fn snapshot_tree(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn visit(root: &Path, path: &Path, snapshot: &mut BTreeMap<PathBuf, Vec<u8>>) {
        let mut entries = fs::read_dir(path)
            .expect("fixture directory must be readable")
            .collect::<Result<Vec<_>, _>>()
            .expect("fixture entries must be readable");
        entries.sort_by_key(|entry| entry.file_name());

        for entry in entries {
            let entry_path = entry.path();
            let file_type = entry.file_type().expect("fixture type must be readable");
            if file_type.is_dir() {
                visit(root, &entry_path, snapshot);
            } else if file_type.is_file() {
                snapshot.insert(
                    entry_path
                        .strip_prefix(root)
                        .expect("fixture entry must remain under its root")
                        .to_owned(),
                    fs::read(&entry_path).expect("fixture file must be readable"),
                );
            }
        }
    }

    let mut snapshot = BTreeMap::new();
    visit(root, root, &mut snapshot);
    snapshot
}

#[test]
fn fixture_schema_is_frozen() {
    assert_eq!(FIXTURE_SCHEMA, "chatgpt_fix.fixture.v1");
}

#[test]
fn multi_version_selects_the_highest_valid_candidate() {
    let plan = plan_fixture(&fixture("multi-version")).expect("fixture must parse");
    let baseline = plan.baseline.as_ref().expect("ready plan needs a baseline");

    assert_eq!(plan.fixture_id, "multi-version");
    assert_eq!(plan.decision, PlanDecision::Ready);
    assert_eq!(baseline.baseline_id, "chatgpt-26.721.4979.0");
    assert_eq!(
        baseline.package_full_name,
        "OpenAI.ChatGPT_26.721.4979.0_x64__8wekyb3d8bbwe"
    );
    assert_eq!(baseline.version, "26.721.4979.0");
    assert_eq!(baseline.architecture, "x64");
    assert_eq!(baseline.publisher, "CN=OpenAI");
    assert_eq!(
        baseline.source.as_str(),
        "payload/26.721.4979.0/ChatGPT.exe"
    );
    assert_eq!(baseline.bytes, 4);
    assert_eq!(
        baseline.sha256.as_str(),
        "edeaaff3f1774ad2888673770c6d64097e391bc362d7d6fb34982ddf0efd18cb"
    );
    plan.validate().expect("ready PlanV1 must be valid");
}

#[test]
fn ready_plan_has_the_canonical_dry_run_actions_and_json() {
    let first = plan_fixture(&fixture("multi-version")).expect("fixture must parse");
    let second = plan_fixture(&fixture("multi-version")).expect("fixture must parse twice");

    assert_eq!(first, second);
    assert_eq!(first.actions.len(), 5);
    let expected = [
        (PlanActionKind::WouldCopy, "baselines/chatgpt-26.721.4979.0"),
        (
            PlanActionKind::WouldWrite,
            "baselines/chatgpt-26.721.4979.0/chatgpt-fix-baseline.json",
        ),
        (PlanActionKind::WouldSwitch, "current.json"),
        (PlanActionKind::WouldStart, "launcher/ChatGPT.exe"),
        (PlanActionKind::WouldTerminate, "owned-processes"),
    ];
    for (action, (kind, target)) in first.actions.iter().zip(expected) {
        assert_eq!(action.kind, kind);
        assert_eq!(action.target.as_str(), target);
        assert!(!action.execute);
    }

    let expected_json = concat!(
        r#"{"schema":"chatgpt_fix.plan.v1","fixture_id":"multi-version","decision":"ready","baseline":{"schema":"chatgpt_fix.baseline.v2","baseline_id":"chatgpt-26.721.4979.0","package_full_name":"OpenAI.ChatGPT_26.721.4979.0_x64__8wekyb3d8bbwe","version":"26.721.4979.0","architecture":"x64","publisher":"CN=OpenAI","source":"payload/26.721.4979.0/ChatGPT.exe","bytes":4,"sha256":""#,
        "edeaaff3f1774ad2888673770c6d64097e391bc362d7d6fb34982ddf0efd18cb",
        r#""},"actions":[{"kind":"would-copy","target":"baselines/chatgpt-26.721.4979.0","execute":false},{"kind":"would-write","target":"baselines/chatgpt-26.721.4979.0/chatgpt-fix-baseline.json","execute":false},{"kind":"would-switch","target":"current.json","execute":false},{"kind":"would-start","target":"launcher/ChatGPT.exe","execute":false},{"kind":"would-terminate","target":"owned-processes","execute":false}],"errors":[]}"#,
    );
    assert_eq!(first.to_json().unwrap(), expected_json);
    assert_eq!(second.to_json().unwrap(), expected_json);
}

#[test]
fn multi_version_payloads_match_the_bounded_fixture_bytes() {
    let root = fixture("multi-version");

    assert_eq!(
        fs::read(root.join("payload/26.707.8479.0/ChatGPT.exe")).unwrap(),
        b"hello world\n"
    );
    assert_eq!(
        fs::read(root.join("payload/26.721.4979.0/ChatGPT.exe")).unwrap(),
        b"abc\n"
    );
}

#[test]
fn planning_is_read_only_for_the_fixture_tree() {
    let root = fixture("multi-version");
    let before = snapshot_tree(&root);

    let plan = plan_fixture(&root).expect("fixture must parse");
    plan.validate().expect("plan must validate");

    assert_eq!(snapshot_tree(&root), before);
}

#[test]
fn wrong_architecture_is_rejected() {
    assert_rejected("wrong-architecture", "architecture_mismatch");
}

#[test]
fn unknown_publisher_is_rejected() {
    assert_rejected("unknown-publisher", "unknown_publisher");
}

#[test]
fn corrupt_manifest_is_rejected() {
    assert_rejected("corrupt-manifest", "corrupt_manifest");
}

#[test]
fn declared_reparse_root_is_rejected_before_payload_access() {
    assert_rejected("reparse-path", "reparse_path");
}

#[test]
fn insufficient_disk_is_rejected() {
    assert_rejected("insufficient-disk", "insufficient_disk");
}

#[test]
fn hash_mismatch_is_rejected() {
    assert_rejected("hash-mismatch", "hash_mismatch");
}

#[test]
fn path_escape_is_rejected() {
    assert_rejected("path-escape", "unsafe_path");
}

#[test]
fn an_unmarked_parent_is_a_stable_structural_error() {
    let marker = fixtures_root().join("chatgpt-fix.fixture");
    let error = plan_fixture(&fixtures_root()).expect_err("unmarked roots must fail");

    assert_eq!(error.code, "not_fixture_root");
    assert_eq!(error.path, marker);
    assert_eq!(error.message, "fixture marker is missing");
    assert_eq!(
        error.to_string(),
        format!(
            "not_fixture_root at {}: fixture marker is missing",
            marker.display()
        )
    );
    let _: &(dyn Error + 'static) = &error;
    let _: FixtureError = error;
}
