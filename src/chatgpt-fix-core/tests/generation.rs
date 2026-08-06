use std::fs;
use std::path::PathBuf;

use chatgpt_fix_core::{
    GENERATION_SCHEMA, GenerationState, GenerationV1, ShortcutBackup, activate,
    parse_shortcut_json, read_pointer, rollback, write_shortcut_json,
};

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn baseline_fixture() -> PathBuf {
    repository_root().join("tests/launch-fixtures/baseline")
}

fn program_root(tag: &str) -> PathBuf {
    let out = std::env::temp_dir().join(format!("chatgpt-fix-p4-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&out);
    fs::create_dir_all(&out).expect("create program root");
    out
}

fn fixture_shortcut() -> ShortcutBackup {
    ShortcutBackup {
        target_path:
            r"C:\Program Files\WindowsApps\OpenAI.Codex_9.9.9.0_x64__2p2nqsd0c76g0\ChatGPT.exe"
                .to_owned(),
        arguments: String::new(),
        working_directory: r"C:\Program Files\WindowsApps\OpenAI.Codex_9.9.9.0_x64__2p2nqsd0c76g0"
            .to_owned(),
        icon_location:
            r"C:\Program Files\WindowsApps\OpenAI.Codex_9.9.9.0_x64__2p2nqsd0c76g0\ChatGPT.exe,0"
                .to_owned(),
    }
}

#[test]
fn activate_switches_pointer_and_persists_generation() {
    let root = program_root("activate");
    let generation = activate(&baseline_fixture(), &root, fixture_shortcut(), false)
        .expect("activate must succeed");

    assert_eq!(generation.state, GenerationState::Activated);
    assert_eq!(
        generation.baseline_root,
        baseline_fixture().to_string_lossy().replace('\\', "/")
    );
    assert!(generation.generation_id.contains('-'));

    // Pointer switched.
    let pointer = read_pointer(&root).expect("pointer must be readable");
    assert!(pointer.contains(&baseline_fixture().to_string_lossy().replace('\\', "/")));

    // Generation receipt persisted in backups/<id>/generation.json.
    let backup_dir = root.join("backups").join(&generation.generation_id);
    let receipt = fs::read_to_string(backup_dir.join("generation.json")).expect("receipt exists");
    assert!(receipt.contains(GENERATION_SCHEMA));
    assert!(receipt.contains("activated"));

    // Round-trip through schema.
    let parsed = GenerationV1::from_json(receipt.as_bytes()).expect("receipt parses");
    assert_eq!(parsed, generation);
}

#[test]
fn activate_refuses_unverified_baseline() {
    let root = program_root("unverified");
    // Baseline with state=staged (not verified).
    let bad = root.join("baseline");
    fs::create_dir_all(&bad).expect("create bad baseline");
    fs::write(bad.join("state.json"), r#"{"schema":"chatgpt_fix.staging.v1","source_package_full_name":"x","source_version":"1","source_hash_manifest":[{"relative_path":"ChatGPT.exe","bytes":1,"sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}],"staging_root":"x","baseline_root":"x","files_staged":1,"total_bytes":1,"state":"staged","created_at_utc":"2026-08-06T00:00:00Z"}"#)
        .expect("write staged state");

    let err = activate(&bad, &root, fixture_shortcut(), false).expect_err("must refuse unverified");
    assert_eq!(err.code, "baseline_not_verified");
    assert!(
        !root.join("current.json").exists(),
        "pointer must not be written"
    );
}

#[test]
fn rollback_restores_pointer_and_marks_rolled_back() {
    let root = program_root("rollback");
    let generation = activate(&baseline_fixture(), &root, fixture_shortcut(), false)
        .expect("activate must succeed");

    // Pointer is now active; roll it back.
    let rolled = rollback(&root).expect("rollback must succeed");
    assert_eq!(rolled.state, GenerationState::RolledBack);
    assert_eq!(rolled.generation_id, generation.generation_id);

    // Pointer should be reset to null.
    let pointer = read_pointer(&root).expect("pointer must be readable");
    assert!(
        pointer.contains("null"),
        "pointer must be null after rollback: {pointer}"
    );

    // Backup dir preserved.
    let backup_dir = root.join("backups").join(&generation.generation_id);
    assert!(
        backup_dir.join("generation.json").exists(),
        "receipt preserved"
    );
}

#[test]
fn smoke_activation_marks_smoke_started() {
    let root = program_root("smoke");
    let generation = activate(&baseline_fixture(), &root, fixture_shortcut(), true)
        .expect("smoke activate must succeed");
    assert_eq!(generation.state, GenerationState::SmokeStarted);
}

#[test]
fn shortcut_json_roundtrips() {
    let baseline = baseline_fixture();
    let shortcut = fixture_shortcut();
    write_shortcut_json(&baseline, &shortcut).expect("write shortcut json");
    let bytes = fs::read(baseline.join("shortcut-backup.json")).expect("read shortcut json");
    let parsed = parse_shortcut_json(&bytes).expect("parse shortcut json");
    assert_eq!(parsed, shortcut);
    assert_eq!(parsed.target_path, shortcut.target_path);
    assert_eq!(parsed.icon_location, shortcut.icon_location);
}

#[test]
fn shortcut_parse_rejects_empty_target() {
    let bad = r#"{"schema":"chatgpt_fix.shortcut.v1","target_path":"","arguments":"","working_directory":"","icon_location":""}"#;
    let err = parse_shortcut_json(bad.as_bytes()).expect_err("empty target must be rejected");
    assert_eq!(err.code, "empty_field");
}

#[test]
fn shortcut_parse_rejects_wrong_schema() {
    let bad = r#"{"schema":"chatgpt_fix.other","target_path":"x","arguments":"","working_directory":"","icon_location":""}"#;
    let err = parse_shortcut_json(bad.as_bytes()).expect_err("wrong schema must be rejected");
    assert_eq!(err.code, "schema_mismatch");
}
