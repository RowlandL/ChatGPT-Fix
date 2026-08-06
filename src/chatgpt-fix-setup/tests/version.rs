use std::process::Command;

const BINARY: &str = env!("CARGO_BIN_EXE_chatgpt-fix-setup");

#[test]
fn prints_exact_version() {
    let output = Command::new(BINARY)
        .arg("--version")
        .output()
        .expect("run chatgpt-fix-setup");

    assert_eq!(output.status.code(), Some(0), "stderr: {:?}", output.stderr);
    assert_eq!(output.stdout, b"ChatGPT-Fix-Setup 1.0.0\n");
    assert!(output.stderr.is_empty(), "stderr: {:?}", output.stderr);
}

#[test]
fn rejects_missing_argument() {
    let output = Command::new(BINARY)
        .output()
        .expect("run chatgpt-fix-setup");
    assert_eq!(output.status.code(), Some(4), "stderr: {:?}", output.stderr);
    assert_eq!(
        output.stderr,
        b"install_forbidden: running Setup requires A8a authorization\n"
    );
}

#[test]
fn rejects_install_style_argument() {
    // Even a plausible install invocation must be rejected fail-closed.
    let output = Command::new(BINARY)
        .arg("--install")
        .output()
        .expect("run chatgpt-fix-setup");
    assert_eq!(output.status.code(), Some(4), "stderr: {:?}", output.stderr);
    assert!(output.stdout.is_empty(), "stdout: {:?}", output.stdout);
}

#[test]
fn rejects_extra_argument_after_version() {
    let output = Command::new(BINARY)
        .args(["--version", "extra"])
        .output()
        .expect("run chatgpt-fix-setup");
    assert_eq!(output.status.code(), Some(2), "stderr: {:?}", output.stderr);
}
