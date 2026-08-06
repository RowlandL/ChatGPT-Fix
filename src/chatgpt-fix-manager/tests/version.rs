use std::process::Command;

const BINARY: &str = env!("CARGO_BIN_EXE_chatgpt-fix-manager");

#[test]
fn prints_exact_version() {
    let output = Command::new(BINARY)
        .arg("--version")
        .output()
        .expect("run chatgpt-fix-manager");

    assert_eq!(output.status.code(), Some(0), "stderr: {:?}", output.stderr);
    assert_eq!(output.stdout, b"ChatGPT-Fix-Manager 0.6.0\n");
    assert!(output.stderr.is_empty(), "stderr: {:?}", output.stderr);
}

#[test]
fn rejects_missing_argument() {
    let mut command = Command::new(BINARY);
    assert_usage_error(&mut command);
}

#[test]
fn rejects_unknown_argument() {
    let mut command = Command::new(BINARY);
    command.arg("--help");
    assert_usage_error(&mut command);
}

#[test]
fn rejects_extra_argument_after_version() {
    let mut command = Command::new(BINARY);
    command.args(["--version", "extra"]);
    assert_usage_error(&mut command);
}

fn assert_usage_error(command: &mut Command) {
    let output = command.output().expect("run chatgpt-fix-manager");

    assert_eq!(output.status.code(), Some(2), "stderr: {:?}", output.stderr);
    assert!(output.stdout.is_empty(), "stdout: {:?}", output.stdout);
    assert_eq!(
        output.stderr,
        concat!(
            "Usage: ChatGPT-Fix-Manager --version\n",
            "       ChatGPT-Fix-Manager review --fixture-root <path>\n",
            "       ChatGPT-Fix-Manager review --plan-stdin\n",
            "       ChatGPT-Fix-Manager activate --baseline <root> [--smoke]\n",
            "       ChatGPT-Fix-Manager rollback\n",
            "       ChatGPT-Fix-Manager doctor\n",
        )
        .as_bytes(),
        "stderr: {:?}",
        output.stderr
    );
}
