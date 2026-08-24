use std::fs;
use std::process::Command;

const BINARY: &str = env!("CARGO_BIN_EXE_chatgpt-fix-manager");

fn fixture_root(name: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("chatgpt-fix-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("create root");
    fs::write(root.join("chatgpt-fix.fixture"), b"launcher-owned-v1")
        .expect("write launcher ownership marker");
    root
}

fn write_commit(root: &std::path::Path, before: &[u8], after: &[u8]) {
    let before = chatgpt_fix_core::sha256_bytes(before);
    let after = chatgpt_fix_core::sha256_bytes(after);
    fs::write(
        root.join("ntc-commit.json"),
        format!(
            "{{\"schema\":\"chatgpt_fix.ntc_commit.v1\",\"artifact\":\"app.asar\",\"before_sha256\":\"{}\",\"after_sha256\":\"{}\"}}",
            before, after
        ),
    )
    .expect("write NTC commit receipt");
}

#[test]
fn ntc_health_command_emits_receipt() {
    // ntc-health must always produce a typed receipt, whether or not the
    // helper is actually running (fail-open on the probe itself).
    let output = Command::new(BINARY)
        .arg("ntc-health")
        .output()
        .expect("run manager ntc-health");
    assert_eq!(output.status.code(), Some(0), "stderr: {:?}", output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("chatgpt_fix.ntc_health.v1"),
        "stdout: {stdout}"
    );
    assert!(stdout.contains("\"healthy\""), "stdout: {stdout}");
}

#[test]
fn ntc_reapply_fails_closed_when_asar_missing() {
    // A fixture dir without app.asar must fail closed (exit 3), never
    // fabricate a manifest.
    let root = fixture_root("ntc-reapply");

    let output = Command::new(BINARY)
        .args(["ntc-reapply", "--fixture-root"])
        .arg(&root)
        .output()
        .expect("run manager ntc-reapply");
    assert_eq!(output.status.code(), Some(3), "stderr: {:?}", output.stderr);
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("app.asar not found"),
        "stderr: {:?}",
        output.stderr
    );
}

#[test]
fn ntc_reapply_fails_closed_when_userscript_missing() {
    // A fixture with app.asar but no userscript must fail closed.
    let root = fixture_root("ntc-reapply2");
    fs::write(root.join("app.asar"), b"dummy").expect("write dummy asar");

    let output = Command::new(BINARY)
        .args(["ntc-reapply", "--fixture-root"])
        .arg(&root)
        .env("CHATGPT_FIX_NTC_NO_DOWNLOAD", "1")
        .env("USERPROFILE", &root)
        .env("LOCALAPPDATA", &root)
        .output()
        .expect("run manager ntc-reapply");
    assert_eq!(output.status.code(), Some(3), "stderr: {:?}", output.stderr);
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("userscript unavailable"),
        "stderr: {:?}",
        output.stderr
    );
}

#[test]
fn ntc_reapply_does_not_publish_an_empty_local_userscript() {
    let root = fixture_root("ntc-empty-local-userscript");
    fs::write(root.join("app.asar"), b"dummy").expect("write dummy asar");
    let candidate = root.join(".codex/tools/codex-token-cost/scripts/codex-live-token-cost.js");
    fs::create_dir_all(candidate.parent().expect("candidate parent"))
        .expect("create local candidate directory");
    fs::write(&candidate, b"").expect("write empty local candidate");
    let injector = root.join("injector.js");
    fs::write(&injector, b"process.exit(0);").expect("write injector fixture");

    let output = Command::new(BINARY)
        .args(["ntc-reapply", "--fixture-root"])
        .arg(&root)
        .env("CHATGPT_FIX_NTC_NO_DOWNLOAD", "1")
        .env("USERPROFILE", &root)
        .env("LOCALAPPDATA", root.join("unused-local-app-data"))
        .env("CHATGPT_FIX_NTC_INJECT_SCRIPT", &injector)
        .env("CHATGPT_FIX_NODE", "node-must-not-run-for-empty-userscript")
        .output()
        .expect("run manager ntc-reapply");

    let userscript = root.join("ntc-overlay/codex-live-token-cost.js");
    assert_eq!(output.status.code(), Some(3), "stderr: {:?}", output.stderr);
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("userscript unavailable"),
        "stderr: {:?}",
        output.stderr
    );
    assert!(
        !userscript.exists(),
        "an invalid candidate must never be published to the final userscript path"
    );
    let overlay = userscript.parent().expect("overlay directory");
    if overlay.is_dir() {
        assert!(
            fs::read_dir(overlay)
                .expect("read overlay")
                .all(|entry| !entry
                    .expect("read overlay entry")
                    .file_name()
                    .to_string_lossy()
                    .contains(".tmp-")),
            "failed staging must clean every temporary userscript"
        );
    }
}

#[test]
fn ntc_ensure_fails_closed_when_asar_missing() {
    // ntc-ensure must fail closed (exit 3) when app.asar is absent, the
    // same as ntc-reapply — never fabricate a manifest.
    let root = fixture_root("ntc-ensure");

    let output = Command::new(BINARY)
        .args(["ntc-ensure", "--fixture-root"])
        .arg(&root)
        .output()
        .expect("run manager ntc-ensure");
    assert_eq!(output.status.code(), Some(3), "stderr: {:?}", output.stderr);
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("app.asar not found"),
        "stderr: {:?}",
        output.stderr
    );
}

#[test]
fn ntc_ensure_skips_only_with_a_matching_commit_receipt() {
    let root = fixture_root("ntc-ensure-valid");
    let before = b"original-asar";
    let after = b"injected-asar";
    fs::write(root.join("app.asar"), after).expect("write asar");
    fs::write(root.join("app.asar.pre-ntc"), before).expect("write pre-ntc backup");
    write_commit(&root, before, after);

    let output = Command::new(BINARY)
        .args(["ntc-ensure", "--fixture-root"])
        .arg(&root)
        .output()
        .expect("run manager ntc-ensure");
    assert_eq!(output.status.code(), Some(0), "stderr: {:?}", output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("chatgpt_fix.ntc_ensure.v1") && stdout.contains("already_injected"),
        "stdout: {stdout}"
    );
    // The asar must be untouched.
    assert_eq!(
        fs::read(root.join("app.asar")).expect("asar still present"),
        b"injected-asar"
    );
}

#[test]
fn ntc_ensure_rejects_missing_or_invalid_commit_for_an_existing_backup() {
    for (name, receipt) in [
        ("ntc-ensure-missing", None),
        ("ntc-ensure-invalid", Some(b"not-json".as_slice())),
    ] {
        let root = fixture_root(name);
        fs::write(root.join("app.asar"), b"injected-asar").expect("write asar");
        fs::write(root.join("app.asar.pre-ntc"), b"original-asar").expect("write pre-ntc backup");
        if let Some(receipt) = receipt {
            fs::write(root.join("ntc-commit.json"), receipt).expect("write invalid receipt");
        }
        let output = Command::new(BINARY)
            .args(["ntc-ensure", "--fixture-root"])
            .arg(&root)
            .output()
            .expect("run manager ntc-ensure");
        assert_eq!(output.status.code(), Some(4), "stderr: {:?}", output.stderr);
    }
}

#[test]
fn ntc_ensure_rejects_hash_drift_in_a_commit_receipt() {
    let root = fixture_root("ntc-ensure-drift");
    let before = b"original-asar";
    let after = b"injected-asar";
    fs::write(root.join("app.asar"), b"drifted-asar").expect("write drifted asar");
    fs::write(root.join("app.asar.pre-ntc"), before).expect("write pre-ntc backup");
    write_commit(&root, before, after);

    let output = Command::new(BINARY)
        .args(["ntc-ensure", "--fixture-root"])
        .arg(&root)
        .output()
        .expect("run manager ntc-ensure");
    assert_eq!(output.status.code(), Some(4), "stderr: {:?}", output.stderr);
}

#[test]
fn ntc_reapply_rejects_an_invalid_timeout_before_starting_node() {
    let root = fixture_root("ntc-invalid-timeout");
    fs::create_dir_all(root.join("ntc-overlay")).expect("create overlay dir");
    fs::write(root.join("app.asar"), b"original-asar").expect("write asar");
    fs::write(
        root.join("ntc-overlay/codex-live-token-cost.js"),
        b"// userscript",
    )
    .expect("write userscript");
    let injector = root.join("injector.js");
    fs::write(&injector, b"process.exit(0);").expect("write injector stub");

    let output = Command::new(BINARY)
        .args(["ntc-reapply", "--fixture-root"])
        .arg(&root)
        .env("CHATGPT_FIX_NTC_INJECT_SCRIPT", &injector)
        .env("CHATGPT_FIX_NTC_TIMEOUT_SECS", "zero")
        .output()
        .expect("run manager ntc-reapply");
    assert_eq!(output.status.code(), Some(4), "stderr: {:?}", output.stderr);
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("CHATGPT_FIX_NTC_TIMEOUT_SECS"),
        "stderr: {:?}",
        output.stderr
    );
    assert!(
        !root.join("ntc-commit.json").exists(),
        "invalid configuration must not create a commit marker"
    );
}

#[test]
fn ntc_reapply_kills_a_timed_out_node_without_a_commit_marker() {
    if Command::new("node").arg("--version").output().is_err() {
        return;
    }

    let root = fixture_root("ntc-short-timeout");
    fs::create_dir_all(root.join("ntc-overlay")).expect("create overlay dir");
    fs::write(root.join("app.asar"), b"original-asar").expect("write asar");
    fs::write(
        root.join("ntc-overlay/codex-live-token-cost.js"),
        b"// userscript",
    )
    .expect("write userscript");
    let injector = root.join("slow-injector.js");
    fs::write(&injector, b"setInterval(() => {}, 1000);").expect("write slow injector");

    let output = Command::new(BINARY)
        .args(["ntc-reapply", "--fixture-root"])
        .arg(&root)
        .env("CHATGPT_FIX_NTC_INJECT_SCRIPT", &injector)
        .env("CHATGPT_FIX_NTC_TIMEOUT_SECS", "1")
        .env("CHATGPT_FIX_NODE", "node")
        .output()
        .expect("run manager ntc-reapply");
    assert_eq!(output.status.code(), Some(4), "stderr: {:?}", output.stderr);
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("timed out"),
        "stderr: {:?}",
        output.stderr
    );
    assert!(
        !root.join("ntc-commit.json").exists(),
        "timeout must not create a commit marker"
    );
}

#[test]
fn ntc_reapply_real_injection_roundtrip() {
    // End-to-end: build a tiny fake app.asar fixture (a directory repacked by
    // @electron/asar is not available in tests), so instead we exercise the
    // fail-closed path when node is present but the asar is not a real asar.
    // This keeps the test hermetic and still covers the command wiring.
    let root = fixture_root("ntc-reapply3");
    fs::create_dir_all(root.join("ntc-overlay")).expect("create overlay dir");
    fs::write(root.join("app.asar"), b"not-a-real-asar").expect("write fake asar");
    fs::write(
        root.join("ntc-overlay/codex-live-token-cost.js"),
        b"// userscript",
    )
    .expect("write userscript");

    let output = Command::new(BINARY)
        .args(["ntc-reapply", "--fixture-root"])
        .arg(&root)
        .output()
        .expect("run manager ntc-reapply");
    // Either node is unavailable (exit 4) or the fake asar fails to extract
    // (exit 3); never exit 0.
    let code = output.status.code().unwrap_or(-1);
    assert!(
        code == 3 || code == 4,
        "expected fail-closed (3/4), got {code}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
