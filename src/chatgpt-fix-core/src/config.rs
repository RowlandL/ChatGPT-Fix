use std::fs;
use std::path::Path;

use crate::{
    ConfigApplyMode, ConfigProposalState, ConfigProposalV1, ConfigScope, ContractError,
    sha256_bytes,
};

const CURRENT_CONFIG: &str = "config.json";
const PROPOSED_CONFIG: &str = "proposed.json";

fn contract_error(code: &'static str, field: &str, message: impl Into<String>) -> ContractError {
    ContractError::new(code, field, message)
}

/// Read a config file, returning `None` when it does not exist.
fn read_optional(path: &Path) -> Result<Option<Vec<u8>>, ContractError> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(contract_error(
            "config_unreadable",
            path.to_string_lossy().as_ref(),
            format!("config file cannot be read: {error}"),
        )),
    }
}

/// Dry-run effective-config proposal (`chatgpt_fix.config_proposal.v1`).
///
/// Reads `config.json` (current effective state) and `proposed.json` from the
/// fixture root, computes both digests, and emits a `proposed` receipt. No
/// write happens during planning. The proposal never contains auth/token/cookie
/// content — only digests and paths.
pub fn config_plan(
    fixture_root: &Path,
    scope: ConfigScope,
    backup_path: &str,
) -> Result<ConfigProposalV1, ContractError> {
    let current_bytes = read_optional(&fixture_root.join(CURRENT_CONFIG))?;
    let proposed_bytes = read_optional(&fixture_root.join(PROPOSED_CONFIG))?;

    let current_digest = match &current_bytes {
        Some(bytes) => sha256_bytes(bytes).as_str().to_owned(),
        None => "absent".to_owned(),
    };
    let proposed_digest = match &proposed_bytes {
        Some(bytes) => sha256_bytes(bytes).as_str().to_owned(),
        None => {
            return Err(contract_error(
                "invalid_value",
                "proposed.json",
                "a config proposal requires a proposed.json content file",
            ));
        }
    };

    let proposal = ConfigProposalV1 {
        proposal_id: format!(
            "p7-{}-{}",
            scope.as_json_str(),
            crate::utc_now_rfc3339()
                .replace([':', '-'], "")
                .replace('T', "-")
                .replace('Z', "")
        ),
        scope,
        current_digest,
        proposed_digest,
        backup_path: backup_path.to_owned(),
        apply_mode: ConfigApplyMode::Canary,
        state: ConfigProposalState::Proposed,
        created_at_utc: crate::utc_now_rfc3339(),
    };
    proposal.validate()?;
    Ok(proposal)
}

/// Apply a `proposed` config proposal to an empty canary profile (A6 scope).
///
/// The proposed content is re-read from the fixture and its digest must match
/// the proposal's `proposed_digest` (fail closed). The canary's existing file
/// (if any) is backed up to `<canary_root>/backups/<proposal_id>/` before the
/// proposed content is written to `<canary_root>/<scope>.json`. Auth/token
/// content is never written by this function.
pub fn config_apply(
    fixture_root: &Path,
    canary_root: &Path,
    proposal: &ConfigProposalV1,
) -> Result<ConfigProposalV1, ContractError> {
    if proposal.state != ConfigProposalState::Proposed {
        return Err(contract_error(
            "invalid_state",
            "state",
            "only a 'proposed' proposal may be applied",
        ));
    }
    if proposal.apply_mode != ConfigApplyMode::Canary {
        return Err(contract_error(
            "invalid_value",
            "apply_mode",
            "A6 scope only permits canary applies",
        ));
    }

    let proposed_bytes = fs::read(fixture_root.join(PROPOSED_CONFIG)).map_err(|error| {
        contract_error(
            "proposed_unreadable",
            "proposed.json",
            format!("proposed config cannot be read: {error}"),
        )
    })?;
    let proposed_digest = sha256_bytes(&proposed_bytes);
    if proposed_digest.as_str() != proposal.proposed_digest {
        return Err(contract_error(
            "digest_mismatch",
            "proposed_digest",
            "proposed content digest does not match the proposal (fail closed)",
        ));
    }

    // Backup any existing canary file before overwriting.
    let scope_name = proposal.scope.as_json_str();
    let target = canary_root.join(format!("{scope_name}.json"));
    let backup_dir = canary_root.join("backups").join(&proposal.proposal_id);
    if target.exists() {
        fs::create_dir_all(&backup_dir).map_err(|error| {
            contract_error(
                "backup_write_failed",
                "backup_path",
                format!("cannot create backup directory: {error}"),
            )
        })?;
        let backup_target = backup_dir.join(format!("{scope_name}.json"));
        fs::copy(&target, &backup_target).map_err(|error| {
            contract_error(
                "backup_write_failed",
                "backup_path",
                format!("cannot back up canary config: {error}"),
            )
        })?;
    }

    fs::create_dir_all(canary_root).map_err(|error| {
        contract_error(
            "canary_write_failed",
            "canary_root",
            format!("cannot create canary root: {error}"),
        )
    })?;
    fs::write(&target, &proposed_bytes).map_err(|error| {
        contract_error(
            "canary_write_failed",
            "canary_root",
            format!("cannot write canary config: {error}"),
        )
    })?;

    let mut applied = proposal.clone();
    applied.state = ConfigProposalState::Applied;
    applied.validate()?;
    Ok(applied)
}

/// Roll back an applied proposal from its independent backup.
///
/// The canary root is derived from the proposal's `backup_path`
/// (`<canary_root>/backups/<proposal_id>`). If the backup exists, the original
/// canary file is restored and the proposal becomes `rolled_back`.
pub fn config_rollback(
    canary_root: &Path,
    proposal: &ConfigProposalV1,
) -> Result<ConfigProposalV1, ContractError> {
    if proposal.state != ConfigProposalState::Applied {
        return Err(contract_error(
            "invalid_state",
            "state",
            "only an 'applied' proposal may be rolled back",
        ));
    }

    let scope_name = proposal.scope.as_json_str();
    let target = canary_root.join(format!("{scope_name}.json"));
    let backup_file = canary_root
        .join("backups")
        .join(&proposal.proposal_id)
        .join(format!("{scope_name}.json"));

    if backup_file.exists() {
        fs::copy(&backup_file, &target).map_err(|error| {
            contract_error(
                "rollback_failed",
                "backup_path",
                format!("cannot restore canary config from backup: {error}"),
            )
        })?;
    } else {
        // No prior canary file existed; remove the applied file.
        if target.exists() {
            fs::remove_file(&target).map_err(|error| {
                contract_error(
                    "rollback_failed",
                    "canary_root",
                    format!("cannot remove applied canary config: {error}"),
                )
            })?;
        }
    }

    let mut rolled_back = proposal.clone();
    rolled_back.state = ConfigProposalState::RolledBack;
    rolled_back.validate()?;
    Ok(rolled_back)
}

/// Parse a config proposal JSON document.
pub fn parse_config_proposal(json: &[u8]) -> Result<ConfigProposalV1, ContractError> {
    ConfigProposalV1::from_json(json)
}
