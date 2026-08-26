use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use crate::ContractError;

const BACKUP_NAME: &str = "config.toml.bak-chatgpt-fix";

/// Normalize machine-bound marketplace state and repair the legacy Windows
/// sandbox spelling that caused the app to reject the whole configuration.
///
/// The transformation is deliberately narrow: plugin enablement, model
/// selection, provider settings, and all other user-owned values are kept.
pub fn sanitize_codex_config_text(input: &str) -> (String, bool) {
    sanitize_codex_config_text_for_home(input, None)
}

/// Normalize the reserved marketplace declaration and repair the legacy
/// Windows sandbox spelling.
///
/// `openai-bundled` is a reserved marketplace name owned by the app itself:
/// the desktop app-server rejects ANY external registration of it
/// ("marketplace `openai-bundled` is reserved and cannot be added from this
/// source"). A config that declares it as a local/path source therefore puts
/// the app into a repeated marketplace/add + reconcile failure loop on every
/// focus, which shows up as lag, frozen conversations and a slow start.
///
/// The whole `[marketplaces.openai-bundled]` section is dropped. The
/// `[plugins."*@openai-bundled"]` enablement sections are ALSO dropped: the
/// app-server keeps only bundled plugins that appear in its own discovered
/// marketplaces, and a config-only enablement for the (never-discoverable)
/// reserved marketplace makes "configured non-curated plugin no longer exists
/// in discovered marketplaces" reconcile failures that UNINSTALL the bundled
/// plugins (browser/chrome/computer-use/visualize/codex-app-tools disappear
/// from the app). The bundled plugins remain natively available: the app
/// materializes them from its own resources and re-registers them at startup.
///
/// `codex_home` is retained in the signature for callers that used the
/// previous normalize-in-place behavior; the reserved marketplace is now
/// removed regardless of the home path.
pub fn sanitize_codex_config_text_for_home(
    input: &str,
    _codex_home: Option<&Path>,
) -> (String, bool) {
    let mut output = String::with_capacity(input.len());
    let mut changed = false;
    let mut in_reserved_marketplace = false;
    let mut in_orphan_plugin = false;
    let mut in_windows_section = false;

    for raw_line in input.split_inclusive('\n') {
        let body_len = raw_line.trim_end_matches(&['\r', '\n'][..]).len();
        let body = &raw_line[..body_len];
        let ending = &raw_line[body_len..];
        let trimmed = body.trim();

        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            in_reserved_marketplace = trimmed == "[marketplaces.openai-bundled]";
            in_orphan_plugin = trimmed
                .strip_prefix("[plugins.\"")
                .and_then(|rest| rest.strip_suffix("\"]"))
                .is_some_and(|name| name.ends_with("@openai-bundled"));
            in_windows_section = trimmed == "[windows]";
        }

        if in_reserved_marketplace || in_orphan_plugin {
            // Drop the reserved marketplace declaration and the orphan
            // bundled-plugin enablement sections entirely (headers and their
            // keys). A stale optional entry must never stop the app, and
            // either presence drives app-server reconcile failures.
            changed = true;
            continue;
        }

        if in_windows_section && (trimmed == r#"sandbox = "low""# || trimmed == "sandbox = 'low'") {
            let indent_len = body.len() - body.trim_start().len();
            output.push_str(&body[..indent_len]);
            output.push_str(r#"sandbox = "elevated""#);
            output.push_str(ending);
            changed = true;
            continue;
        }

        output.push_str(raw_line);
    }

    // `split_inclusive` yields no item for an empty input and preserves all
    // existing newline styles for non-empty input.
    (output, changed)
}

/// Sanitize the active `%CODEX_HOME%\config.toml` before launching the
/// isolated baseline. Returns `true` when a backup and rewrite occurred.
pub fn sanitize_codex_config() -> Result<bool, ContractError> {
    let home = match env::var_os("CODEX_HOME") {
        Some(value) if !value.is_empty() => PathBuf::from(value),
        _ => match env::var_os("USERPROFILE") {
            Some(value) if !value.is_empty() => PathBuf::from(value).join(".codex"),
            _ => return Ok(false),
        },
    };
    let config_path = home.join("config.toml");
    let bytes = match fs::read(&config_path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => {
            return Err(ContractError::new(
                "config_unreadable",
                config_path.to_string_lossy(),
                format!("cannot read config.toml: {error}"),
            ));
        }
    };
    let input = std::str::from_utf8(&bytes).map_err(|error| {
        ContractError::new(
            "config_invalid_encoding",
            config_path.to_string_lossy(),
            format!("config.toml is not UTF-8: {error}"),
        )
    })?;
    let (sanitized, changed) = sanitize_codex_config_text_for_home(input, Some(&home));
    if !changed {
        return Ok(false);
    }

    let backup_path = home.join(BACKUP_NAME);
    if !backup_path.exists() {
        fs::copy(&config_path, &backup_path).map_err(|error| {
            ContractError::new(
                "config_backup_failed",
                backup_path.to_string_lossy(),
                format!("cannot preserve the original config.toml: {error}"),
            )
        })?;
    }

    let temp_path = config_path.with_extension("toml.chatgpt-fix.tmp");
    fs::write(&temp_path, sanitized.as_bytes()).map_err(|error| {
        ContractError::new(
            "config_write_failed",
            temp_path.to_string_lossy(),
            format!("cannot write sanitized config.toml: {error}"),
        )
    })?;
    if let Err(first_error) = fs::rename(&temp_path, &config_path) {
        // Windows does not replace an existing file with rename. The
        // original is already preserved in BACKUP_NAME before this point.
        fs::remove_file(&config_path).map_err(|error| {
            ContractError::new(
                "config_commit_failed",
                config_path.to_string_lossy(),
                format!("cannot replace config.toml after rename error {first_error}: {error}"),
            )
        })?;
        fs::rename(&temp_path, &config_path).map_err(|error| {
            ContractError::new(
                "config_commit_failed",
                config_path.to_string_lossy(),
                format!("cannot commit sanitized config.toml: {error}"),
            )
        })?;
    }
    Ok(true)
}

/// Desktop-section preservation ("user desktop settings must survive
/// restarts, without touching the config manager").
///
/// The desktop app writes its own settings — appearance theme, locale,
/// window behavior — into the `[desktop]` section of
/// `%CODEX_HOME%\config.toml` on exit. Config managers (e.g. cc-switch)
/// rewrite config.toml from their own provider+common template on provider
/// switches and app restarts, DROPPING sections they do not manage
/// (observed: `[desktop]` and `[windows]` vanish on a cc-switch rewrite).
/// That resets the user's appearance/language on the next app start.
///
/// This function keeps a last-known-good copy of the `[desktop]` section in
/// `<state_dir>/desktop-config.toml` (its own state dir; the config
/// manager's data is never touched):
///
/// - When config.toml still contains a `[desktop]` section, the copy is
///   refreshed from it (the live value always wins — the user may have just
///   changed the theme in the app).
/// - When config.toml LOST the section (external rewrite), the copy is
///   appended back, preserving the user's own choices. The original file is
///   backed up once as `config.toml.bak-chatgpt-fix-desktop`.
///
/// Returns `true` when a restore write occurred. Idempotent: a second run
/// with the section present only refreshes the copy.
pub fn preserve_desktop_section(state_dir: &Path) -> Result<bool, ContractError> {
    let home = match env::var_os("CODEX_HOME") {
        Some(value) if !value.is_empty() => PathBuf::from(value),
        _ => match env::var_os("USERPROFILE") {
            Some(value) if !value.is_empty() => PathBuf::from(value).join(".codex"),
            _ => return Ok(false),
        },
    };
    preserve_desktop_section_at(&home.join("config.toml"), state_dir)
}

/// Path-explicit core of `preserve_desktop_section` (kept separate so tests
/// can exercise the copy/restore logic without touching the process
/// environment).
pub fn preserve_desktop_section_at(
    config_path: &Path,
    state_dir: &Path,
) -> Result<bool, ContractError> {
    let snapshot_path = state_dir.join("desktop-config.toml");

    let text = match fs::read_to_string(config_path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => {
            return Err(ContractError::new(
                "config_unreadable",
                config_path.to_string_lossy(),
                format!("cannot read config.toml: {error}"),
            ));
        }
    };

    match extract_desktop_section(&text) {
        Some(section) => {
            // Live section present: refresh the copy (atomic swap).
            if let Some(parent) = snapshot_path.parent() {
                fs::create_dir_all(parent).map_err(|error| {
                    ContractError::new(
                        "state_dir_create",
                        parent.to_string_lossy(),
                        format!("cannot create state dir: {error}"),
                    )
                })?;
            }
            let tmp = snapshot_path.with_extension("toml.tmp");
            fs::write(&tmp, section.as_bytes()).map_err(|error| {
                ContractError::new(
                    "snapshot_write",
                    tmp.to_string_lossy(),
                    format!("cannot write desktop copy: {error}"),
                )
            })?;
            let _ = fs::rename(&tmp, &snapshot_path);
            Ok(false)
        }
        None => {
            // Section lost (external rewrite): restore from the copy.
            let snapshot = match fs::read_to_string(&snapshot_path) {
                Ok(snapshot) => snapshot,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
                Err(error) => {
                    return Err(ContractError::new(
                        "snapshot_unreadable",
                        snapshot_path.to_string_lossy(),
                        format!("cannot read desktop copy: {error}"),
                    ));
                }
            };
            let section = match extract_desktop_section(&snapshot) {
                Some(section) => section,
                None => return Ok(false),
            };
            let mut restored = text;
            if !restored.ends_with('\n') {
                restored.push('\n');
            }
            if !restored.ends_with("\n\n") {
                restored.push('\n');
            }
            restored.push_str(section.trim_end_matches(['\r', '\n']));
            restored.push('\n');

            let backup_path = config_path.with_file_name("config.toml.bak-chatgpt-fix-desktop");
            if !backup_path.exists() {
                fs::copy(config_path, &backup_path).map_err(|error| {
                    ContractError::new(
                        "config_backup_failed",
                        backup_path.to_string_lossy(),
                        format!("cannot preserve the original config.toml: {error}"),
                    )
                })?;
            }
            let temp_path = config_path.with_extension("toml.desktop.tmp");
            fs::write(&temp_path, restored.as_bytes()).map_err(|error| {
                ContractError::new(
                    "config_write_failed",
                    temp_path.to_string_lossy(),
                    format!("cannot write restored config.toml: {error}"),
                )
            })?;
            if let Err(first_error) = fs::rename(&temp_path, config_path) {
                fs::remove_file(config_path).map_err(|error| {
                    ContractError::new(
                        "config_commit_failed",
                        config_path.to_string_lossy(),
                        format!(
                            "cannot replace config.toml after rename error {first_error}: {error}"
                        ),
                    )
                })?;
                fs::rename(&temp_path, config_path).map_err(|error| {
                    ContractError::new(
                        "config_commit_failed",
                        config_path.to_string_lossy(),
                        format!("cannot commit restored config.toml: {error}"),
                    )
                })?;
            }
            Ok(true)
        }
    }
}

/// Extract the `[desktop]` section (header + keys, including `[desktop.*]`
/// subsections) as verbatim text, or `None` when the section is absent.
fn extract_desktop_section(input: &str) -> Option<String> {
    let mut output = String::new();
    let mut in_section = false;
    for raw_line in input.split_inclusive('\n') {
        let body_len = raw_line.trim_end_matches(&['\r', '\n'][..]).len();
        let trimmed = raw_line[..body_len].trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            if trimmed == "[desktop]" || trimmed.starts_with("[desktop.") {
                in_section = true;
            } else if in_section {
                break;
            }
        }
        if in_section {
            output.push_str(raw_line);
        }
    }
    if output.is_empty() {
        None
    } else {
        Some(output)
    }
}
