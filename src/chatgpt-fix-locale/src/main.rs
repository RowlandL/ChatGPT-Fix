// ChatGPT-Fix-Locale: dedicated repair tool for the Chinese UI.
//
// The desktop app translates its UI only when a server-fetched Statsig layer
// (enable_i18n) is available; without network the cached layer expires and
// the UI falls back to English. This tool patches the renderer bundle inside
// the launcher-owned baseline's app.asar COPY so `enable_i18n` defaults to
// true, making the Chinese UI persist with no network dependency.
//
// The official WindowsApps package is never touched.

use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

const PRODUCT_NAME: &str = "ChatGPT-Fix-Locale";
const COMMIT_FILE: &str = "locale-commit.json";
const BACKUP_NAME: &str = "app.asar.pre-locale";
const OUT_NAME: &str = "app.asar.locale-new";

fn default_program_root() -> Option<PathBuf> {
    std::env::var("LOCALAPPDATA")
        .ok()
        .filter(|v| !v.is_empty())
        .map(|base| PathBuf::from(base).join("Programs").join("ChatGPT-Fix"))
}

/// Parse the pointer's `baseline_root` string field. The pointer format is
/// fixed by the launcher (`chatgpt_fix.pointer.v1`); a missing/null baseline
/// fails closed here.
fn parse_baseline_root(pointer: &str) -> Result<PathBuf, String> {
    let marker = "\"baseline_root\":\"";
    let start = pointer
        .find(marker)
        .ok_or_else(|| "pointer has no baseline_root field".to_owned())?;
    let rest = &pointer[start + marker.len()..];
    let end = rest
        .find('"')
        .ok_or_else(|| "pointer baseline_root is malformed".to_owned())?;
    let value = &rest[..end];
    if value.is_empty() || value == "null" {
        return Err("pointer points at no baseline".to_owned());
    }
    Ok(PathBuf::from(value))
}

/// Locate `app.asar` under a baseline root. The official package layout puts
/// it in `app/resources/app.asar`; a flat layout uses `app.asar` directly.
fn locate_app_asar(baseline: &Path) -> Result<PathBuf, String> {
    for candidate in [
        baseline.join("app").join("resources").join("app.asar"),
        baseline.join("app.asar"),
    ] {
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    Err(format!(
        "app.asar not found under baseline {}",
        baseline.display()
    ))
}

/// The launcher-owned baseline is safe to patch only while the app is not
/// running from it (Windows locks the asar while the app uses it).
fn chatgpt_running() -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let output = Command::new("tasklist")
            .creation_flags(0x0800_0000)
            .args(["/FI", "IMAGENAME eq ChatGPT.exe", "/FO", "CSV", "/NH"])
            .output();
        match output {
            Ok(output) if output.status.success() => {
                let hay = output.stdout.to_ascii_lowercase();
                hay.windows(b"chatgpt.exe".len())
                    .any(|w| w == b"chatgpt.exe")
            }
            _ => false,
        }
    }
    #[cfg(not(windows))]
    {
        false
    }
}

fn resolve_script(program_root: &Path) -> PathBuf {
    if let Some(value) = std::env::var_os("CHATGPT_FIX_LOCALE_INJECT_SCRIPT") {
        let path = PathBuf::from(value);
        if path.is_file() {
            return path;
        }
    }
    let installed = program_root.join("scripts").join("inject-locale-i18n.js");
    if installed.is_file() {
        return installed;
    }
    // Release binaries must use the script installed beside the tool. There
    // is deliberately no source-tree fallback: embedding CARGO_MANIFEST_DIR
    // would leak a build-machine path into the published executable and could
    // make a clean installation fail to find its own payload.
    program_root.join("scripts").join("inject-locale-i18n.js")
}

fn resolve_node() -> String {
    std::env::var("CHATGPT_FIX_NODE").unwrap_or_else(|_| "node".to_owned())
}

/// Run the inject script with node. `@electron/asar` must resolve; the
/// install-local runtime (<program-root>\ntc\node_modules) is prepended to
/// NODE_PATH, followed by any caller-provided NODE_PATH.
fn run_script(program_root: &Path, script: &Path, args: &[&OsStr]) -> Result<String, String> {
    let node = resolve_node();
    let mut command = Command::new(&node);
    let mut node_path = program_root
        .join("ntc")
        .join("node_modules")
        .to_string_lossy()
        .into_owned();
    if let Ok(existing) = std::env::var("NODE_PATH")
        && !existing.is_empty()
    {
        node_path.push(';');
        node_path.push_str(&existing);
    }
    command.env("NODE_PATH", &node_path);
    command.arg(script);
    for arg in args {
        command.arg(arg);
    }
    let output = command
        .output()
        .map_err(|error| format!("cannot run node ({node}): {error}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    if !output.status.success() {
        return Err(format!(
            "inject script failed (exit {:?}): {}",
            output.status.code(),
            stderr.trim()
        ));
    }
    // The receipt/status JSON is the last non-empty stdout line.
    Ok(stdout
        .lines()
        .rev()
        .find(|line| line.trim_start().starts_with('{'))
        .unwrap_or(&stdout)
        .trim()
        .to_owned())
}

fn current_baseline(program_root: &Path) -> Result<PathBuf, String> {
    let pointer_path = program_root.join("current.json");
    let pointer = fs::read_to_string(&pointer_path)
        .map_err(|error| format!("cannot read {}: {error}", pointer_path.display()))?;
    parse_baseline_root(&pointer)
}

fn run_status(program_root: &Path) -> ExitCode {
    let baseline = match current_baseline(program_root) {
        Ok(baseline) => baseline,
        Err(error) => {
            eprintln!("locale_status_failed: {error}");
            return ExitCode::from(3);
        }
    };
    let app_asar = match locate_app_asar(&baseline) {
        Ok(path) => path,
        Err(error) => {
            eprintln!("locale_status_failed: {error}");
            return ExitCode::from(3);
        }
    };
    let script = resolve_script(program_root);
    if !script.is_file() {
        eprintln!(
            "locale_status_failed: inject script missing at {}",
            script.display()
        );
        return ExitCode::from(4);
    }
    match run_script(
        program_root,
        &script,
        &[OsStr::new("check"), app_asar.as_os_str()],
    ) {
        Ok(json) => {
            println!("{json}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            // `check` exits 1 when the bundle still needs the patch; surface
            // the status JSON from the error and reflect it in the exit code.
            eprintln!("locale_status: {error}");
            ExitCode::from(1)
        }
    }
}

fn run_fix(program_root: &Path) -> ExitCode {
    if chatgpt_running() {
        eprintln!(
            "locale_fix_failed: ChatGPT.exe is running; close the app before patching the baseline"
        );
        return ExitCode::from(4);
    }
    let baseline = match current_baseline(program_root) {
        Ok(baseline) => baseline,
        Err(error) => {
            eprintln!("locale_fix_failed: {error}");
            return ExitCode::from(3);
        }
    };
    let app_asar = match locate_app_asar(&baseline) {
        Ok(path) => path,
        Err(error) => {
            eprintln!("locale_fix_failed: {error}");
            return ExitCode::from(3);
        }
    };
    let resources_dir = match app_asar.parent() {
        Some(dir) => dir.to_path_buf(),
        None => {
            eprintln!("locale_fix_failed: cannot resolve resources dir");
            return ExitCode::from(3);
        }
    };
    let backup_path = resources_dir.join(BACKUP_NAME);
    let out_path = resources_dir.join(OUT_NAME);
    let commit_path = resources_dir.join(COMMIT_FILE);
    let unpacked = resources_dir.join("app.asar.unpacked");

    // Idempotency: a committed, still-patched asar needs no work.
    if commit_path.is_file() {
        let script = resolve_script(program_root);
        if let Ok(json) = run_script(
            program_root,
            &script,
            &[OsStr::new("check"), app_asar.as_os_str()],
        ) && json.contains("\"status\":\"patched\"")
        {
            println!(
                "{{\"schema\":\"chatgpt_fix.locale_fix.v1\",\"status\":\"already_fixed\",\"baseline\":\"{}\"}}",
                baseline.to_string_lossy().replace('\\', "/")
            );
            return ExitCode::SUCCESS;
        }
    }

    if backup_path.exists() || out_path.exists() {
        eprintln!(
            "locale_fix_failed: existing locale transaction state at {}; inspect and remove it before retrying",
            resources_dir.display()
        );
        return ExitCode::from(4);
    }

    let script = resolve_script(program_root);
    if !script.is_file() {
        eprintln!(
            "locale_fix_failed: inject script missing at {}",
            script.display()
        );
        return ExitCode::from(4);
    }

    let receipt = match run_script(
        program_root,
        &script,
        &[
            OsStr::new("fix"),
            app_asar.as_os_str(),
            out_path.as_os_str(),
            unpacked.as_os_str(),
        ],
    ) {
        Ok(receipt) => receipt,
        Err(error) => {
            let _ = fs::remove_file(&out_path);
            eprintln!("locale_fix_failed: {error}");
            return ExitCode::from(4);
        }
    };
    if !receipt.contains("\"ok\":true") {
        let _ = fs::remove_file(&out_path);
        eprintln!("locale_fix_failed: inject script reported failure: {receipt}");
        return ExitCode::from(4);
    }
    if !out_path.is_file() {
        eprintln!("locale_fix_failed: inject script did not produce the patched asar");
        return ExitCode::from(4);
    }

    // Commit: preserve the original asar, then atomically replace it.
    if let Err(error) = fs::rename(&app_asar, &backup_path) {
        eprintln!("locale_fix_failed: cannot preserve original asar: {error}");
        let _ = fs::remove_file(&out_path);
        return ExitCode::from(4);
    }
    if let Err(error) = fs::rename(&out_path, &app_asar) {
        // Roll the backup back on failure.
        let _ = fs::rename(&backup_path, &app_asar);
        let _ = fs::remove_file(&out_path);
        eprintln!("locale_fix_failed: cannot install patched asar: {error}");
        return ExitCode::from(4);
    }

    let before = receipt
        .split("\"before_sha256\":\"")
        .nth(1)
        .and_then(|s| s.split('"').next())
        .unwrap_or("unrecorded");
    let after = receipt
        .split("\"after_sha256\":\"")
        .nth(1)
        .and_then(|s| s.split('"').next())
        .unwrap_or("unrecorded");
    let commit = format!(
        "{{\"schema\":\"chatgpt_fix.locale_commit.v1\",\"ok\":true,\"before_sha256\":\"{before}\",\"after_sha256\":\"{after}\",\"patched_at_utc\":\"{}\"}}\n",
        chatgpt_fix_core::utc_now_rfc3339()
    );
    if let Err(error) = fs::write(&commit_path, commit.as_bytes()) {
        eprintln!("locale_fix_warning: cannot write commit marker: {error}");
    }

    println!(
        "{{\"schema\":\"chatgpt_fix.locale_fix.v1\",\"status\":\"fixed\",\"baseline\":\"{}\",\"before_sha256\":\"{before}\",\"after_sha256\":\"{after}\"}}",
        baseline.to_string_lossy().replace('\\', "/")
    );
    ExitCode::SUCCESS
}

fn main() -> ExitCode {
    let arguments = std::env::args_os().skip(1).collect::<Vec<_>>();

    match arguments.as_slice() {
        [argument] if argument == OsStr::new("--version") => {
            chatgpt_fix_core::run_version_command(PRODUCT_NAME)
        }
        [command] if command == OsStr::new("status") => match default_program_root() {
            Some(root) => run_status(&root),
            None => {
                eprintln!("locale_status_failed: LOCALAPPDATA is not set");
                ExitCode::from(4)
            }
        },
        [command] if command == OsStr::new("fix") => match default_program_root() {
            Some(root) => run_fix(&root),
            None => {
                eprintln!("locale_fix_failed: LOCALAPPDATA is not set");
                ExitCode::from(4)
            }
        },
        [command, option, root]
            if command == OsStr::new("fix") && option == OsStr::new("--root") =>
        {
            run_fix(Path::new(root))
        }
        [command, option, root]
            if command == OsStr::new("status") && option == OsStr::new("--root") =>
        {
            run_status(Path::new(root))
        }
        _ => {
            eprintln!("Usage: {PRODUCT_NAME} [--version] [status|fix] [--root <program-root>]");
            ExitCode::from(2)
        }
    }
}
