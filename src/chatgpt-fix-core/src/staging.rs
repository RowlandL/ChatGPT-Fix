use std::fs::{self, File};
use std::io::Write as IoWrite;
use std::path::{Path, PathBuf};

use crate::json::write_string;
use crate::sha256::sha256_file;
use crate::{
    ContractError, LiveInspectionV1, SafeRelativePath, StagingFileEntry, StagingState, StagingV1,
};

/// The marker directory name for quarantined staging failures.
const QUARANTINE_DIR: &str = ".quarantine";
/// The staging receipt file name inside a staging root.
const STATE_FILE: &str = "state.json";
/// The hash manifest file name inside a staging root.
const HASH_MANIFEST_FILE: &str = "hashes.json";

/// Whitelisted file extensions copied from a package app directory.
///
/// The P3 contract copies only the manifest-declared app resource subset.
/// Anything outside this allowlist is refused so that no unrelated or
/// runtime state leaks into an immutable baseline.
const APP_WHITELIST_EXTENSIONS: &[&str] = &[
    "exe", "dll", "json", "asar", "pak", "txt", "ico", "png", "html", "js", "css", "map", "node",
];

fn contract_error(code: &'static str, field: &str, message: impl Into<String>) -> ContractError {
    ContractError::new(code, field, message)
}

/// Read a probe JSON, validate it as a `LiveInspectionV1`, and verify the
/// install location is a readable directory that is not a WindowsApps root
/// (live package paths are refused by the P3 contract).
fn read_inspection(probe_json_path: &Path) -> Result<LiveInspectionV1, ContractError> {
    let bytes = fs::read(probe_json_path).map_err(|error| {
        contract_error(
            "probe_json_unreadable",
            "probe_json",
            format!("probe JSON cannot be read: {error}"),
        )
    })?;
    let inspection = LiveInspectionV1::from_json(&bytes).map_err(|error| {
        contract_error(
            "probe_json_invalid",
            "probe_json",
            format!("probe JSON is invalid: {error}"),
        )
    })?;
    inspection.validate().map_err(|error| {
        contract_error(
            "probe_validation_failed",
            "probe_json",
            format!("probe validation failed: {error}"),
        )
    })?;

    // P3 refuses live WindowsApps package roots; only fixture/staging
    // directories may be staged. Normalize the path (lower-case + forward
    // slashes) before checking so that case variations and forward slashes
    // cannot bypass the check. When the path exists, canonicalize first so
    // 8.3 short names are also resolved; if canonicalization fails the raw
    // path check still applies (a live root that cannot be read must not be
    // staged either way).
    let install = Path::new(&inspection.install_location);
    let raw_normalized = inspection
        .install_location
        .to_lowercase()
        .replace('\\', "/");
    let normalized = match fs::canonicalize(install) {
        Ok(canonical) => canonical
            .to_string_lossy()
            .to_lowercase()
            .replace('\\', "/"),
        Err(_) => raw_normalized.clone(),
    };
    if normalized.contains("program files/windowsapps") {
        return Err(contract_error(
            "live_package_refused",
            "install_location",
            "staging a live WindowsApps package root is not authorized by A3",
        ));
    }
    let metadata = fs::symlink_metadata(install).map_err(|error| {
        contract_error(
            "install_location_unreadable",
            "install_location",
            format!("install location cannot be read: {error}"),
        )
    })?;
    if !metadata.is_dir() {
        return Err(contract_error(
            "install_location_not_dir",
            "install_location",
            "install location is not a directory",
        ));
    }

    Ok(inspection)
}

/// Walk the app directory and collect whitelisted regular files as relative
/// paths. Symlinks and reparse points are refused outright.
fn collect_whitelisted_files(root: &Path) -> Result<Vec<PathBuf>, ContractError> {
    let mut files = Vec::new();
    collect_whitelisted_files_inner(root, root, &mut files)?;
    files.sort();
    Ok(files)
}

fn collect_whitelisted_files_inner(
    root: &Path,
    current: &Path,
    out: &mut Vec<PathBuf>,
) -> Result<(), ContractError> {
    let entries = fs::read_dir(current).map_err(|error| {
        contract_error(
            "app_dir_unreadable",
            "install_location",
            format!("app directory cannot be read: {error}"),
        )
    })?;
    for entry in entries {
        let entry = entry.map_err(|error| {
            contract_error(
                "app_dir_entry",
                "install_location",
                format!("app directory entry cannot be read: {error}"),
            )
        })?;
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path).map_err(|error| {
            contract_error(
                "app_entry_metadata",
                "install_location",
                format!("entry metadata cannot be read: {error}"),
            )
        })?;
        if metadata.file_type().is_symlink() {
            return Err(contract_error(
                "symlink_refused",
                "install_location",
                format!("symlink is refused in staging: {}", path.display()),
            ));
        }
        // Reject any file with reparse/other attributes on Windows.
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
            if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
                return Err(contract_error(
                    "reparse_point_refused",
                    "install_location",
                    format!("reparse point is refused in staging: {}", path.display()),
                ));
            }
        }
        if metadata.is_dir() {
            collect_whitelisted_files_inner(root, &path, out)?;
        } else if metadata.is_file() {
            let relative = path.strip_prefix(root).map_err(|_| {
                contract_error("path_escape", "install_location", "path escapes app root")
            })?;
            // Validate the relative path is safe (no .., no drive, no separators).
            let relative_text = relative
                .to_str()
                .ok_or_else(|| {
                    contract_error("non_utf8_path", "install_location", "non-UTF-8 path")
                })?
                .replace('\\', "/");
            let safe = SafeRelativePath::parse(&relative_text).map_err(|error| {
                contract_error(
                    "unsafe_relative_path",
                    "install_location",
                    format!("unsafe relative path {relative_text}: {error}"),
                )
            })?;
            let extension = safe
                .as_str()
                .rsplit_once('.')
                .map(|(_, ext)| ext)
                .unwrap_or_default();
            if APP_WHITELIST_EXTENSIONS.contains(&extension) {
                out.push(PathBuf::from(safe.as_str()));
            }
        }
    }
    Ok(())
}

/// Copy a whitelisted file from the source app root into the staging root,
/// preserving the relative path.
fn copy_file(
    source_root: &Path,
    staging_root: &Path,
    relative: &Path,
) -> Result<u64, ContractError> {
    let source = source_root.join(relative);
    let target = staging_root.join(relative);
    if let Some(parent) = target.parent() {
        fs::create_dir_all(crate::path::long_path(parent)).map_err(|error| {
            contract_error(
                "staging_mkdir",
                "staging_root",
                format!("cannot create staging directory: {error}"),
            )
        })?;
    }
    // Reject target escape by construction: relative is SafeRelativePath-safe.
    let copied = fs::copy(
        &crate::path::long_path(&source),
        &crate::path::long_path(&target),
    )
    .map_err(|error| {
        contract_error(
            "staging_copy",
            "staging_root",
            format!("cannot copy {}: {error}", relative.display()),
        )
    })?;
    Ok(copied)
}

/// Write `state.json` (the `StagingV1` receipt) atomically into a staging root.
fn write_staging_state(staging_root: &Path, staging: &StagingV1) -> Result<(), ContractError> {
    let json = staging.to_json()?;
    let state_path = staging_root.join(STATE_FILE);
    let tmp_path = staging_root.join(format!("{STATE_FILE}.tmp"));
    {
        let mut file = File::create(&tmp_path).map_err(|error| {
            contract_error(
                "state_write",
                "state.json",
                format!("cannot create state file: {error}"),
            )
        })?;
        file.write_all(json.as_bytes()).map_err(|error| {
            contract_error(
                "state_write",
                "state.json",
                format!("cannot write state file: {error}"),
            )
        })?;
        file.write_all(b"\n").map_err(|error| {
            contract_error(
                "state_write",
                "state.json",
                format!("cannot write state newline: {error}"),
            )
        })?;
        file.sync_all().map_err(|error| {
            contract_error(
                "state_write",
                "state.json",
                format!("cannot sync state file: {error}"),
            )
        })?;
    }
    fs::rename(&tmp_path, &state_path).map_err(|error| {
        contract_error(
            "state_write",
            "state.json",
            format!("cannot finalize state file: {error}"),
        )
    })?;
    Ok(())
}

/// Stage a validated probe into an immutable staging root.
///
/// Only A3-authorized write paths (`staging_root` and final immutable
/// baseline root) are touched. The staging root must not already contain a
/// `state.json` (a failed staging is left in `.quarantine` and never
/// auto-deleted).
pub fn stage_from_probe(
    probe_json_path: &Path,
    out_root: &Path,
) -> Result<StagingV1, ContractError> {
    if out_root.join(STATE_FILE).exists() {
        return Err(contract_error(
            "staging_already_exists",
            "staging_root",
            "staging root already contains a state.json",
        ));
    }
    let inspection = read_inspection(probe_json_path)?;
    let source_root = Path::new(&inspection.install_location);

    let whitelisted = collect_whitelisted_files(source_root)?;
    if whitelisted.is_empty() {
        return Err(contract_error(
            "no_whitelisted_files",
            "install_location",
            "no whitelisted files found in app directory",
        ));
    }

    fs::create_dir_all(out_root).map_err(|error| {
        contract_error(
            "staging_mkdir",
            "staging_root",
            format!("cannot create staging root: {error}"),
        )
    })?;

    let mut manifest = Vec::with_capacity(whitelisted.len());
    let mut total_bytes: u64 = 0;
    for relative in &whitelisted {
        let copied = copy_file(source_root, out_root, relative)?;
        let target = out_root.join(relative);
        let digest = sha256_file(&target).map_err(|error| {
            contract_error(
                "staging_hash",
                "staging_root",
                format!("cannot hash {}: {error}", relative.display()),
            )
        })?;
        manifest.push(StagingFileEntry {
            relative_path: SafeRelativePath::parse(&relative.to_string_lossy().replace('\\', "/"))
                .map_err(|error| {
                    contract_error(
                        "unsafe_relative_path",
                        "staging_root",
                        format!("unsafe staged path: {error}"),
                    )
                })?,
            bytes: copied,
            sha256: digest,
        });
        total_bytes = total_bytes
            .checked_add(copied)
            .ok_or_else(|| contract_error("overflow", "total_bytes", "total bytes overflow"))?;
    }

    let now = crate::utc_now_rfc3339();
    let staging = StagingV1 {
        source_package_full_name: inspection.package_full_name.clone(),
        source_version: inspection.version.clone(),
        source_hash_manifest: manifest,
        staging_root: out_root.to_string_lossy().replace('\\', "/"),
        baseline_root: out_root
            .to_string_lossy()
            .replace('\\', "/")
            .trim_end_matches('/')
            .to_owned(),
        files_staged: whitelisted.len() as u64,
        total_bytes,
        state: StagingState::Staged,
        created_at_utc: now,
    };
    staging.validate()?;
    // Persist the standalone hash manifest (hashes.json) alongside the
    // state.json receipt so the manifest can be independently audited.
    write_hash_manifest(out_root, &staging.source_hash_manifest)?;
    write_staging_state(out_root, &staging)?;
    Ok(staging)
}

/// Verify a staged root: recompute every file SHA-256 and compare against
/// the recorded hash manifest. On any mismatch the state is flipped to
/// `quarantined` (fail closed) and the error is returned; the quarantine
/// marker directory is created but never auto-deleted.
pub fn verify_staging(staging_root: &Path) -> Result<StagingV1, ContractError> {
    let state_bytes = fs::read(staging_root.join(STATE_FILE)).map_err(|error| {
        contract_error(
            "state_unreadable",
            "state.json",
            format!("staging state cannot be read: {error}"),
        )
    })?;
    let mut staging = StagingV1::from_json(&state_bytes)?;
    if staging.state == StagingState::Quarantined {
        return Err(contract_error(
            "staging_quarantined",
            "state",
            "staging is already quarantined",
        ));
    }

    let mut verified_entries = Vec::with_capacity(staging.source_hash_manifest.len());
    for entry in &staging.source_hash_manifest {
        let file_path = staging_root.join(
            entry
                .relative_path
                .as_str()
                .replace('/', std::path::MAIN_SEPARATOR_STR),
        );
        let digest = match sha256_file(&file_path) {
            Ok(d) => d,
            Err(error) => {
                let reason = format!(
                    "missing or unreadable file {}: {error}",
                    entry.relative_path.as_str()
                );
                let _ = quarantine(staging_root, &reason);
                staging.state = StagingState::Quarantined;
                let _ = write_staging_state(staging_root, &staging);
                return Err(contract_error(
                    "verify_missing_file",
                    "staging_root",
                    reason,
                ));
            }
        };
        if digest != entry.sha256 {
            let reason = format!("hash mismatch for {}", entry.relative_path.as_str());
            let _ = quarantine(staging_root, &reason);
            staging.state = StagingState::Quarantined;
            let _ = write_staging_state(staging_root, &staging);
            return Err(contract_error(
                "verify_hash_mismatch",
                "staging_root",
                format!(
                    "hash mismatch for {}: expected {}, got {}",
                    entry.relative_path.as_str(),
                    entry.sha256.as_str(),
                    digest.as_str()
                ),
            ));
        }
        verified_entries.push(StagingFileEntry {
            relative_path: entry.relative_path.clone(),
            bytes: entry.bytes,
            sha256: digest,
        });
    }

    staging.source_hash_manifest = verified_entries;
    staging.state = StagingState::Verified;
    write_staging_state(staging_root, &staging)?;
    Ok(staging)
}

/// Write a quarantine marker (with a human-readable reason) into the
/// staging root. Quarantined content is preserved and never auto-deleted.
fn quarantine(staging_root: &Path, reason: &str) -> Result<(), ContractError> {
    let quarantine_dir = staging_root.join(QUARANTINE_DIR);
    fs::create_dir_all(&quarantine_dir).map_err(|error| {
        contract_error(
            "quarantine_mkdir",
            "quarantine",
            format!("cannot create quarantine directory: {error}"),
        )
    })?;
    let marker_path = quarantine_dir.join("reason.txt");
    let mut file = File::create(&marker_path).map_err(|error| {
        contract_error(
            "quarantine_write",
            "quarantine",
            format!("cannot write quarantine marker: {error}"),
        )
    })?;
    writeln!(file, "{reason}").map_err(|error| {
        contract_error(
            "quarantine_write",
            "quarantine",
            format!("cannot write quarantine reason: {error}"),
        )
    })?;
    Ok(())
}

/// Serialize a staging hash manifest to `hashes.json` (used only for the
/// build-evidence test contract; state.json remains the authority).
pub(crate) fn write_hash_manifest(
    staging_root: &Path,
    entries: &[StagingFileEntry],
) -> Result<(), ContractError> {
    let mut output = String::from("{\"schema\":\"chatgpt_fix.hash_manifest.v1\",\"entries\":[");
    for (i, entry) in entries.iter().enumerate() {
        if i != 0 {
            output.push(',');
        }
        output.push_str("{\"relative_path\":");
        write_string(&mut output, entry.relative_path.as_str());
        output.push_str(",\"bytes\":");
        output.push_str(&entry.bytes.to_string());
        output.push_str(",\"sha256\":");
        write_string(&mut output, entry.sha256.as_str());
        output.push('}');
    }
    output.push_str("]}\n");
    let path = staging_root.join(HASH_MANIFEST_FILE);
    let tmp = staging_root.join(format!("{HASH_MANIFEST_FILE}.tmp"));
    {
        let mut file = File::create(&tmp).map_err(|error| {
            contract_error(
                "hash_manifest_write",
                "hashes.json",
                format!("cannot create hash manifest: {error}"),
            )
        })?;
        file.write_all(output.as_bytes()).map_err(|error| {
            contract_error(
                "hash_manifest_write",
                "hashes.json",
                format!("cannot write hash manifest: {error}"),
            )
        })?;
        file.sync_all().map_err(|error| {
            contract_error(
                "hash_manifest_write",
                "hashes.json",
                format!("cannot sync hash manifest: {error}"),
            )
        })?;
    }
    fs::rename(&tmp, &path).map_err(|error| {
        contract_error(
            "hash_manifest_write",
            "hashes.json",
            format!("cannot finalize hash manifest: {error}"),
        )
    })?;
    Ok(())
}

/// Public read of the staging state file (used by tests).
pub fn read_staging_state(staging_root: &Path) -> Result<StagingV1, ContractError> {
    let bytes = fs::read(staging_root.join(STATE_FILE)).map_err(|error| {
        contract_error(
            "state_unreadable",
            "state.json",
            format!("staging state cannot be read: {error}"),
        )
    })?;
    StagingV1::from_json(&bytes)
}
