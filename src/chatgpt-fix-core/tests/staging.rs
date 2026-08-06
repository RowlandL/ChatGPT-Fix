use std::fs;
use std::path::PathBuf;

use chatgpt_fix_core::{
    STAGING_SCHEMA, StagingState, read_staging_state, stage_from_probe, verify_staging,
};

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn fixture_probe() -> PathBuf {
    repository_root().join("tests/staging-fixtures/probe.json")
}

fn staging_dir(tag: &str) -> PathBuf {
    let out = std::env::temp_dir().join(format!(
        "chatgpt-fix-p3-staging-{tag}-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&out);
    out
}

#[test]
fn stage_produces_valid_receipt_and_whitelisted_files() {
    let out = staging_dir("valid");
    let staging = stage_from_probe(&fixture_probe(), &out).expect("stage must succeed");

    assert_eq!(staging.state, StagingState::Staged);
    assert_eq!(
        staging.source_package_full_name,
        "OpenAI.Codex_9.9.9.0_x64__2p2nqsd0c76g0"
    );
    assert_eq!(staging.source_version, "9.9.9.0");
    assert_eq!(
        staging.files_staged, 4,
        "only whitelisted extensions are staged"
    );
    assert!(staging.total_bytes > 0);
    assert_eq!(staging.source_hash_manifest.len(), 4);

    // Whitelisted files are present; the .tmp file must NOT be staged.
    for rel in [
        "ChatGPT.exe",
        "resources/app.asar",
        "resources/config.json",
        "assets/icon.png",
    ] {
        let p = out.join(rel);
        assert!(p.exists(), "staged file missing: {rel}");
        assert!(p.is_file(), "staged path is not a file: {rel}");
    }
    assert!(
        !out.join("cache.tmp").exists(),
        "non-whitelisted file must not be staged"
    );
    assert!(out.join("state.json").exists(), "state.json must exist");

    // Re-read and validate the on-disk receipt.
    let reread = read_staging_state(&out).expect("state must re-read");
    assert_eq!(reread.state, StagingState::Staged);
    assert_eq!(reread.source_hash_manifest, staging.source_hash_manifest);
}

#[test]
fn verify_passes_when_hashes_match() {
    let out = staging_dir("verify-ok");
    let _ = stage_from_probe(&fixture_probe(), &out).expect("stage must succeed");
    let verified = verify_staging(&out).expect("verify must succeed");

    assert_eq!(verified.state, StagingState::Verified);
    let reread = read_staging_state(&out).expect("state must re-read");
    assert_eq!(reread.state, StagingState::Verified);
    assert!(
        !out.join(".quarantine").exists(),
        "no quarantine on success"
    );
}

#[test]
fn verify_fails_closed_on_hash_mismatch_and_quarantines() {
    let out = staging_dir("verify-bad");
    let _ = stage_from_probe(&fixture_probe(), &out).expect("stage must succeed");

    // Corrupt a staged file after staging.
    let exe = out.join("ChatGPT.exe");
    fs::write(&exe, b"tampered").expect("write tampered file");

    let err = verify_staging(&out).expect_err("verify must fail on mismatch");
    assert!(
        err.code == "verify_hash_mismatch" || err.code == "verify_missing_file",
        "unexpected error code: {}",
        err.code
    );

    let reread = read_staging_state(&out).expect("state must re-read");
    assert_eq!(reread.state, StagingState::Quarantined);
    assert!(
        out.join(".quarantine").join("reason.txt").exists(),
        "quarantine marker must exist and not be auto-deleted"
    );
}

#[test]
fn stage_refuses_live_windowsapps_path() {
    // Build a probe whose install_location looks like a live package root.
    let probe_path = staging_dir("live").join("probe.json");
    fs::create_dir_all(probe_path.parent().expect("probe has parent"))
        .expect("create probe directory");
    let source = fs::read_to_string(fixture_probe()).expect("read fixture probe");
    let live_root = r"C:\Program Files\WindowsApps\OpenAI.Codex_9.9.9.0_x64__2p2nqsd0c76g0";
    // The fixture probe was written with json.dump, so backslashes are
    // already JSON-escaped (doubled). Re-escape the live root the same way.
    let live_root_escaped = live_root.replace('\\', "\\\\");
    // Locate the install_location value inside the JSON text and replace it.
    let marker = "\"install_location\": \"";
    let start = source.find(marker).expect("install_location present");
    let after = &source[start + marker.len()..];
    let end = after.find('"').expect("install_location value terminates");
    let live_probe = format!(
        "{}{}{}{}",
        &source[..start],
        marker,
        live_root_escaped,
        &after[end..]
    );
    fs::write(&probe_path, live_probe).expect("write live probe");

    let out = staging_dir("live-out");
    let err = stage_from_probe(&probe_path, &out).expect_err("live package must be refused");
    assert_eq!(err.code, "live_package_refused");
    assert!(
        !out.exists() || !out.join("state.json").exists(),
        "nothing may be staged"
    );
}

#[test]
fn staging_state_roundtrips_through_schema() {
    let out = staging_dir("roundtrip");
    let staging = stage_from_probe(&fixture_probe(), &out).expect("stage must succeed");

    let json = staging.to_json().expect("to_json must succeed");
    assert!(json.contains(STAGING_SCHEMA), "schema must be present");
    let parsed =
        chatgpt_fix_core::StagingV1::from_json(json.as_bytes()).expect("from_json must succeed");
    assert_eq!(parsed, staging);
}

/// The staging root must never accept a second `state.json` (fail closed on
/// double stage).
#[test]
fn stage_refuses_existing_state() {
    let out = staging_dir("double");
    let _ = stage_from_probe(&fixture_probe(), &out).expect("stage must succeed");
    let err = stage_from_probe(&fixture_probe(), &out).expect_err("double stage must be refused");
    assert_eq!(err.code, "staging_already_exists");
}
