use std::env;
use std::fs;
use std::path::PathBuf;

use crate::ContractError;

const BACKUP_NAME: &str = "config.toml.bak-chatgpt-fix";

/// Remove machine-bound marketplace state and repair the legacy Windows
/// sandbox spelling that caused the app to reject the whole configuration.
///
/// The transformation is deliberately narrow: plugin enablement, model
/// selection, provider settings, and all other user-owned values are kept.
pub fn sanitize_codex_config_text(input: &str) -> (String, bool) {
    let mut output = String::with_capacity(input.len());
    let mut changed = false;
    let mut skip_reserved_marketplace = false;
    let mut in_windows_section = false;

    for raw_line in input.split_inclusive('\n') {
        let body_len = raw_line.trim_end_matches(&['\r', '\n'][..]).len();
        let body = &raw_line[..body_len];
        let ending = &raw_line[body_len..];
        let trimmed = body.trim();

        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            skip_reserved_marketplace = trimmed == "[marketplaces.openai-bundled]";
            in_windows_section = trimmed == "[windows]";
            if skip_reserved_marketplace {
                changed = true;
                continue;
            }
        } else if skip_reserved_marketplace {
            changed = true;
            continue;
        }

        if in_windows_section
            && (trimmed == r#"sandbox = "low""# || trimmed == "sandbox = 'low'")
        {
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
    let (sanitized, changed) = sanitize_codex_config_text(input);
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
