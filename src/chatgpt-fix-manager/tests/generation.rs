use std::fs;
use std::path::PathBuf;
use std::process::Command;

const BINARY: &str = env!("CARGO_BIN_EXE_chatgpt-fix-manager");

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn baseline_fixture() -> PathBuf {
    repository_root().join("tests/launch-fixtures/baseline")
}

fn program_root(tag: &str) -> PathBuf {
    let out = std::env::temp_dir().join(format!("chatgpt-fix-p4-mgr-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&out);
    fs::create_dir_all(&out).expect("create program root");
    out
}

#[test]
fn activate_cli_switches_pointer_and_persists_receipt() {
    let root = program_root("activate");
    let output = Command::new(BINARY)
        .env("CHATGPT_FIX_PROGRAM_ROOT", &root)
        .args(["activate", "--baseline"])
        .arg(baseline_fixture())
        .output()
        .expect("run manager activate");

    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {:?}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("stdout utf-8");
    assert!(stdout.contains("chatgpt_fix.generation.v1"));
    assert!(stdout.contains("\"state\":\"activated\""));

    // Pointer file created in program root.
    let pointer = fs::read_to_string(root.join("current.json")).expect("pointer exists");
    assert!(pointer.contains(&baseline_fixture().to_string_lossy().replace('\\', "/")));
}

#[test]
fn activate_cli_refuses_unverified_baseline() {
    let root = program_root("unverified");
    let bad = root.join("baseline");
    fs::create_dir_all(&bad).expect("create bad baseline");
    fs::write(
        bad.join("state.json"),
        r#"{"schema":"chatgpt_fix.staging.v1","source_package_full_name":"x","source_version":"1","source_hash_manifest":[{"relative_path":"ChatGPT.exe","bytes":1,"sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}],"staging_root":"x","baseline_root":"x","files_staged":1,"total_bytes":1,"state":"staged","created_at_utc":"2026-08-06T00:00:00Z"}"#,
    )
    .expect("write staged state");
    fs::write(
        bad.join("shortcut-backup.json"),
        r#"{"schema":"chatgpt_fix.shortcut.v1","target_path":"x","arguments":"","working_directory":"","icon_location":""}"#,
    )
    .expect("write shortcut metadata");

    let output = Command::new(BINARY)
        .env("CHATGPT_FIX_PROGRAM_ROOT", &root)
        .args(["activate", "--baseline"])
        .arg(&bad)
        .output()
        .expect("run manager activate");

    assert_eq!(
        output.status.code(),
        Some(3),
        "stderr: {:?}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("baseline_not_verified"), "stderr: {stderr}");
    assert!(
        !root.join("current.json").exists(),
        "pointer must not be written"
    );
}

#[test]
fn rollback_cli_restores_pointer() {
    let root = program_root("rollback");
    let activate = Command::new(BINARY)
        .env("CHATGPT_FIX_PROGRAM_ROOT", &root)
        .args(["activate", "--baseline"])
        .arg(baseline_fixture())
        .output()
        .expect("run manager activate");
    assert_eq!(activate.status.code(), Some(0), "activate failed");

    let rollback = Command::new(BINARY)
        .env("CHATGPT_FIX_PROGRAM_ROOT", &root)
        .arg("rollback")
        .output()
        .expect("run manager rollback");
    assert_eq!(
        rollback.status.code(),
        Some(0),
        "stderr: {:?}",
        String::from_utf8_lossy(&rollback.stderr)
    );
    let stdout = String::from_utf8(rollback.stdout).expect("stdout utf-8");
    assert!(stdout.contains("\"state\":\"rolled_back\""));

    let pointer = fs::read_to_string(root.join("current.json")).expect("pointer exists");
    assert!(
        pointer.contains("null"),
        "pointer must be null after rollback"
    );
}

#[test]
fn smoke_flag_marks_smoke_started() {
    let root = program_root("smoke");
    let output = Command::new(BINARY)
        .env("CHATGPT_FIX_PROGRAM_ROOT", &root)
        .args(["activate", "--baseline"])
        .arg(baseline_fixture())
        .arg("--smoke")
        .output()
        .expect("run manager activate --smoke");

    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {:?}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("stdout utf-8");
    assert!(stdout.contains("\"state\":\"smoke_started\""));
}
