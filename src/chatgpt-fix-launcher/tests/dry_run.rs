use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use chatgpt_fix_core::{plan_fixture, sha256_bytes};

const BINARY: &str = env!("CARGO_BIN_EXE_chatgpt-fix-launcher");

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

fn run_dry_run(root: &Path) -> std::process::Output {
    Command::new(BINARY)
        .args(["dry-run", "--fixture-root"])
        .arg(root)
        .output()
        .expect("run chatgpt-fix-launcher dry-run")
}

#[test]
fn emits_deterministic_launch_and_receipt_without_starting() {
    let root = fixture("multi-version");
    let plan_json = plan_fixture(&root)
        .expect("fixture must produce a plan")
        .to_json()
        .expect("plan must serialize");
    let plan_sha256 = sha256_bytes(plan_json.as_bytes());
    let source_commit = option_env!("CHATGPT_FIX_SOURCE_COMMIT").unwrap_or("unrecorded");
    let expected = format!(
        concat!(
            "{{\"schema\":\"chatgpt_fix.launch.v1\",",
            "\"launch_id\":\"fixture-multi-version\",",
            "\"generation\":1,",
            "\"baseline_id\":\"chatgpt-26.721.4979.0\",",
            "\"executable\":\"baselines/chatgpt-26.721.4979.0/ChatGPT.exe\",",
            "\"would_start\":false,",
            "\"reason\":\"p1_offline_dry_run\"}}\n",
            "{{\"schema\":\"chatgpt_fix.receipt.v1\",",
            "\"operation\":\"launch\",",
            "\"status\":\"dry-run\",",
            "\"plan_sha256\":\"{}\",",
            "\"artifact\":null,",
            "\"artifact_sha256\":null,",
            "\"source_commit\":\"{}\",",
            "\"toolchain\":\"rustc-1.97.1-x86_64-pc-windows-msvc\",",
            "\"signing_status\":\"unsigned\"}}\n",
        ),
        plan_sha256.as_str(),
        source_commit
    );

    let output = run_dry_run(&root);

    assert_eq!(output.status.code(), Some(0), "stderr: {:?}", output.stderr);
    assert_eq!(output.stdout, expected.as_bytes());
    assert!(output.stderr.is_empty(), "stderr: {:?}", output.stderr);
}

#[test]
fn dry_run_is_read_only_for_the_fixture_tree() {
    let root = fixture("multi-version");
    let before = snapshot_tree(&root);

    let output = run_dry_run(&root);

    assert_eq!(output.status.code(), Some(0), "stderr: {:?}", output.stderr);
    assert_eq!(snapshot_tree(&root), before);
}

#[test]
fn rejects_a_rejected_fixture_plan_without_launch_output() {
    let root = fixture("hash-mismatch");
    let marker = root.join("chatgpt-fix.fixture");

    let output = run_dry_run(&root);

    assert_eq!(output.status.code(), Some(3), "stderr: {:?}", output.stderr);
    assert!(output.stdout.is_empty());
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        format!(
            "fixture_plan_rejected at {}: hash_mismatch\n",
            marker.display()
        )
    );
}

#[test]
fn live_launch_is_refused_before_path_access() {
    let output = Command::new(BINARY)
        .args(["launch", "--live", r"C:\definitely-not-present\ChatGPT.exe"])
        .output()
        .expect("run forbidden live launch");

    assert_eq!(output.status.code(), Some(4), "stderr: {:?}", output.stderr);
    assert!(output.stdout.is_empty());
    assert_eq!(
        output.stderr,
        b"live_launch_forbidden: P1 launcher does not start programs\n"
    );
}
