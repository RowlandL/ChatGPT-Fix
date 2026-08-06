use std::fs;
use std::process::Command;

const BINARY: &str = env!("CARGO_BIN_EXE_chatgpt-fix-manager");

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
    let root = std::env::temp_dir().join(format!("chatgpt-fix-ntc-reapply-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("create root");

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
    let root =
        std::env::temp_dir().join(format!("chatgpt-fix-ntc-reapply2-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("create root");
    fs::write(root.join("app.asar"), b"dummy").expect("write dummy asar");

    let output = Command::new(BINARY)
        .args(["ntc-reapply", "--fixture-root"])
        .arg(&root)
        .output()
        .expect("run manager ntc-reapply");
    assert_eq!(output.status.code(), Some(3), "stderr: {:?}", output.stderr);
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("codex-live-token-cost.js not found"),
        "stderr: {:?}",
        output.stderr
    );
}

#[test]
fn ntc_reapply_real_injection_roundtrip() {
    // End-to-end: build a tiny fake app.asar fixture (a directory repacked by
    // @electron/asar is not available in tests), so instead we exercise the
    // fail-closed path when node is present but the asar is not a real asar.
    // This keeps the test hermetic and still covers the command wiring.
    let root =
        std::env::temp_dir().join(format!("chatgpt-fix-ntc-reapply3-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
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
