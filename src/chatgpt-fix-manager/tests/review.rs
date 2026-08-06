use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use chatgpt_fix_core::plan_fixture;

const BINARY: &str = env!("CARGO_BIN_EXE_chatgpt-fix-manager");

fn fixtures_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures")
}

fn fixture(name: &str) -> PathBuf {
    fixtures_root().join(name)
}

fn run_review(root: &Path) -> std::process::Output {
    Command::new(BINARY)
        .args(["review", "--fixture-root"])
        .arg(root)
        .output()
        .expect("run chatgpt-fix-manager review")
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
fn displays_the_exact_core_plan_without_rediscovery() {
    for name in ["multi-version", "hash-mismatch"] {
        let root = fixture(name);
        let expected = format!(
            "{}\n",
            plan_fixture(&root)
                .expect("fixture must produce a plan")
                .to_json()
                .expect("plan must serialize")
        );

        let output = run_review(&root);

        assert_eq!(
            output.status.code(),
            Some(0),
            "fixture {name}, stderr: {:?}",
            output.stderr
        );
        assert_eq!(output.stdout, expected.as_bytes(), "fixture {name}");
        assert!(output.stderr.is_empty(), "fixture {name}");
    }
}

#[test]
fn review_is_read_only_for_the_fixture_tree() {
    let root = fixture("multi-version");
    let before = snapshot_tree(&root);

    let output = run_review(&root);

    assert_eq!(output.status.code(), Some(0), "stderr: {:?}", output.stderr);
    assert_eq!(snapshot_tree(&root), before);
}

#[test]
fn rejects_an_unmarked_directory() {
    let root = fixtures_root();
    let marker = root.join("chatgpt-fix.fixture");

    let output = run_review(&root);

    assert_eq!(output.status.code(), Some(3), "stderr: {:?}", output.stderr);
    assert!(output.stdout.is_empty());
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
        .args(["review", "--live-root", r"C:\live-package"])
        .output()
        .expect("run manager with forbidden live option");

    assert_eq!(output.status.code(), Some(2), "stderr: {:?}", output.stderr);
    assert!(output.stdout.is_empty());
    assert_eq!(
        output.stderr,
        concat!(
            "Usage: ChatGPT-Fix-Manager --version\n",
            "       ChatGPT-Fix-Manager review --fixture-root <path>\n",
            "       ChatGPT-Fix-Manager review --plan-stdin\n",
            "       ChatGPT-Fix-Manager doctor\n",
        )
        .as_bytes()
    );
}

#[test]
fn doctor_requires_p3_authorization() {
    let output = Command::new(BINARY)
        .args(["doctor"])
        .output()
        .expect("run manager doctor");

    assert_eq!(output.status.code(), Some(2), "stderr: {:?}", output.stderr);
    assert!(output.stdout.is_empty());
    assert_eq!(
        output.stderr,
        b"doctor v2 requires P3 authorization\n"
    );
}

#[test]
fn review_plan_stdin_accepts_canonical_ready_plan() {
    // Use plan_fixture to generate a canonical plan, then pipe to stdin.
    let root = fixture("multi-version");
    let plan = plan_fixture(&root).expect("fixture must produce a plan");
    let plan_json = plan.to_json().expect("plan must serialize");

    let output = Command::new(BINARY)
        .args(["review", "--plan-stdin"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn manager review --plan-stdin");

    use std::io::Write;
    let mut child = output;
    child
        .stdin
        .take()
        .expect("stdin must be available")
        .write_all(plan_json.as_bytes())
        .expect("write plan to stdin");
    let result = child.wait_with_output().expect("wait for manager");

    assert_eq!(result.status.code(), Some(0), "stderr: {:?}", result.stderr);
    assert_eq!(
        String::from_utf8_lossy(&result.stdout).trim(),
        plan_json,
        "canonical output must match"
    );
    assert!(result.stderr.is_empty(), "stderr: {:?}", result.stderr);
}

#[test]
fn review_plan_stdin_accepts_canonical_rejected_plan() {
    // Use plan_fixture to generate a rejected plan, then pipe to stdin.
    let root = fixture("unknown-publisher");
    let plan = plan_fixture(&root).expect("fixture must produce a plan");
    let plan_json = plan.to_json().expect("plan must serialize");

    let output = Command::new(BINARY)
        .args(["review", "--plan-stdin"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn manager review --plan-stdin");

    use std::io::Write;
    let mut child = output;
    child
        .stdin
        .take()
        .expect("stdin must be available")
        .write_all(plan_json.as_bytes())
        .expect("write plan to stdin");
    let result = child.wait_with_output().expect("wait for manager");

    assert_eq!(result.status.code(), Some(0), "stderr: {:?}", result.stderr);
    assert_eq!(
        String::from_utf8_lossy(&result.stdout).trim(),
        plan_json,
        "canonical output must match"
    );
    assert!(result.stderr.is_empty(), "stderr: {:?}", result.stderr);
}

#[test]
fn review_plan_stdin_rejects_duplicate_key() {
    // A plan JSON with a duplicate key.
    let bad_json = b"{\"schema\":\"chatgpt_fix.plan.v1\",\"schema\":\"chatgpt_fix.plan.v1\",\"fixture_id\":\"test\",\"decision\":\"rejected\",\"baseline\":null,\"actions\":[],\"errors\":[\"test\"]}";

    let output = Command::new(BINARY)
        .args(["review", "--plan-stdin"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn manager review --plan-stdin");

    use std::io::Write;
    let mut child = output;
    child
        .stdin
        .take()
        .expect("stdin must be available")
        .write_all(bad_json)
        .expect("write bad json to stdin");
    let result = child.wait_with_output().expect("wait for manager");

    assert_eq!(result.status.code(), Some(3), "stderr: {:?}", result.stderr);
    assert!(result.stdout.is_empty(), "stdout must be empty");
    assert!(
        String::from_utf8_lossy(&result.stderr).contains("duplicate_key"),
        "stderr must mention duplicate key: {:?}",
        result.stderr
    );
}

#[test]
fn review_plan_stdin_rejects_noncanonical_json() {
    // JSON with trailing data.
    let bad_json = b"{\"schema\":\"chatgpt_fix.plan.v1\",\"fixture_id\":\"test\",\"decision\":\"rejected\",\"baseline\":null,\"actions\":[],\"errors\":[\"test\"]} trailing";

    let output = Command::new(BINARY)
        .args(["review", "--plan-stdin"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn manager review --plan-stdin");

    use std::io::Write;
    let mut child = output;
    child
        .stdin
        .take()
        .expect("stdin must be available")
        .write_all(bad_json)
        .expect("write bad json to stdin");
    let result = child.wait_with_output().expect("wait for manager");

    assert_eq!(result.status.code(), Some(3), "stderr: {:?}", result.stderr);
    assert!(result.stdout.is_empty(), "stdout must be empty");
    assert!(
        String::from_utf8_lossy(&result.stderr).contains("trailing data"),
        "stderr must mention trailing data: {:?}",
        result.stderr
    );
}

#[test]
fn review_plan_stdin_rejects_unknown_schema() {
    let bad_json = b"{\"schema\":\"unknown.schema\",\"fixture_id\":\"test\",\"decision\":\"rejected\",\"baseline\":null,\"actions\":[],\"errors\":[\"test\"]}";

    let output = Command::new(BINARY)
        .args(["review", "--plan-stdin"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn manager review --plan-stdin");

    use std::io::Write;
    let mut child = output;
    child
        .stdin
        .take()
        .expect("stdin must be available")
        .write_all(bad_json)
        .expect("write bad json to stdin");
    let result = child.wait_with_output().expect("wait for manager");

    assert_eq!(result.status.code(), Some(3), "stderr: {:?}", result.stderr);
    assert!(result.stdout.is_empty(), "stdout must be empty");
    assert!(
        String::from_utf8_lossy(&result.stderr).contains("schema_mismatch"),
        "stderr must mention schema mismatch: {:?}",
        result.stderr
    );
}

#[test]
fn review_plan_stdin_rejects_invalid_plan() {
    // A plan with valid JSON but invalid plan structure (ready without baseline).
    let bad_json = b"{\"schema\":\"chatgpt_fix.plan.v1\",\"fixture_id\":\"test\",\"decision\":\"ready\",\"baseline\":null,\"actions\":[],\"errors\":[]}";

    let output = Command::new(BINARY)
        .args(["review", "--plan-stdin"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn manager review --plan-stdin");

    use std::io::Write;
    let mut child = output;
    child
        .stdin
        .take()
        .expect("stdin must be available")
        .write_all(bad_json)
        .expect("write bad json to stdin");
    let result = child.wait_with_output().expect("wait for manager");

    assert_eq!(result.status.code(), Some(3), "stderr: {:?}", result.stderr);
    assert!(result.stdout.is_empty(), "stdout must be empty");
    assert!(
        String::from_utf8_lossy(&result.stderr).contains("invariant_violation"),
        "stderr must mention invariant violation: {:?}",
        result.stderr
    );
}
