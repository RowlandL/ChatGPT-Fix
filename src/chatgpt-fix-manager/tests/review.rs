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
        )
        .as_bytes()
    );
}
