use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const BINARY: &str = env!("CARGO_BIN_EXE_chatgpt-fix-setup");

fn temp_source(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "chatgpt-fix-setup-src-{tag}-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create source dir");
    dir
}

fn temp_localappdata(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "chatgpt-fix-setup-lapp-{tag}-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create localappdata dir");
    dir
}

fn write_fake_artifacts(dir: &Path) {
    for name in [
        "ChatGPT-Fix-Launcher.exe",
        "ChatGPT-Fix-Manager.exe",
        "ChatGPT-Fix-Packer.exe",
        "ChatGPT-Fix-Setup.exe",
    ] {
        fs::write(dir.join(name), format!("fake-binary-{name}")).expect("write fake artifact");
    }
}

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
fn rejects_unknown_invocation() {
    let output = Command::new(BINARY)
        .arg("--bogus")
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

#[test]
fn install_requires_source_flag() {
    let output = Command::new(BINARY)
        .arg("install")
        .output()
        .expect("run setup");
    assert_eq!(output.status.code(), Some(2), "stderr: {:?}", output.stderr);
}

#[test]
fn install_fails_closed_when_source_missing_artifacts() {
    let src = temp_source("empty");
    let lapp = temp_localappdata("empty");
    let output = Command::new(BINARY)
        .env("LOCALAPPDATA", &lapp)
        .args(["install", "--source"])
        .arg(&src)
        .output()
        .expect("run setup");
    assert_eq!(output.status.code(), Some(3), "stderr: {:?}", output.stderr);
    assert!(
        output
            .stderr
            .starts_with(b"install_failed: source directory missing artifacts"),
        "stderr: {:?}",
        output.stderr
    );
}

#[test]
fn install_copies_artifacts_and_prints_receipt() {
    let src = temp_source("ok");
    write_fake_artifacts(&src);
    let lapp = temp_localappdata("ok");
    let output = Command::new(BINARY)
        .env("LOCALAPPDATA", &lapp)
        .args(["install", "--source"])
        .arg(&src)
        .output()
        .expect("run setup");
    assert_eq!(output.status.code(), Some(0), "stderr: {:?}", output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("chatgpt_fix.setup_receipt.v1"),
        "stdout: {stdout}"
    );
    assert!(
        stdout.contains("\"operation\":\"install\""),
        "stdout: {stdout}"
    );
    assert!(
        stdout.contains("\"status\":\"success\""),
        "stdout: {stdout}"
    );
    // Artifacts actually installed.
    let bin = lapp.join("Programs/ChatGPT-Fix/bin");
    for name in [
        "ChatGPT-Fix-Launcher.exe",
        "ChatGPT-Fix-Manager.exe",
        "ChatGPT-Fix-Packer.exe",
        "ChatGPT-Fix-Setup.exe",
    ] {
        assert!(bin.join(name).is_file(), "missing installed {name}");
    }
}

#[test]
fn install_backs_up_existing_files() {
    let src = temp_source("backup");
    write_fake_artifacts(&src);
    let lapp = temp_localappdata("backup");
    // Pre-place an OLD Launcher at the destination.
    let bin = lapp.join("Programs/ChatGPT-Fix/bin");
    fs::create_dir_all(&bin).expect("create bin");
    fs::write(bin.join("ChatGPT-Fix-Launcher.exe"), b"OLD-BINARY-CONTENT")
        .expect("write old launcher");

    let output = Command::new(BINARY)
        .env("LOCALAPPDATA", &lapp)
        .args(["install", "--source"])
        .arg(&src)
        .output()
        .expect("run setup");
    assert_eq!(output.status.code(), Some(0), "stderr: {:?}", output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("ChatGPT-Fix-Launcher.exe"),
        "receipt must list the backed-up artifact: {stdout}"
    );
    // New content installed.
    assert_eq!(
        fs::read(bin.join("ChatGPT-Fix-Launcher.exe")).expect("read installed"),
        b"fake-binary-ChatGPT-Fix-Launcher.exe"
    );
    // Backup dir exists under backups/<ts>/bin with the OLD content.
    let backups_root = lapp.join("Programs/ChatGPT-Fix/backups");
    let entries: Vec<_> = fs::read_dir(&backups_root)
        .expect("backups root exists")
        .collect::<Result<_, _>>()
        .expect("read backups");
    assert_eq!(entries.len(), 1, "one backup timestamp dir expected");
    let backup_bin = entries[0].path().join("bin");
    assert_eq!(
        fs::read(backup_bin.join("ChatGPT-Fix-Launcher.exe")).expect("read backup"),
        b"OLD-BINARY-CONTENT"
    );
}

#[test]
fn uninstall_removes_artifacts_keeps_backups() {
    let src = temp_source("uninstall");
    write_fake_artifacts(&src);
    let lapp = temp_localappdata("uninstall");
    // Install first.
    let install = Command::new(BINARY)
        .env("LOCALAPPDATA", &lapp)
        .args(["install", "--source"])
        .arg(&src)
        .output()
        .expect("run setup install");
    assert_eq!(
        install.status.code(),
        Some(0),
        "stderr: {:?}",
        install.stderr
    );

    // Uninstall.
    let out = Command::new(BINARY)
        .env("LOCALAPPDATA", &lapp)
        .arg("uninstall")
        .output()
        .expect("run setup uninstall");
    assert_eq!(out.status.code(), Some(0), "stderr: {:?}", out.stderr);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("\"operation\":\"uninstall\""),
        "stdout: {stdout}"
    );
    let bin = lapp.join("Programs/ChatGPT-Fix/bin");
    for name in [
        "ChatGPT-Fix-Launcher.exe",
        "ChatGPT-Fix-Manager.exe",
        "ChatGPT-Fix-Packer.exe",
        "ChatGPT-Fix-Setup.exe",
    ] {
        assert!(
            !bin.join(name).exists(),
            "artifact should be removed: {name}"
        );
    }
}
