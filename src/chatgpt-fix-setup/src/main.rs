// The Setup binary is a Windows GUI-subsystem program: double-clicking it
// must NOT flash a console window. With no arguments it shows a guided
// install dialog (parity with the initial Codex-NTFS-Fix GUI installer);
// `install --source <dir>`, `uninstall` and `--version` remain available for
// scripting (their console output is not visible in GUI mode; results are
// recorded in logs/setup.jsonl).
#![windows_subsystem = "windows"]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const PRODUCT_NAME: &str = "ChatGPT-Fix-Setup";

/// Resolve only the absolute inbox Windows PowerShell helper. Setup must not
/// execute a PATH-controlled helper while discovering an official package.
fn find_powershell() -> Result<PathBuf, String> {
    let path =
        PathBuf::from(std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".to_owned()))
            .join("System32")
            .join("WindowsPowerShell")
            .join("v1.0")
            .join("powershell.exe");
    if path.is_file() {
        Ok(path)
    } else {
        Err(format!(
            "Windows PowerShell helper not found at {}",
            path.display()
        ))
    }
}
const INSTALL_SUBDIR: &str = "ChatGPT-Fix";
const BIN_SUBDIR: &str = "bin";
const BACKUPS_SUBDIR: &str = "backups";
const LOG_SUBDIR: &str = "logs";
const LOG_FILE: &str = "setup.jsonl";
const LOG_SCHEMA: &str = "chatgpt_fix.setup_log.v1";

/// The five artifacts installed by Setup (per-user, only this project's
/// binaries — never the official OpenAI package).
const ARTIFACTS: [&str; 5] = [
    "ChatGPT-Fix-Launcher.exe",
    "ChatGPT-Fix-Manager.exe",
    "ChatGPT-Fix-Packer.exe",
    "ChatGPT-Fix-Setup.exe",
    "ChatGPT-Fix-Locale.exe",
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

/// ISO-8601 UTC timestamp (no external crate; Hinnant civil-from-days).
fn utc_iso8601() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let (hh, mm, ss) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y2 = if m <= 2 { y + 1 } else { y };
    format!("{y2:04}-{m:02}-{d:02}T{hh:02}:{mm:02}:{ss:02}Z")
}

/// Append one line to `<root>/logs/setup.jsonl` (parity with the initial
/// `codex.ntfs.setup-log.v1` design). Best-effort: logging must never fail
/// an install, so errors are silently ignored.
fn append_setup_log(root: &Path, action: &str, status: &str, code: &str) {
    let log_dir = root.join(LOG_SUBDIR);
    let log_path = log_dir.join(LOG_FILE);
    let line = format!(
        "{{\"Schema\":\"{LOG_SCHEMA}\",\"TimestampUtc\":\"{}\",\"Action\":\"{action}\",\"Status\":\"{status}\",\"Code\":\"{code}\",\"SetupVersion\":\"{}\"}}\n",
        utc_iso8601(),
        env!("CARGO_PKG_VERSION")
    );
    if fs::create_dir_all(&log_dir).is_err() {
        return;
    }
    use std::io::Write;
    if let Ok(mut file) = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
    {
        let _ = file.write_all(line.as_bytes());
    }
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
mod gui;

fn run_install(source_dir: &Path) -> ExitCode {
    run_install_with_progress(source_dir, &|_, _| {})
}

/// Install pipeline with a progress callback `(done_bytes, total_bytes)`.
/// The GUI wizard drives this on a worker thread; the console/CLI path uses
/// the no-op closure above. All existing features are preserved (baseline
/// staging with full hash manifest, setup.jsonl logging, shortcuts, icons,
/// self-healing state.json).
fn run_install_with_progress(source_dir: &Path, progress: &dyn Fn(u64, u64)) -> ExitCode {
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
    append_setup_log(&root, "INSTALL", "START", "ACTION_REQUESTED");
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
                append_setup_log(&root, "INSTALL", "FAIL", "FAIL_BACKUP_DIR");
                return ExitCode::from(3);
            }
            let backup_dst = backup_target_dir.join(name);
            if let Err(e) = fs::copy(&dst, &backup_dst) {
                eprintln!("install_failed: cannot back up {name}: {e}");
                append_setup_log(&root, "INSTALL", "FAIL", "FAIL_BACKUP_COPY");
                return ExitCode::from(3);
            }
            backed_up.push(name);
        }
    }

    // Install step: atomic copy of each artifact.
    if let Err(e) = fs::create_dir_all(&bin_dir) {
        eprintln!("install_failed: cannot create bin dir: {e}");
        append_setup_log(&root, "INSTALL", "FAIL", "FAIL_BIN_DIR");
        return ExitCode::from(3);
    }
    for name in ARTIFACTS {
        if let Err(e) = copy_atomic(&source_dir.join(name), &bin_dir.join(name)) {
            eprintln!("install_failed: {e}");
            append_setup_log(&root, "INSTALL", "FAIL", "FAIL_ARTIFACT_COPY");
            return ExitCode::from(3);
        }
    }

    // One-click configuration (the whole point of Setup): detect the official
    // package, stage the user-owned baseline, write current.json, and create
    // shortcuts. Failures here are reported but do NOT roll back the installed
    // binaries — the install itself succeeded.
    let launcher = bin_dir.join("ChatGPT-Fix-Launcher.exe");
    let config_ok = complete_one_click_config(&root, &launcher, progress);
    if config_ok {
        append_setup_log(
            &root,
            "CONFIG",
            "PASS",
            "BASELINE_STAGED_AND_POINTER_WRITTEN",
        );
    } else {
        append_setup_log(&root, "CONFIG", "FAIL", "CONFIG_INCOMPLETE");
    }

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
    append_setup_log(&root, "INSTALL", "PASS", "INSTALLED");
    ExitCode::SUCCESS
}

/// Stage an immutable user-owned baseline: copy `<pkg_root>/app` into
/// `baseline_app` (tmp + rename for atomicity) and write a verified
/// `state.json` (StagingV1 shape, FULL hash manifest) so the launcher's
/// baseline check accepts it.
///
/// Idempotent: an existing baseline whose `ChatGPT.exe` matches the source
/// size is reused as-is (fast reinstall).
fn stage_baseline(
    pkg_root: &Path,
    baseline_root: &Path,
    baseline_app: &Path,
    src_exe: &Path,
    progress: &dyn Fn(u64, u64),
) -> bool {
    let existing_exe = baseline_app.join("ChatGPT.exe");
    let size_matches = std::fs::metadata(&existing_exe).ok().map(|m| m.len())
        == std::fs::metadata(src_exe).ok().map(|m| m.len());
    // Reuse only when the baseline is complete AND its state.json matches the
    // current source version (verified). A stale/partial/invalid baseline is
    // re-staged below — this self-heals the earlier bug where state.json was
    // written with a full file count but a single-entry manifest.
    let pkg_dir_name = pkg_root
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "package".to_owned());
    let current_version = pkg_dir_name
        .split('_')
        .nth(1)
        .filter(|s| !s.is_empty())
        .unwrap_or(&pkg_dir_name)
        .to_owned();
    let state_ok = fs::read(baseline_root.join("state.json"))
        .map(|bytes| {
            let text = String::from_utf8_lossy(&bytes);
            text.contains("\"state\":\"verified\"")
                && text.contains(&format!("\"source_version\":\"{current_version}\""))
        })
        .unwrap_or(false);
    if existing_exe.is_file() && size_matches && state_ok {
        return true; // already staged and verified for this package version
    }
    if fs::create_dir_all(baseline_root).is_err() {
        eprintln!("setup_warn: cannot create baseline root");
        return false;
    }
    let tmp = baseline_root.join("app.tmp");
    if tmp.exists() && fs::remove_dir_all(&tmp).is_err() {
        eprintln!("setup_warn: cannot clear stale baseline tmp");
        return false;
    }
    // Pre-scan total bytes so the copy can report meaningful progress.
    let total_bytes: u64 = dir_total_bytes(&pkg_root.join("app"));
    let manifest = match copy_dir_manifest(&pkg_root.join("app"), &tmp, total_bytes, progress) {
        Some(m) => m,
        None => {
            eprintln!("setup_warn: cannot copy official app to baseline");
            return false;
        }
    };
    if manifest.is_empty() {
        eprintln!("setup_warn: baseline manifest is empty; refusing to commit");
        let _ = fs::remove_dir_all(&tmp);
        return false;
    }
    if baseline_app.exists() && fs::remove_dir_all(baseline_app).is_err() {
        eprintln!("setup_warn: cannot replace previous baseline app");
        return false;
    }
    if fs::rename(&tmp, baseline_app).is_err() {
        eprintln!("setup_warn: cannot commit baseline app");
        return false;
    }
    // Verified StagingV1 state file with the FULL per-file hash manifest
    // (parity with the initial codex.ntfs.recoverable-backup.v1 design; the
    // launcher's invariant check requires files_staged == manifest length).
    let files_staged = manifest.len() as u64;
    let total_bytes: u64 = manifest.iter().map(|e| e.bytes).sum();
    let entries_json = manifest
        .iter()
        .map(|e| {
            format!(
                "{{\"relative_path\":\"{}\",\"bytes\":{},\"sha256\":\"{}\"}}",
                e.rel, e.bytes, e.sha
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let state = format!(
        "{{\"schema\":\"chatgpt_fix.staging.v1\",\"source_package_full_name\":\"{}\",\"source_version\":\"{}\",\"source_hash_manifest\":[{}],\"staging_root\":\"{}\",\"baseline_root\":\"{}\",\"files_staged\":{},\"total_bytes\":{},\"state\":\"verified\",\"created_at_utc\":\"{}\"}}\n",
        pkg_root.to_string_lossy().replace('\\', "/"),
        current_version,
        entries_json,
        baseline_root.to_string_lossy().replace('\\', "/"),
        baseline_root.to_string_lossy().replace('\\', "/"),
        files_staged,
        total_bytes,
        utc_now_compact()
    );
    fs::write(baseline_root.join("state.json"), state).is_ok()
}

/// One manifest entry: relative path, size, lowercase hex SHA-256.
struct ManifestEntry {
    rel: String,
    bytes: u64,
    sha: String,
}

/// Total bytes under `root` (quick pre-scan for progress reporting).
fn dir_total_bytes(root: &Path) -> u64 {
    fn walk(dir: &Path, acc: &mut u64) {
        if let Ok(entries) = fs::read_dir(chatgpt_fix_core::long_path(dir)) {
            for entry in entries.flatten() {
                let p = dir.join(entry.file_name());
                if let Ok(md) = fs::symlink_metadata(chatgpt_fix_core::long_path(&p)) {
                    if md.is_dir() {
                        walk(&p, acc);
                    } else {
                        *acc += md.len();
                    }
                }
            }
        }
    }
    let mut total = 0u64;
    walk(root, &mut total);
    total
}

/// Recursively copy `src` -> `dst`, hashing every file while copying.
/// Returns the full manifest (relative path / bytes / SHA-256). The progress
/// callback `(done_bytes, total_bytes)` fires after every file.
fn copy_dir_manifest(
    src: &Path,
    dst: &Path,
    total_bytes: u64,
    progress: &dyn Fn(u64, u64),
) -> Option<Vec<ManifestEntry>> {
    fs::create_dir_all(dst).ok()?;
    let mut out = Vec::new();
    let mut done_bytes = 0u64;
    copy_dir_manifest_inner(
        src,
        src,
        dst,
        &mut done_bytes,
        total_bytes,
        progress,
        &mut out,
    )?;
    Some(out)
}

fn copy_dir_manifest_inner(
    src: &Path,
    root_src: &Path,
    dst: &Path,
    done_bytes: &mut u64,
    total_bytes: u64,
    progress: &dyn Fn(u64, u64),
    out: &mut Vec<ManifestEntry>,
) -> Option<()> {
    // Long-path aware: read_dir must see the `\\?\` form for deep trees
    // (>260 chars), but paths are rebuilt from the non-prefixed `src` so the
    // manifest `strip_prefix` below keeps working.
    let entries = fs::read_dir(chatgpt_fix_core::long_path(src)).ok()?;
    for entry in entries.flatten() {
        let from = src.join(entry.file_name());
        let to = dst.join(entry.file_name());
        let md = fs::symlink_metadata(chatgpt_fix_core::long_path(&from)).ok()?;
        if md.is_dir() {
            // Regression fix: the 1.0.1+ refactor dropped this mkdir, so any
            // package with subdirectories failed staging at the first subdir.
            fs::create_dir_all(chatgpt_fix_core::long_path(&to)).ok()?;
            copy_dir_manifest_inner(&from, root_src, &to, done_bytes, total_bytes, progress, out)?;
        } else {
            let bytes = fs::copy(
                chatgpt_fix_core::long_path(&from),
                chatgpt_fix_core::long_path(&to),
            )
            .ok()?;
            let data = fs::read(chatgpt_fix_core::long_path(&from)).ok()?;
            let sha = chatgpt_fix_core::sha256_bytes(&data).to_string();
            // Relative path must be anchored at the TOP-level source root so
            // every entry is unique (the 1.0.1+ refactor anchored it at the
            // current recursion level, flattening subdirectory entries into
            // colliding top-level names once recursion actually ran).
            let rel = from
                .strip_prefix(root_src)
                .ok()?
                .to_string_lossy()
                .replace('\\', "/");
            out.push(ManifestEntry { rel, bytes, sha });
            *done_bytes += bytes;
            progress(*done_bytes, total_bytes);
        }
    }
    Some(())
}

/// One-click configuration: locate the official OpenAI.Codex package, stage a
/// user-owned baseline copy, write `<root>/current.json` pointing at it, and
/// create/repair the `ChatGPT.lnk` and `ChatGPT-Fix-Launcher.lnk` shortcuts so
/// the user is done after a single double-click install.
fn complete_one_click_config(root: &Path, launcher: &Path, progress: &dyn Fn(u64, u64)) -> bool {
    let mut ok = true;

    // 1. Detect the official package install location via Get-AppxPackage.
    let mut app_root: Option<String> = None;
    let ps = match find_powershell() {
        Ok(path) => path,
        Err(error) => {
            eprintln!("package discovery failed closed: {error}");
            return false;
        }
    };
    if let Ok(output) = std::process::Command::new(&ps)
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "(Get-AppxPackage -Name 'OpenAI.Codex' -ErrorAction SilentlyContinue | Sort-Object Version -Descending | Select-Object -First 1 -ExpandProperty InstallLocation)",
        ])
        .output()
        && output.status.success()
    {
        let text = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        if !text.is_empty() {
            app_root = Some(text);
        }
    }
    if let Some(pkg_root) = app_root.as_deref() {
        let exe = Path::new(pkg_root).join("app").join("ChatGPT.exe");
        if exe.is_file() {
            // 2. Stage an immutable user-owned baseline (the ORIGINAL NTFS
            // mitigation, plan P3/A3): copy the official app out of the
            // protected WindowsApps store into baselines/<package>/app and
            // point current.json at that copy. Running ChatGPT.exe directly
            // from WindowsApps leaks kernel nonpaged pool (~440 MB/min
            // observed); the user-local copy runs as a plain file tree and
            // does not.
            let pkg_dir = Path::new(pkg_root)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "package".to_owned());
            let baseline_root = root.join("baselines").join(&pkg_dir);
            let baseline_app = baseline_root.join("app");
            if stage_baseline(
                Path::new(pkg_root),
                &baseline_root,
                &baseline_app,
                &exe,
                progress,
            ) {
                // Pointer must reference the baseline ROOT (the directory that
                // holds state.json) so the launcher's verified-baseline check
                // passes; the executable lives at <root>/app/ChatGPT.exe.
                let pointer = format!(
                    "{{\"schema\":\"chatgpt_fix.pointer.v1\",\"baseline_root\":\"{}\"}}\n",
                    baseline_root.to_string_lossy().replace('\\', "/")
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
                eprintln!("setup_warn: baseline staging failed; current.json unchanged");
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

    // 3. Create/repair shortcuts. IMPORTANT (user feedback): the fix is a
    // wrapper and must NOT override the official startup method. So:
    //   - ChatGPT.lnk keeps the OFFICIAL launch (shell:AppsFolder AUMID),
    //     preserving the official icon and startup path; a package update
    //     never breaks it because no version path is embedded.
    //   - ChatGPT-Fix-Launcher.lnk points at our Launcher (the wrapper) as a
    //     separate entry, with the official ChatGPT icon.
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

    // Official package family name for the AUMID launch. The publisher id is
    // stable across package versions, so the AUMID survives updates.
    const AUMID: &str = "shell:AppsFolder\\OpenAI.Codex_2p2nqsd0c76g0!App";
    // Icons: prefer the version-independent LOCAL icos so a package update
    // never breaks the shortcut icons. The two shortcuts get distinct
    // icons so the user can tell them apart at a glance:
    //   - ChatGPT.lnk          -> chatgpt-icon.ico     (official ChatGPT logo)
    //   - ChatGPT-Fix-Launcher -> chatgpt-fix-icon.ico (same logo + badge)
    let local_official = root.join("chatgpt-icon.ico");
    let local_fix = root.join("chatgpt-fix-icon.ico");
    let fallback_exe = app_root
        .as_deref()
        .map(|p| Path::new(p).join("app").join("ChatGPT.exe"))
        .filter(|p| p.is_file())
        .map(|p| p.to_string_lossy().into_owned());
    let official_icon = if local_official.is_file() {
        local_official.to_string_lossy().into_owned()
    } else {
        fallback_exe.clone().unwrap_or_default()
    };
    let fix_icon = if local_fix.is_file() {
        local_fix.to_string_lossy().into_owned()
    } else {
        fallback_exe.unwrap_or_default()
    };

    let ps_args = format!(
        "$sh = New-Object -ComObject WScript.Shell; try {{ $s1 = $sh.CreateShortcut('{}'); $s1.TargetPath = 'C:\\Windows\\explorer.exe'; $s1.Arguments = '{}'; $s1.WorkingDirectory = 'C:\\Windows'; $s1.Description = 'ChatGPT (official)'; $s1.IconLocation = '{}'; $s1.Save(); if (-not (Test-Path -LiteralPath '{}')) {{ throw 'ChatGPT.lnk not created' }} }} catch {{ Write-Error $_; exit 1 }}; try {{ $s2 = $sh.CreateShortcut('{}'); $s2.TargetPath = '{}'; $s2.WorkingDirectory = '{}'; $s2.Description = 'ChatGPT-Fix-Launcher (wrapper)'; $s2.IconLocation = '{}'; $s2.Save(); if (-not (Test-Path -LiteralPath '{}')) {{ throw 'Launcher.lnk not created' }} }} catch {{ Write-Error $_; exit 1 }}",
        lnk_chatgpt.to_string_lossy().replace('\'', "''"),
        AUMID.replace('\'', "''"),
        official_icon.replace('\'', "''"),
        lnk_chatgpt.to_string_lossy().replace('\'', "''"),
        lnk_launcher.to_string_lossy().replace('\'', "''"),
        shortcut_target.replace('\'', "''"),
        work_dir.replace('\'', "''"),
        fix_icon.replace('\'', "''"),
        lnk_launcher.to_string_lossy().replace('\'', "''"),
    );
    // Run, and on failure retry once (transient locks from a still-running
    // app can prevent Save); only then report failure.
    let mut created_ok = false;
    for attempt in 0..2 {
        if let Ok(output) = std::process::Command::new(&ps)
            .args(["-NoProfile", "-NonInteractive", "-Command", &ps_args])
            .output()
        {
            if output.status.success() && lnk_chatgpt.exists() && lnk_launcher.exists() {
                created_ok = true;
                break;
            }
            if attempt == 1 {
                eprintln!(
                    "setup_warn: shortcut creation failed after retry: {}",
                    String::from_utf8_lossy(&output.stderr).trim()
                );
            }
        } else if attempt == 1 {
            eprintln!("setup_warn: cannot run powershell for shortcut creation");
        }
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
    if !created_ok {
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
        None => gui::run_gui_install(),
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
        Some(_) => {
            eprintln!(
                "install_forbidden: unsupported invocation; use 'install --source <dir>', 'uninstall', or '--version'"
            );
            ExitCode::from(4)
        }
    }
}
