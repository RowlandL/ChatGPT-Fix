use std::process::Command;

#[test]
fn version_command_reports_workspace_version() {
    let output = Command::new(env!("CARGO_BIN_EXE_chatgpt-fix-locale"))
        .arg("--version")
        .output()
        .expect("run locale tool");

    assert!(output.status.success());
    assert_eq!(output.stdout, b"ChatGPT-Fix-Locale 1.0.3\n");
}
