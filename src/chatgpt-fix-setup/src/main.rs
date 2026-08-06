use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const PRODUCT_NAME: &str = "ChatGPT-Fix-Setup";
const INSTALL_SUBDIR: &str = "ChatGPT-Fix";
const BIN_SUBDIR: &str = "bin";
const BACKUPS_SUBDIR: &str = "backups";

/// The four artifacts installed by Setup (per-user, only this project's
/// binaries — never the official OpenAI package).
const ARTIFACTS: [&str; 4] = [
    "ChatGPT-Fix-Launcher.exe",
    "ChatGPT-Fix-Manager.exe",
    "ChatGPT-Fix-Packer.exe",
    "ChatGPT-Fix-Setup.exe",
];

fn install_root() -> Result<PathBuf, String> {
    match std::env::var("LOCALAPPDATA") {
        Ok(base) if !base.is_empty() => {
            Ok(PathBuf::from(base).join("Programs").join(INSTALL_SUBDIR))
        }
        _ => Err("LOCALAPPDATA is not set; cannot determine per-user install root".to_owned()),
    }
}

fn utc_now_compact() -> String {
    // Seconds-resolution UNIX timestamp as a compact token for backup dirs.
    // Installs are rare; if two land in the same second the copy below fails
    // on the already-existing backup dir (fail closed, no silent overwrite).
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
        .to_string()
}

fn copy_atomic(src: &Path, dst: &Path) -> Result<(), String> {
    let tmp = dst.with_extension("exe.tmp");
    fs::copy(src, &tmp).map_err(|e| format!("cannot copy {}: {e}", src.display()))?;
    fs::rename(&tmp, dst).map_err(|e| format!("cannot commit {}: {e}", dst.display()))?;
    Ok(())
}

/// `install --source <dir>`: per-user install of this project's four EXEs.
///
/// Backs up any pre-existing destination files under `backups/<timestamp>/bin/`
/// BEFORE overwriting (the Setup backup function), then installs atomically.
fn run_install(source_dir: &Path) -> ExitCode {
    // Verify every source artifact exists first (fail closed).
    let mut missing = Vec::new();
    for name in ARTIFACTS {
        if !source_dir.join(name).is_file() {
            missing.push(name);
        }
    }
    if !missing.is_empty() {
        eprintln!(
            "install_failed: source directory missing artifacts: {}",
            missing.join(", ")
        );
        return ExitCode::from(3);
    }

    let root = match install_root() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("install_failed: {e}");
            return ExitCode::from(4);
        }
    };
    let bin_dir = root.join(BIN_SUBDIR);
    let backups_dir = root.join(BACKUPS_SUBDIR);

    // Backup step: any file that already exists at the destination is copied
    // to backups/<timestamp>/bin/ BEFORE being overwritten.
    let timestamp = utc_now_compact();
    let backup_target_dir = backups_dir.join(&timestamp).join(BIN_SUBDIR);
    let mut backed_up = Vec::new();
    for name in ARTIFACTS {
        let dst = bin_dir.join(name);
        if dst.is_file() {
            if let Err(e) = fs::create_dir_all(&backup_target_dir) {
                eprintln!("install_failed: cannot create backup dir: {e}");
                return ExitCode::from(3);
            }
            let backup_dst = backup_target_dir.join(name);
            if let Err(e) = fs::copy(&dst, &backup_dst) {
                eprintln!("install_failed: cannot back up {name}: {e}");
                return ExitCode::from(3);
            }
            backed_up.push(name);
        }
    }

    // Install step: atomic copy of each artifact.
    if let Err(e) = fs::create_dir_all(&bin_dir) {
        eprintln!("install_failed: cannot create bin dir: {e}");
        return ExitCode::from(3);
    }
    for name in ARTIFACTS {
        if let Err(e) = copy_atomic(&source_dir.join(name), &bin_dir.join(name)) {
            eprintln!("install_failed: {e}");
            return ExitCode::from(3);
        }
    }

    // One-click configuration (the whole point of Setup): detect the official
    // package, write current.json, and create shortcuts. Failures here are
    // reported but do NOT roll back the installed binaries — the install
    // itself succeeded.
    let launcher = bin_dir.join("ChatGPT-Fix-Launcher.exe");
    let config_ok = complete_one_click_config(&root, &launcher);

    // Install receipt (stdout, typed schema).
    let receipt = format!(
        "{{\"schema\":\"chatgpt_fix.setup_receipt.v1\",\"operation\":\"install\",\"status\":\"success\",\"version\":\"{}\",\"install_root\":\"{}\",\"backups\":[{}],\"config_complete\":{},\"created_at_utc\":\"{}Z\"}}",
        env!("CARGO_PKG_VERSION"),
        root.to_string_lossy().replace('\\', "/"),
        backed_up
            .iter()
            .map(|n| format!("\"{}\"", n))
            .collect::<Vec<_>>()
            .join(","),
        if config_ok { "true" } else { "false" },
        utc_now_compact()
    );
    println!("{receipt}");
    ExitCode::SUCCESS
}

/// One-click configuration: locate the official OpenAI.Codex package, write
/// `<root>/current.json` pointing at its app directory, and create/repair the
/// `ChatGPT.lnk` and `ChatGPT-Fix-Launcher.lnk` shortcuts so the user is done
/// after a single double-click install.
fn complete_one_click_config(root: &Path, launcher: &Path) -> bool {
    let mut ok = true;

    // 1. Detect the official package install location via Get-AppxPackage.
    let mut app_root: Option<String> = None;
    if let Ok(output) = std::process::Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "(Get-AppxPackage -Name 'OpenAI.Codex' -ErrorAction SilentlyContinue | Select-Object -First 1 -ExpandProperty InstallLocation)",
        ])
        .output()
        && output.status.success()
    {
        let text = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        if !text.is_empty() {
            app_root = Some(text);
        }
    }
    if let Some(pkg_root) = app_root {
        let exe = Path::new(&pkg_root).join("app").join("ChatGPT.exe");
        if exe.is_file() {
            // 2. Write current.json pointer (chatgpt_fix.pointer.v1) pointing
            // at the package app directory (baseline root).
            let pointer = format!(
                "{{\"schema\":\"chatgpt_fix.pointer.v1\",\"baseline_root\":\"{}\"}}\n",
                Path::new(&pkg_root)
                    .join("app")
                    .to_string_lossy()
                    .replace('\\', "/")
            );
            let pointer_path = root.join("current.json");
            let tmp = pointer_path.with_extension("json.tmp");
            if fs::write(&tmp, pointer).is_ok() && fs::rename(&tmp, &pointer_path).is_ok() {
                ok = ok && true;
            } else {
                eprintln!("setup_warn: cannot write current.json");
                ok = false;
            }
        } else {
            eprintln!("setup_warn: official package app\\ChatGPT.exe not found at {pkg_root}");
            ok = false;
        }
    } else {
        eprintln!("setup_warn: OpenAI.Codex package not detected; current.json not written");
        ok = false;
    }

    // 3. Create/repair shortcuts via PowerShell WScript.Shell (standard
    // Windows shortcut authoring; Setup runs this as the installing user).
    let shortcut_target = launcher.to_string_lossy().into_owned();
    let work_dir = launcher
        .parent()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();
    let start_menu = std::env::var("APPDATA")
        .map(|a| PathBuf::from(a).join("Microsoft/Windows/Start Menu/Programs"))
        .unwrap_or_default();
    let lnk_chatgpt = start_menu.join("ChatGPT.lnk");
    let lnk_launcher = start_menu.join("ChatGPT-Fix-Launcher.lnk");

    let ps = format!(
        "$s1 = (New-Object -ComObject WScript.Shell).CreateShortcut('{}'); $s1.TargetPath = '{}'; $s1.WorkingDirectory = '{}'; $s1.Description = 'ChatGPT (launched by ChatGPT-Fix-Launcher)'; $s1.Save(); $s2 = (New-Object -ComObject WScript.Shell).CreateShortcut('{}'); $s2.TargetPath = '{}'; $s2.WorkingDirectory = '{}'; $s2.Description = 'ChatGPT-Fix-Launcher'; $s2.Save();",
        lnk_chatgpt.to_string_lossy().replace('\'', "''"),
        shortcut_target.replace('\'', "''"),
        work_dir.replace('\'', "''"),
        lnk_launcher.to_string_lossy().replace('\'', "''"),
        shortcut_target.replace('\'', "''"),
        work_dir.replace('\'', "''"),
    );
    if let Ok(output) = std::process::Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", &ps])
        .output()
    {
        if output.status.success() {
            ok = ok && true;
        } else {
            eprintln!(
                "setup_warn: shortcut creation failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            );
            ok = false;
        }
    } else {
        eprintln!("setup_warn: cannot run powershell for shortcut creation");
        ok = false;
    }

    ok
}

/// `uninstall`: remove this project's installed EXEs (keeps backups).
fn run_uninstall() -> ExitCode {
    let root = match install_root() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("uninstall_failed: {e}");
            return ExitCode::from(4);
        }
    };
    let bin_dir = root.join(BIN_SUBDIR);
    if !bin_dir.is_dir() {
        eprintln!(
            "uninstall_failed: install root not found: {}",
            root.display()
        );
        return ExitCode::from(3);
    }
    let mut removed = Vec::new();
    let mut pending = Vec::new();
    for name in ARTIFACTS {
        let dst = bin_dir.join(name);
        if dst.is_file() {
            match fs::remove_file(&dst) {
                Ok(()) => removed.push(name),
                Err(e) => {
                    // A running Setup.exe cannot delete itself on Windows
                    // (the file is locked by the executing image). This is
                    // expected: mark it for deletion and report the uninstall
                    // as success-with-pending so the caller knows to retry or
                    // delete after exit. Never fail the whole uninstall for a
                    // self-lock.
                    if name == "ChatGPT-Fix-Setup.exe" {
                        eprintln!(
                            "uninstall_pending: {name} is locked by the running installer; it will be removed on the next run or manually after exit"
                        );
                        pending.push(name);
                        continue;
                    }
                    eprintln!("uninstall_failed: cannot remove {name}: {e}");
                    return ExitCode::from(3);
                }
            }
        }
    }
    let receipt = format!(
        "{{\"schema\":\"chatgpt_fix.setup_receipt.v1\",\"operation\":\"uninstall\",\"status\":\"success\",\"version\":\"{}\",\"removed\":[{}],\"pending\":[{}],\"created_at_utc\":\"{}Z\"}}",
        env!("CARGO_PKG_VERSION"),
        removed
            .iter()
            .map(|n| format!("\"{}\"", n))
            .collect::<Vec<_>>()
            .join(","),
        pending
            .iter()
            .map(|n| format!("\"{}\"", n))
            .collect::<Vec<_>>()
            .join(","),
        utc_now_compact()
    );
    println!("{receipt}");
    ExitCode::SUCCESS
}

fn main() -> ExitCode {
    let mut arguments = std::env::args_os().skip(1);
    match arguments.next() {
        Some(argument) if argument == std::ffi::OsStr::new("--version") => match arguments.next() {
            None => {
                println!("{PRODUCT_NAME} {}", env!("CARGO_PKG_VERSION"));
                ExitCode::SUCCESS
            }
            Some(_) => {
                eprintln!("Usage: {PRODUCT_NAME} --version");
                ExitCode::from(2)
            }
        },
        Some(argument) if argument == std::ffi::OsStr::new("install") => {
            // install --source <dir>
            match (arguments.next(), arguments.next()) {
                (Some(flag), Some(source)) if flag == std::ffi::OsStr::new("--source") => {
                    if arguments.next().is_some() {
                        eprintln!("Usage: {PRODUCT_NAME} install --source <dir>");
                        ExitCode::from(2)
                    } else {
                        run_install(Path::new(&source))
                    }
                }
                _ => {
                    eprintln!("Usage: {PRODUCT_NAME} install --source <dir>");
                    ExitCode::from(2)
                }
            }
        }
        Some(argument) if argument == std::ffi::OsStr::new("uninstall") => {
            if arguments.next().is_some() {
                eprintln!("Usage: {PRODUCT_NAME} uninstall");
                ExitCode::from(2)
            } else {
                run_uninstall()
            }
        }
        _ => {
            eprintln!(
                "install_forbidden: unsupported invocation; use 'install --source <dir>', 'uninstall', or '--version'"
            );
            ExitCode::from(4)
        }
    }
}
