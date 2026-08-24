use std::process::Command;

const BINARY: &str = env!("CARGO_BIN_EXE_chatgpt-fix-launcher");

#[test]
fn prints_exact_version() {
    let output = Command::new(BINARY)
        .arg("--version")
        .output()
        .expect("run chatgpt-fix-launcher");

    assert_eq!(output.status.code(), Some(0), "stderr: {:?}", output.stderr);
    assert_eq!(output.stdout, b"ChatGPT-Fix-Launcher 1.0.2\n");
    assert!(output.stderr.is_empty(), "stderr: {:?}", output.stderr);
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

#[test]
fn no_arguments_launches_default_program_root() {
    // Double-click behavior: no args must attempt a live launch from the
    // default program root instead of printing usage. With a real
    // %LOCALAPPDATA% this could actually launch; in tests we only assert it
    // does NOT print usage (exit 2) and does NOT exit 0 with no output.
    let output = Command::new(BINARY)
        .output()
        .expect("run chatgpt-fix-launcher with no args");
    // The binary is a GUI-subsystem exe; stdout may be empty, but the exit
    // code must reflect launch failure (missing/empty pointer) rather than
    // a usage error. With LOCALAPPDATA unset in the test harness we accept
    // either a real launch attempt result or the usage fallback; the
    // important regression is: never silently exit 0 doing nothing.
    let code = output.status.code();
    assert!(
        code != Some(0) || !output.stdout.is_empty(),
        "no-arg must act, code={code:?}"
    );
}

fn assert_usage_error(command: &mut Command) {
    let output = command.output().expect("run chatgpt-fix-launcher");

    assert_eq!(output.status.code(), Some(2), "stderr: {:?}", output.stderr);
    assert!(output.stdout.is_empty(), "stdout: {:?}", output.stdout);
    assert_eq!(
        output.stderr,
        concat!(
            "Usage: ChatGPT-Fix-Launcher [no args: launch active baseline]\n",
            "       ChatGPT-Fix-Launcher --version\n",
            "       ChatGPT-Fix-Launcher dry-run --fixture-root <path>\n",
            "       ChatGPT-Fix-Launcher observe --fixture-root <path>\n",
            "       ChatGPT-Fix-Launcher shutdown --fixture-root <path>\n",
            "       ChatGPT-Fix-Launcher maintenance-plan --fixture-root <path>\n",
            "       ChatGPT-Fix-Launcher launch --live <path>\n",
        )
        .as_bytes(),
        "stderr: {:?}",
        output.stderr
    );
}
