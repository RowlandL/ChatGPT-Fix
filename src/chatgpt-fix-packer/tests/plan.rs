use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use chatgpt_fix_core::plan_fixture;

const BINARY: &str = env!("CARGO_BIN_EXE_chatgpt-fix-packer");

fn fixtures_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures")
}

fn fixture(name: &str) -> PathBuf {
    fixtures_root().join(name)
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

fn run_plan(root: &Path) -> std::process::Output {
    Command::new(BINARY)
        .args(["plan", "--fixture-root"])
        .arg(root)
        .output()
        .expect("run chatgpt-fix-packer plan")
}

#[test]
fn prints_only_the_canonical_ready_plan() {
    let root = fixture("multi-version");
    let expected = format!(
        "{}\n",
        plan_fixture(&root)
            .expect("fixture must plan")
            .to_json()
            .expect("plan must serialize")
    );

    let output = run_plan(&root);

    assert_eq!(output.status.code(), Some(0), "stderr: {:?}", output.stderr);
    assert_eq!(output.stdout, expected.as_bytes());
    assert!(output.stderr.is_empty(), "stderr: {:?}", output.stderr);
}

#[test]
fn prints_canonical_plans_for_every_rejected_fixture() {
    for name in [
        "wrong-architecture",
        "unknown-publisher",
        "corrupt-manifest",
        "reparse-path",
        "insufficient-disk",
        "hash-mismatch",
        "path-escape",
    ] {
        let root = fixture(name);
        let expected = format!(
            "{}\n",
            plan_fixture(&root)
                .expect("negative fixture must produce a plan")
                .to_json()
                .expect("plan must serialize")
        );

        let output = run_plan(&root);

        assert_eq!(
            output.status.code(),
            Some(0),
            "fixture {name}, stderr: {:?}",
            output.stderr
        );
        assert_eq!(output.stdout, expected.as_bytes(), "fixture {name}");
        assert!(
            output.stderr.is_empty(),
            "fixture {name}, stderr: {:?}",
            output.stderr
        );
    }
}

#[test]
fn planning_does_not_modify_the_fixture_tree() {
    let root = fixture("multi-version");
    let before = snapshot_tree(&root);

    let output = run_plan(&root);

    assert_eq!(output.status.code(), Some(0), "stderr: {:?}", output.stderr);
    assert_eq!(snapshot_tree(&root), before);
}

#[test]
fn rejects_an_unmarked_directory_with_the_stable_error() {
    let root = fixtures_root();
    let marker = root.join("chatgpt-fix.fixture");

    let output = run_plan(&root);

    assert_eq!(output.status.code(), Some(3), "stderr: {:?}", output.stderr);
    assert!(output.stdout.is_empty(), "stdout: {:?}", output.stdout);
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        format!(
            "not_fixture_root at {}: fixture marker is missing\n",
            marker.display()
        )
    );
}

#[test]
fn rejects_live_style_arguments_before_path_access() {
    let output = Command::new(BINARY)
        .args(["plan", "--live-root", r"C:\live-package"])
        .output()
        .expect("run chatgpt-fix-packer with a forbidden live option");

    assert_eq!(output.status.code(), Some(2), "stderr: {:?}", output.stderr);
    assert!(output.stdout.is_empty(), "stdout: {:?}", output.stdout);
    assert_eq!(
        output.stderr,
        concat!(
            "Usage: ChatGPT-Fix-Packer --version\n",
            "       ChatGPT-Fix-Packer plan --fixture-root <path>\n",
            "       ChatGPT-Fix-Packer inspect --probe-json <path>\n",
            "       ChatGPT-Fix-Packer plan --probe-json <path>\n",
            "       ChatGPT-Fix-Packer inspect --live-readonly\n",
            "       ChatGPT-Fix-Packer plan --live-readonly\n",
            "       ChatGPT-Fix-Packer stage --probe-json <path> --out <staging-root>\n",
            "       ChatGPT-Fix-Packer verify --staging <path>\n",
            "       ChatGPT-Fix-Packer maintenance-plan --fixture-root <path>\n",
        )
        .as_bytes()
    );
}

fn probe_fixtures_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../src/chatgpt-fix-core/tests/live-fixtures")
}

fn probe_fixture(name: &str) -> PathBuf {
    probe_fixtures_root().join(name).join("probe-output.json")
}

fn run_inspect_probe_json(path: &Path) -> std::process::Output {
    Command::new(BINARY)
        .args(["inspect", "--probe-json"])
        .arg(path)
        .output()
        .expect("run chatgpt-fix-packer inspect")
}

fn run_plan_probe_json(path: &Path) -> std::process::Output {
    Command::new(BINARY)
        .args(["plan", "--probe-json"])
        .arg(path)
        .output()
        .expect("run chatgpt-fix-packer plan")
}

#[test]
fn inspect_probe_json_outputs_canonical_json_for_valid_current() {
    let path = probe_fixture("valid-current");
    let expected = format!(
        "{}\n",
        chatgpt_fix_core::inspect_probe_json(&path)
            .expect("fixture must inspect")
            .to_json()
            .expect("inspection must serialize")
    );

    let output = run_inspect_probe_json(&path);

    assert_eq!(output.status.code(), Some(0), "stderr: {:?}", output.stderr);
    assert_eq!(output.stdout, expected.as_bytes());
    assert!(output.stderr.is_empty(), "stderr: {:?}", output.stderr);
}

#[test]
fn inspect_probe_json_outputs_canonical_json_for_all_fixtures() {
    // concurrent-update has intentional identity change and is excluded
    // from the "all pass" test (it is tested separately via plan rejection).
    for name in [
        "valid-current",
        "publisher-mismatch",
        "dependency-missing",
        "shortcut-drift",
        "reparse-path",
    ] {
        let path = probe_fixture(name);
        let expected = format!(
            "{}\n",
            chatgpt_fix_core::inspect_probe_json(&path)
                .expect("fixture must inspect")
                .to_json()
                .expect("inspection must serialize")
        );

        let output = run_inspect_probe_json(&path);

        assert_eq!(
            output.status.code(),
            Some(0),
            "fixture {name}, stderr: {:?}",
            output.stderr
        );
        assert_eq!(output.stdout, expected.as_bytes(), "fixture {name}");
        assert!(
            output.stderr.is_empty(),
            "fixture {name}, stderr: {:?}",
            output.stderr
        );
    }
}

#[test]
fn plan_probe_json_produces_ready_plan_for_valid_current() {
    let path = probe_fixture("valid-current");
    let expected = format!(
        "{}\n",
        chatgpt_fix_core::plan_probe_json(&path)
            .expect("fixture must plan")
            .to_json()
            .expect("plan must serialize")
    );

    let output = run_plan_probe_json(&path);

    assert_eq!(output.status.code(), Some(0), "stderr: {:?}", output.stderr);
    assert_eq!(output.stdout, expected.as_bytes());
    assert!(output.stderr.is_empty(), "stderr: {:?}", output.stderr);
}

#[test]
fn plan_probe_json_rejects_identity_changed_fixture() {
    // concurrent-update has identity_before != identity_after, so
    // inspect_probe_json should fail (identity changed during probe).
    let path = probe_fixture("concurrent-update");

    let output = run_plan_probe_json(&path);

    assert_eq!(output.status.code(), Some(3), "stderr: {:?}", output.stderr);
    assert!(output.stdout.is_empty(), "stdout: {:?}", output.stdout);
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("identity_changed"),
        "stderr must mention identity change: {:?}",
        output.stderr
    );
}

#[test]
fn plan_probe_json_rejects_publisher_mismatch() {
    let path = probe_fixture("publisher-mismatch");

    let output = run_plan_probe_json(&path);

    assert_eq!(output.status.code(), Some(0), "stderr: {:?}", output.stderr);
    let plan_json = String::from_utf8(output.stdout).unwrap();
    assert!(plan_json.contains("rejected"), "plan must be rejected");
    assert!(
        plan_json.contains("unknown_publisher"),
        "plan must mention unknown_publisher"
    );
    assert!(output.stderr.is_empty(), "stderr: {:?}", output.stderr);
}

#[test]
fn packer_rejects_live_readonly_without_authorization() {
    let output = Command::new(BINARY)
        .args(["inspect", "--live-readonly"])
        .output()
        .expect("run chatgpt-fix-packer inspect --live-readonly");

    assert_eq!(
        output.status.code(),
        Some(2),
        "must reject without A2-P2, stderr: {:?}",
        output.stderr
    );
    assert!(output.stdout.is_empty(), "stdout: {:?}", output.stdout);

    let output_plan = Command::new(BINARY)
        .args(["plan", "--live-readonly"])
        .output()
        .expect("run chatgpt-fix-packer plan --live-readonly");

    assert_eq!(
        output_plan.status.code(),
        Some(2),
        "must reject without A2-P2, stderr: {:?}",
        output_plan.stderr
    );
    assert!(
        output_plan.stdout.is_empty(),
        "stdout: {:?}",
        output_plan.stdout
    );
}
