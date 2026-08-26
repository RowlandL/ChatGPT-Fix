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
fn two_activations_in_same_second_produce_distinct_ids() {
    // Regression: generation_id includes a random suffix so two activations
    // in the same second never collide on the same backup directory.
    let root = program_root("same-second");

    let g1 =
        activate(&baseline_fixture(), &root, fixture_shortcut(), false).expect("first activate");
    let g2 =
        activate(&baseline_fixture(), &root, fixture_shortcut(), false).expect("second activate");

    assert_ne!(
        g1.generation_id, g2.generation_id,
        "same-second activations must produce distinct generation IDs"
    );
    // Both backup directories exist independently.
    assert!(root.join("backups").join(&g1.generation_id).exists());
    assert!(root.join("backups").join(&g2.generation_id).exists());
}

#[test]
fn rollback_uses_program_root_pointer_not_receipt_path() {
    // Regression: rollback must write to <program_root>/current.json even if
    // a tampered receipt claims a different pointer_path.
    let root = program_root("tampered-pointer");
    let generation =
        activate(&baseline_fixture(), &root, fixture_shortcut(), false).expect("activate");

    // Tamper with the receipt's pointer_path to point elsewhere.
    let backup_dir = root.join("backups").join(&generation.generation_id);
    let receipt_path = backup_dir.join("generation.json");
    let receipt_text = fs::read_to_string(&receipt_path).expect("read receipt");
    let tampered = receipt_text.replace(&generation.pointer_path, "D:/tampered/evil.json");
    fs::write(&receipt_path, tampered).expect("write tampered receipt");

    // Rollback must still write to the real program-root current.json.
    let rolled = rollback(&root).expect("rollback");
    assert_eq!(rolled.state, GenerationState::RolledBack);
    let pointer = fs::read_to_string(root.join("current.json")).expect("read pointer");
    assert!(
        pointer.contains("null"),
        "pointer must be rolled back to null"
    );
    // The tampered path must NOT have been written.
    assert!(!std::path::Path::new("D:/tampered/evil.json").exists());
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
fn launch_from_pointer_accepts_initial_installer_schema() {
    // The initial (pre-refactor) installer writes
    // codex.ntfs.setup-state.v1 + ExperimentalMitigation=true instead of
    // chatgpt_fix.staging.v1 + state=verified. The launch path must accept
    // both, so an existing initial-install baseline can be launched by the
    // current launcher without reinstalling.
    use chatgpt_fix_core::launch_from_pointer;

    let root = program_root("legacy-schema");
    let baseline = root.join("baseline");
    fs::create_dir_all(&baseline).expect("create baseline");
    // Minimal official-package layout: the launcher accepts either
    // <root>/ChatGPT.exe or <root>/app/ChatGPT.exe; use the nested app/
    // layout. cmd.exe is used as a stand-in so the spawn actually succeeds
    // on Windows (a placeholder text file is not executable).
    let app_dir = baseline.join("app");
    fs::create_dir_all(&app_dir).expect("create app dir");
    let system_root = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".to_owned());
    fs::copy(
        PathBuf::from(&system_root).join("System32").join("cmd.exe"),
        app_dir.join("ChatGPT.exe"),
    )
    .expect("copy cmd.exe as stand-in ChatGPT.exe");
    // Initial installer state.json shape (exact field names from the v1
    // installer: "Schema" capitalised, ExperimentalMitigation boolean).
    fs::write(
        baseline.join("state.json"),
        r#"{"Schema":"codex.ntfs.setup-state.v1","ExperimentalMitigation":true,"InstallRoot":"C:\\x","ProfilePath":"C:\\x\\profile","SourceAppPath":"C:\\Program Files\\WindowsApps\\OpenAI.Codex_26.707.8479.0_x64__2p2nqsd0c76g0\\app","SourcePackageRoot":"C:\\Program Files\\WindowsApps\\OpenAI.Codex_26.707.8479.0_x64__2p2nqsd0c76g0","SourcePackageVersion":"26.707.8479.0","ChatGptSha256":"28C3E8B6C55FFF39ECB12A5EB27F493ABF997804247517AA7A46C277CA5D9E93","AppAsarSha256":"8DDC04D44985CA64D59097A76E5871C1010EEB94D7305FD61C5B3F111D452DFF","LastBackupPath":"C:\\x\\backups\\20260714T022028604Z","InstalledAtUtc":"2026-07-14T02:22:57Z","SourceTrustStatus":"REGISTERED_MSIX","SourceTrustBasis":"PACKAGE_REGISTRATION_AND_IDENTITY"}"#,
    )
    .expect("write initial-installer state.json");
    // Pointer written by the initial installer pointing at this baseline.
    fs::write(
        root.join("current.json"),
        format!(
            r#"{{"schema":"chatgpt_fix.pointer.v1","baseline_root":"{}"}}"#,
            baseline.to_string_lossy().replace('\\', "/")
        ),
    )
    .expect("write pointer");

    let (launch, _) = launch_from_pointer(&root).expect("legacy-schema baseline must launch");
    assert!(
        launch.executable.as_str().ends_with("app/ChatGPT.exe"),
        "executable must resolve under app/: {}",
        launch.executable.as_str()
    );
}

#[test]
fn launch_from_pointer_rejects_unverified_legacy_schema() {
    // Same legacy schema but mitigation disabled: must fail closed.
    use chatgpt_fix_core::launch_from_pointer;

    let root = program_root("legacy-unverified");
    let baseline = root.join("baseline");
    fs::create_dir_all(baseline.join("app")).expect("create app dir");
    fs::write(baseline.join("app/ChatGPT.exe"), b"x").expect("write fake exe");
    // Note: this baseline is rejected before spawn, so the placeholder is
    // never executed; a text file is fine here.
    fs::write(
        baseline.join("state.json"),
        r#"{"Schema":"codex.ntfs.setup-state.v1","ExperimentalMitigation":false}"#,
    )
    .expect("write disabled state");
    fs::write(
        root.join("current.json"),
        format!(
            r#"{{"schema":"chatgpt_fix.pointer.v1","baseline_root":"{}"}}"#,
            baseline.to_string_lossy().replace('\\', "/")
        ),
    )
    .expect("write pointer");

    let err = launch_from_pointer(&root).expect_err("must refuse disabled mitigation");
    assert_eq!(err.code, "baseline_not_verified");
}

#[test]
fn stable_profile_migration_moves_once_and_is_reused() {
    // Regression: the user-data dir used to live under the baseline, so a
    // baseline swap reset appearance/desktop settings. It must migrate once
    // to <program-root>/profile/user-data and be reused by later baselines.
    // Tested directly against `resolve_user_data_dir` so it is deterministic
    // regardless of whether a real ChatGPT.exe happens to be running.
    use chatgpt_fix_core::resolve_user_data_dir;

    let root = program_root("profile-migrate");
    let baseline = root.join("baseline");
    let legacy = baseline.join("profile").join("user-data");
    fs::create_dir_all(&legacy).expect("create legacy profile");
    fs::write(legacy.join("appearance.json"), r#"{"theme":"dark"}"#).expect("write profile");

    let stable = resolve_user_data_dir(&root, &baseline);
    assert_eq!(stable, root.join("profile").join("user-data"));
    assert!(
        stable.join("appearance.json").exists(),
        "profile migrated to stable location"
    );
    assert_eq!(
        fs::read_to_string(stable.join("appearance.json")).expect("read stable profile"),
        r#"{"theme":"dark"}"#
    );
    assert!(
        !baseline.join("profile").join("user-data").exists(),
        "legacy baseline profile moved away"
    );
    assert!(
        root.join("profile")
            .join("user-data-migrated.marker")
            .exists(),
        "migration marker written"
    );

    // A later resolve from a DIFFERENT baseline must reuse the stable
    // profile and must not migrate (or overwrite) again.
    let baseline2 = root.join("baseline2");
    let legacy2 = baseline2.join("profile").join("user-data");
    fs::create_dir_all(&legacy2).expect("create legacy2");
    fs::write(legacy2.join("appearance.json"), r#"{"theme":"light"}"#).expect("write profile2");

    let stable2 = resolve_user_data_dir(&root, &baseline2);
    assert_eq!(stable2, stable);
    assert_eq!(
        fs::read_to_string(stable.join("appearance.json")).expect("read stable profile"),
        r#"{"theme":"dark"}"#,
        "the second baseline's fresh profile must not replace the stable one"
    );
    assert!(
        legacy2.join("appearance.json").exists(),
        "second baseline's legacy profile left in place (no re-migration)"
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
