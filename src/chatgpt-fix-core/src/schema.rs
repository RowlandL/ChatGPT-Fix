use std::collections::HashSet;
use std::fmt::Write;

use crate::json::{JsonParser, JsonValue, write_string};
use crate::{ContractError, SafeRelativePath, Sha256Digest};

pub const PLAN_SCHEMA: &str = "chatgpt_fix.plan.v1";
pub const BASELINE_SCHEMA: &str = "chatgpt_fix.baseline.v2";
pub const LAUNCH_SCHEMA: &str = "chatgpt_fix.launch.v1";
pub const RECEIPT_SCHEMA: &str = "chatgpt_fix.receipt.v1";
pub const LIVE_INSPECTION_SCHEMA: &str = "chatgpt_fix.live_inspection.v1";
pub const STAGING_SCHEMA: &str = "chatgpt_fix.staging.v1";
pub const GENERATION_SCHEMA: &str = "chatgpt_fix.generation.v1";
pub const OWNERSHIP_SCHEMA: &str = "chatgpt_fix.ownership.v1";
pub const SHUTDOWN_SCHEMA: &str = "chatgpt_fix.shutdown.v1";
pub const CONFIG_PROPOSAL_SCHEMA: &str = "chatgpt_fix.config_proposal.v1";
pub const MAINTENANCE_PLAN_SCHEMA: &str = "chatgpt_fix.maintenance_plan.v1";
pub const NTC_MANIFEST_SCHEMA: &str = "chatgpt_fix.ntc_manifest.v1";

/// The ownership observation state machine (report-only).
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum OwnershipState {
    Observing,
    Observed,
    Reconciled,
}

impl OwnershipState {
    fn as_json_str(self) -> &'static str {
        match self {
            Self::Observing => "observing",
            Self::Observed => "observed",
            Self::Reconciled => "reconciled",
        }
    }

    fn from_json_str(s: &str) -> Result<Self, ContractError> {
        match s {
            "observing" => Ok(Self::Observing),
            "observed" => Ok(Self::Observed),
            "reconciled" => Ok(Self::Reconciled),
            other => Err(ContractError::new(
                "invalid_value",
                "state",
                format!("expected 'observing', 'observed', or 'reconciled', got '{other}'"),
            )),
        }
    }
}

/// The generation state machine for the pointer/shortcut transaction.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum GenerationState {
    Prepared,
    Activated,
    RolledBack,
    SmokeStarted,
    SmokeObserved,
}

impl GenerationState {
    fn as_json_str(self) -> &'static str {
        match self {
            Self::Prepared => "prepared",
            Self::Activated => "activated",
            Self::RolledBack => "rolled_back",
            Self::SmokeStarted => "smoke_started",
            Self::SmokeObserved => "smoke_observed",
        }
    }

    fn from_json_str(s: &str) -> Result<Self, ContractError> {
        match s {
            "prepared" => Ok(Self::Prepared),
            "activated" => Ok(Self::Activated),
            "rolled_back" => Ok(Self::RolledBack),
            "smoke_started" => Ok(Self::SmokeStarted),
            "smoke_observed" => Ok(Self::SmokeObserved),
            other => Err(ContractError::new(
                "invalid_value",
                "state",
                format!(
                    "expected 'prepared', 'activated', 'rolled_back', 'smoke_started', or 'smoke_observed', got '{other}'"
                ),
            )),
        }
    }
}

/// The staging state machine: staged -> verified, or quarantined on failure.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum StagingState {
    Staged,
    Verified,
    Quarantined,
}

impl StagingState {
    fn as_json_str(self) -> &'static str {
        match self {
            Self::Staged => "staged",
            Self::Verified => "verified",
            Self::Quarantined => "quarantined",
        }
    }

    fn from_json_str(s: &str) -> Result<Self, ContractError> {
        match s {
            "staged" => Ok(Self::Staged),
            "verified" => Ok(Self::Verified),
            "quarantined" => Ok(Self::Quarantined),
            other => Err(ContractError::new(
                "invalid_value",
                "state",
                format!("expected 'staged', 'verified', or 'quarantined', got '{other}'"),
            )),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum PlanDecision {
    Ready,
    Rejected,
}

impl PlanDecision {
    fn as_json_str(self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::Rejected => "rejected",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum PlanActionKind {
    WouldCopy,
    WouldWrite,
    WouldSwitch,
    WouldStart,
    WouldTerminate,
}

impl PlanActionKind {
    fn as_json_str(self) -> &'static str {
        match self {
            Self::WouldCopy => "would-copy",
            Self::WouldWrite => "would-write",
            Self::WouldSwitch => "would-switch",
            Self::WouldStart => "would-start",
            Self::WouldTerminate => "would-terminate",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlanAction {
    pub kind: PlanActionKind,
    pub target: SafeRelativePath,
    pub execute: bool,
}

impl PlanAction {
    pub fn validate(&self) -> Result<(), ContractError> {
        if self.execute {
            return Err(ContractError::new(
                "invariant_violation",
                "execute",
                "plan actions must not execute",
            ));
        }
        Ok(())
    }

    pub fn to_json(&self) -> String {
        let mut output = String::new();
        self.write_json(&mut output);
        output
    }

    fn write_json(&self, output: &mut String) {
        output.push_str("{\"kind\":");
        write_string(output, self.kind.as_json_str());
        output.push_str(",\"target\":");
        write_string(output, self.target.as_str());
        output.push_str(",\"execute\":");
        output.push_str(if self.execute { "true" } else { "false" });
        output.push('}');
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BaselineV2 {
    pub baseline_id: String,
    pub package_full_name: String,
    pub version: String,
    pub architecture: String,
    pub publisher: String,
    pub source: SafeRelativePath,
    pub bytes: u64,
    pub sha256: Sha256Digest,
}

impl BaselineV2 {
    pub fn validate(&self) -> Result<(), ContractError> {
        validate_text("baseline_id", &self.baseline_id)?;
        validate_text("package_full_name", &self.package_full_name)?;
        validate_text("version", &self.version)?;
        validate_text("architecture", &self.architecture)?;
        validate_text("publisher", &self.publisher)?;
        if self.bytes == 0 {
            return Err(ContractError::new(
                "invalid_value",
                "bytes",
                "must be greater than zero",
            ));
        }
        Ok(())
    }

    pub fn to_json(&self) -> Result<String, ContractError> {
        let mut output = String::new();
        self.write_json(&mut output);
        Ok(output)
    }

    fn write_json(&self, output: &mut String) {
        output.push_str("{\"schema\":");
        write_string(output, BASELINE_SCHEMA);
        output.push_str(",\"baseline_id\":");
        write_string(output, &self.baseline_id);
        output.push_str(",\"package_full_name\":");
        write_string(output, &self.package_full_name);
        output.push_str(",\"version\":");
        write_string(output, &self.version);
        output.push_str(",\"architecture\":");
        write_string(output, &self.architecture);
        output.push_str(",\"publisher\":");
        write_string(output, &self.publisher);
        output.push_str(",\"source\":");
        write_string(output, self.source.as_str());
        write!(output, ",\"bytes\":{}", self.bytes).expect("writing JSON to a String cannot fail");
        output.push_str(",\"sha256\":");
        write_string(output, self.sha256.as_str());
        output.push('}');
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlanV1 {
    pub fixture_id: String,
    pub decision: PlanDecision,
    pub baseline: Option<BaselineV2>,
    pub actions: Vec<PlanAction>,
    pub errors: Vec<String>,
}

impl PlanV1 {
    pub fn validate(&self) -> Result<(), ContractError> {
        validate_text("fixture_id", &self.fixture_id)?;

        match self.decision {
            PlanDecision::Ready => {
                let baseline = self.baseline.as_ref().ok_or_else(|| {
                    ContractError::new(
                        "invariant_violation",
                        "baseline",
                        "ready plans require a baseline",
                    )
                })?;
                baseline.validate()?;
                if self.actions.is_empty() {
                    return Err(ContractError::new(
                        "invariant_violation",
                        "actions",
                        "ready plans require at least one action",
                    ));
                }
                if !self.errors.is_empty() {
                    return Err(ContractError::new(
                        "invariant_violation",
                        "errors",
                        "ready plans must not contain errors",
                    ));
                }
                validate_unique_action_kinds(&self.actions)?;
            }
            PlanDecision::Rejected => {
                if self.baseline.is_some() {
                    return Err(ContractError::new(
                        "invariant_violation",
                        "baseline",
                        "rejected plans must not contain a baseline",
                    ));
                }
                if !self.actions.is_empty() {
                    return Err(ContractError::new(
                        "invariant_violation",
                        "actions",
                        "rejected plans must not contain actions",
                    ));
                }
                if self.errors.is_empty() {
                    return Err(ContractError::new(
                        "invariant_violation",
                        "errors",
                        "rejected plans require at least one error",
                    ));
                }
                for error in &self.errors {
                    validate_text("errors", error)?;
                }
            }
        }

        Ok(())
    }

    pub fn to_json(&self) -> Result<String, ContractError> {
        let mut output = String::new();
        output.push_str("{\"schema\":");
        write_string(&mut output, PLAN_SCHEMA);
        output.push_str(",\"fixture_id\":");
        write_string(&mut output, &self.fixture_id);
        output.push_str(",\"decision\":");
        write_string(&mut output, self.decision.as_json_str());
        output.push_str(",\"baseline\":");
        match &self.baseline {
            Some(baseline) => baseline.write_json(&mut output),
            None => output.push_str("null"),
        }
        output.push_str(",\"actions\":[");
        for (index, action) in self.actions.iter().enumerate() {
            if index != 0 {
                output.push(',');
            }
            action.write_json(&mut output);
        }
        output.push_str("],\"errors\":[");
        for (index, error) in self.errors.iter().enumerate() {
            if index != 0 {
                output.push(',');
            }
            write_string(&mut output, error);
        }
        output.push_str("]}");
        Ok(output)
    }
}

fn validate_unique_action_kinds(actions: &[PlanAction]) -> Result<(), ContractError> {
    let mut kinds = HashSet::with_capacity(actions.len());
    for action in actions {
        action.validate()?;
        if !kinds.insert(action.kind) {
            return Err(ContractError::new(
                "duplicate_action_kind",
                "actions",
                "action kinds must be unique",
            ));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// LiveInspectionV1 — chatgpt_fix.live_inspection.v1
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LiveInspectionV1 {
    pub probe_source: String,
    pub package_full_name: String,
    pub version: String,
    pub architecture: String,
    pub publisher: String,
    pub publisher_id: String,
    pub install_location: String,
    pub manifest_sha256: Sha256Digest,
    pub primary_executable: String,
    pub primary_bytes: u64,
    pub primary_sha256: Sha256Digest,
    pub manifest_dependencies: Vec<String>,
    pub shortcut_target_path: String,
    pub shortcut_arguments: String,
    pub shortcut_working_directory: String,
    pub shortcut_icon_location: String,
    pub identity_before: String,
    pub identity_after: String,
    pub hash_before: Option<Sha256Digest>,
    pub hash_after: Option<Sha256Digest>,
    pub codex_home_inspected: bool,
    pub local_state_inspected: bool,
    pub processes_inspected: bool,
}

impl LiveInspectionV1 {
    pub fn validate(&self) -> Result<(), ContractError> {
        validate_text("probe_source", &self.probe_source)?;
        validate_text("package_full_name", &self.package_full_name)?;
        validate_text("version", &self.version)?;
        validate_text("architecture", &self.architecture)?;
        validate_text("publisher", &self.publisher)?;
        validate_text("publisher_id", &self.publisher_id)?;
        validate_text("install_location", &self.install_location)?;
        validate_text("primary_executable", &self.primary_executable)?;
        validate_text("identity_before", &self.identity_before)?;
        validate_text("identity_after", &self.identity_after)?;

        if self.primary_bytes == 0 {
            return Err(ContractError::new(
                "invalid_value",
                "primary_bytes",
                "must be greater than zero",
            ));
        }

        if self.codex_home_inspected {
            return Err(ContractError::new(
                "invariant_violation",
                "codex_home_inspected",
                "P2 does not inspect codex home",
            ));
        }
        if self.local_state_inspected {
            return Err(ContractError::new(
                "invariant_violation",
                "local_state_inspected",
                "P2 does not inspect local state",
            ));
        }
        if self.processes_inspected {
            return Err(ContractError::new(
                "invariant_violation",
                "processes_inspected",
                "P2 does not inspect processes",
            ));
        }

        Ok(())
    }

    pub fn to_json(&self) -> Result<String, ContractError> {
        let mut output = String::new();
        output.push_str("{\"schema\":");
        write_string(&mut output, LIVE_INSPECTION_SCHEMA);

        output.push_str(",\"probe_source\":");
        write_string(&mut output, &self.probe_source);
        output.push_str(",\"package_full_name\":");
        write_string(&mut output, &self.package_full_name);
        output.push_str(",\"version\":");
        write_string(&mut output, &self.version);
        output.push_str(",\"architecture\":");
        write_string(&mut output, &self.architecture);
        output.push_str(",\"publisher\":");
        write_string(&mut output, &self.publisher);
        output.push_str(",\"publisher_id\":");
        write_string(&mut output, &self.publisher_id);
        output.push_str(",\"install_location\":");
        write_string(&mut output, &self.install_location);
        output.push_str(",\"manifest_sha256\":");
        write_string(&mut output, self.manifest_sha256.as_str());
        output.push_str(",\"primary_executable\":");
        write_string(&mut output, &self.primary_executable);
        write!(&mut output, ",\"primary_bytes\":{}", self.primary_bytes)
            .expect("writing JSON to a String cannot fail");
        output.push_str(",\"primary_sha256\":");
        write_string(&mut output, self.primary_sha256.as_str());

        output.push_str(",\"manifest_dependencies\":[");
        for (i, dep) in self.manifest_dependencies.iter().enumerate() {
            if i != 0 {
                output.push(',');
            }
            write_string(&mut output, dep);
        }
        output.push(']');

        output.push_str(",\"shortcut_target_path\":");
        write_string(&mut output, &self.shortcut_target_path);
        output.push_str(",\"shortcut_arguments\":");
        write_string(&mut output, &self.shortcut_arguments);
        output.push_str(",\"shortcut_working_directory\":");
        write_string(&mut output, &self.shortcut_working_directory);
        output.push_str(",\"shortcut_icon_location\":");
        write_string(&mut output, &self.shortcut_icon_location);

        output.push_str(",\"identity_before\":");
        write_string(&mut output, &self.identity_before);
        output.push_str(",\"identity_after\":");
        write_string(&mut output, &self.identity_after);

        output.push_str(",\"hash_before\":");
        match &self.hash_before {
            Some(h) => write_string(&mut output, h.as_str()),
            None => output.push_str("null"),
        }
        output.push_str(",\"hash_after\":");
        match &self.hash_after {
            Some(h) => write_string(&mut output, h.as_str()),
            None => output.push_str("null"),
        }

        write!(
            &mut output,
            ",\"codex_home_inspected\":{},\"local_state_inspected\":{},\"processes_inspected\":{}}}",
            if self.codex_home_inspected { "true" } else { "false" },
            if self.local_state_inspected { "true" } else { "false" },
            if self.processes_inspected { "true" } else { "false" },
        )
        .expect("writing JSON to a String cannot fail");

        Ok(output)
    }

    pub fn from_json(json: &[u8]) -> Result<Self, ContractError> {
        let parsed = JsonParser::new(json)?.parse_top_level()?;
        let _obj = parsed.as_object()?;

        let schema = parsed.field("schema")?.as_str()?;
        if schema != LIVE_INSPECTION_SCHEMA {
            return Err(ContractError::new(
                "schema_mismatch",
                "schema",
                format!("expected {}, got {}", LIVE_INSPECTION_SCHEMA, schema),
            ));
        }

        let make_err = |field: &str, msg: &str| -> ContractError {
            ContractError::new("json_value", field, msg)
        };

        let get_str = |key: &str| -> Result<String, ContractError> {
            Ok(parsed.field(key)?.as_str()?.to_owned())
        };
        let get_digest = |key: &str| -> Result<Sha256Digest, ContractError> {
            let s = get_str(key)?;
            Sha256Digest::parse(&s).map_err(|_| make_err(key, "invalid SHA-256"))
        };
        let get_u64 = |key: &str| -> Result<u64, ContractError> {
            let n = parsed.field(key)?.as_i64()?;
            if n < 0 {
                return Err(make_err(key, "must be non-negative"));
            }
            Ok(n as u64)
        };
        let get_bool = |key: &str| -> Result<bool, ContractError> { parsed.field(key)?.as_bool() };
        let get_vec_str = |key: &str| -> Result<Vec<String>, ContractError> {
            let arr = parsed.field(key)?.as_array()?;
            arr.iter().map(|v| Ok(v.as_str()?.to_owned())).collect()
        };
        let get_opt_digest = |key: &str| -> Result<Option<Sha256Digest>, ContractError> {
            let v = parsed.field(key)?;
            match v {
                JsonValue::Null => Ok(None),
                _ => {
                    let s = v.as_str()?;
                    Ok(Some(
                        Sha256Digest::parse(s).map_err(|_| make_err(key, "invalid SHA-256"))?,
                    ))
                }
            }
        };

        Ok(Self {
            probe_source: get_str("probe_source")?,
            package_full_name: get_str("package_full_name")?,
            version: get_str("version")?,
            architecture: get_str("architecture")?,
            publisher: get_str("publisher")?,
            publisher_id: get_str("publisher_id")?,
            install_location: get_str("install_location")?,
            manifest_sha256: get_digest("manifest_sha256")?,
            primary_executable: get_str("primary_executable")?,
            primary_bytes: get_u64("primary_bytes")?,
            primary_sha256: get_digest("primary_sha256")?,
            manifest_dependencies: get_vec_str("manifest_dependencies")?,
            shortcut_target_path: get_str("shortcut_target_path")?,
            shortcut_arguments: get_str("shortcut_arguments")?,
            shortcut_working_directory: get_str("shortcut_working_directory")?,
            shortcut_icon_location: get_str("shortcut_icon_location")?,
            identity_before: get_str("identity_before")?,
            identity_after: get_str("identity_after")?,
            hash_before: get_opt_digest("hash_before")?,
            hash_after: get_opt_digest("hash_after")?,
            codex_home_inspected: get_bool("codex_home_inspected")?,
            local_state_inspected: get_bool("local_state_inspected")?,
            processes_inspected: get_bool("processes_inspected")?,
        })
    }
}

// ---------------------------------------------------------------------------
// PlanV1::from_json — parse from canonical JSON
// ---------------------------------------------------------------------------

impl PlanV1 {
    /// Parse a PlanV1 from canonical JSON bytes.
    ///
    /// The parse -> validate -> canonical serialize pipeline must be
    /// byte-stable: re-serializing the parsed plan must produce the
    /// exact same bytes.
    pub fn from_json(json: &[u8]) -> Result<Self, ContractError> {
        let parsed = JsonParser::new(json)?.parse_top_level()?;
        let _obj = parsed.as_object()?;

        let schema = parsed.field("schema")?.as_str()?;
        if schema != PLAN_SCHEMA {
            return Err(ContractError::new(
                "schema_mismatch",
                "schema",
                format!("expected {}, got {}", PLAN_SCHEMA, schema),
            ));
        }

        let fixture_id = parsed.field("fixture_id")?.as_str()?.to_owned();

        let decision_str = parsed.field("decision")?.as_str()?;
        let decision = match decision_str {
            "ready" => PlanDecision::Ready,
            "rejected" => PlanDecision::Rejected,
            other => {
                return Err(ContractError::new(
                    "invalid_value",
                    "decision",
                    format!("expected 'ready' or 'rejected', got '{}'", other),
                ));
            }
        };

        let baseline = match parsed.field("baseline")? {
            JsonValue::Null => None,
            _ => {
                let b = parse_baseline_from_json(&parsed, "baseline")?;
                Some(b)
            }
        };

        let actions_arr = parsed.field("actions")?.as_array()?;
        let mut actions = Vec::with_capacity(actions_arr.len());
        for action_val in actions_arr {
            let _action_obj = action_val.as_object()?;
            let kind_str = action_val.field("kind")?.as_str()?;
            let kind = match kind_str {
                "would-copy" => PlanActionKind::WouldCopy,
                "would-write" => PlanActionKind::WouldWrite,
                "would-switch" => PlanActionKind::WouldSwitch,
                "would-start" => PlanActionKind::WouldStart,
                "would-terminate" => PlanActionKind::WouldTerminate,
                other => {
                    return Err(ContractError::new(
                        "invalid_value",
                        "kind",
                        format!("unknown action kind: '{}'", other),
                    ));
                }
            };
            let target =
                SafeRelativePath::parse(action_val.field("target")?.as_str()?).map_err(|e| {
                    ContractError::new(
                        "invalid_value",
                        "target",
                        format!("invalid action target: {}", e.message),
                    )
                })?;
            let execute = action_val.field("execute")?.as_bool()?;
            actions.push(PlanAction {
                kind,
                target,
                execute,
            });
        }

        let errors_arr = parsed.field("errors")?.as_array()?;
        let errors: Vec<String> = errors_arr
            .iter()
            .map(|v| Ok(v.as_str()?.to_owned()))
            .collect::<Result<Vec<_>, ContractError>>()?;

        let plan = PlanV1 {
            fixture_id,
            decision,
            baseline,
            actions,
            errors,
        };

        plan.validate()?;

        Ok(plan)
    }
}

fn parse_baseline_from_json(parent: &JsonValue, field: &str) -> Result<BaselineV2, ContractError> {
    let obj = parent.field(field)?;
    let schema = obj.field("schema")?.as_str()?;
    if schema != BASELINE_SCHEMA {
        return Err(ContractError::new(
            "schema_mismatch",
            "schema",
            format!("expected {}, got {}", BASELINE_SCHEMA, schema),
        ));
    }

    Ok(BaselineV2 {
        baseline_id: obj.field("baseline_id")?.as_str()?.to_owned(),
        package_full_name: obj.field("package_full_name")?.as_str()?.to_owned(),
        version: obj.field("version")?.as_str()?.to_owned(),
        architecture: obj.field("architecture")?.as_str()?.to_owned(),
        publisher: obj.field("publisher")?.as_str()?.to_owned(),
        source: SafeRelativePath::parse(obj.field("source")?.as_str()?).map_err(|e| {
            ContractError::new(
                "invalid_value",
                "source",
                format!("invalid source path: {}", e.message),
            )
        })?,
        bytes: {
            let n = obj.field("bytes")?.as_i64()?;
            if n < 0 {
                return Err(ContractError::new(
                    "invalid_value",
                    "bytes",
                    "must be non-negative",
                ));
            }
            n as u64
        },
        sha256: Sha256Digest::parse(obj.field("sha256")?.as_str()?)
            .map_err(|_| ContractError::new("invalid_value", "sha256", "invalid SHA-256"))?,
    })
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LaunchV1 {
    pub launch_id: String,
    pub generation: u64,
    pub baseline_id: SafeRelativePath,
    pub executable: SafeRelativePath,
    pub would_start: bool,
    pub reason: Option<String>,
}

impl LaunchV1 {
    pub fn validate(&self) -> Result<(), ContractError> {
        validate_text("launch_id", &self.launch_id)?;
        if self.generation == 0 {
            return Err(ContractError::new(
                "invalid_value",
                "generation",
                "must be greater than zero",
            ));
        }

        match (self.would_start, &self.reason) {
            (true, None) => Ok(()),
            (true, Some(_)) => Err(ContractError::new(
                "invariant_violation",
                "reason",
                "must be absent when would_start is true",
            )),
            (false, None) => Err(ContractError::new(
                "invariant_violation",
                "reason",
                "must be present when would_start is false",
            )),
            (false, Some(reason)) => validate_text("reason", reason),
        }
    }

    pub fn to_json(&self) -> Result<String, ContractError> {
        let mut output = String::new();
        output.push_str("{\"schema\":");
        write_string(&mut output, LAUNCH_SCHEMA);
        output.push_str(",\"launch_id\":");
        write_string(&mut output, &self.launch_id);
        write!(output, ",\"generation\":{}", self.generation)
            .expect("writing JSON to a String cannot fail");
        output.push_str(",\"baseline_id\":");
        write_string(&mut output, self.baseline_id.as_str());
        output.push_str(",\"executable\":");
        write_string(&mut output, self.executable.as_str());
        output.push_str(",\"would_start\":");
        output.push_str(if self.would_start { "true" } else { "false" });
        output.push_str(",\"reason\":");
        match &self.reason {
            Some(reason) => write_string(&mut output, reason),
            None => output.push_str("null"),
        }
        output.push('}');
        Ok(output)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReceiptV1 {
    pub operation: String,
    pub status: String,
    pub plan_sha256: Sha256Digest,
    pub artifact: Option<SafeRelativePath>,
    pub artifact_sha256: Option<Sha256Digest>,
    pub source_commit: String,
    pub toolchain: String,
    pub signing_status: String,
}

impl ReceiptV1 {
    pub fn validate(&self) -> Result<(), ContractError> {
        validate_text("operation", &self.operation)?;
        validate_text("status", &self.status)?;
        validate_text("source_commit", &self.source_commit)?;
        validate_text("toolchain", &self.toolchain)?;
        validate_text("signing_status", &self.signing_status)?;

        if self.artifact.is_some() != self.artifact_sha256.is_some() {
            return Err(ContractError::new(
                "invariant_violation",
                "artifact",
                "artifact and artifact_sha256 must both be present or both be absent",
            ));
        }
        Ok(())
    }

    pub fn to_json(&self) -> Result<String, ContractError> {
        let mut output = String::new();
        output.push_str("{\"schema\":");
        write_string(&mut output, RECEIPT_SCHEMA);
        output.push_str(",\"operation\":");
        write_string(&mut output, &self.operation);
        output.push_str(",\"status\":");
        write_string(&mut output, &self.status);
        output.push_str(",\"plan_sha256\":");
        write_string(&mut output, self.plan_sha256.as_str());
        output.push_str(",\"artifact\":");
        match &self.artifact {
            Some(artifact) => write_string(&mut output, artifact.as_str()),
            None => output.push_str("null"),
        }
        output.push_str(",\"artifact_sha256\":");
        match &self.artifact_sha256 {
            Some(sha256) => write_string(&mut output, sha256.as_str()),
            None => output.push_str("null"),
        }
        output.push_str(",\"source_commit\":");
        write_string(&mut output, &self.source_commit);
        output.push_str(",\"toolchain\":");
        write_string(&mut output, &self.toolchain);
        output.push_str(",\"signing_status\":");
        write_string(&mut output, &self.signing_status);
        output.push('}');
        Ok(output)
    }
}

fn validate_text(field: &'static str, value: &str) -> Result<(), ContractError> {
    if value.is_empty() {
        return Err(ContractError::new(
            "empty_field",
            field,
            "must not be empty",
        ));
    }
    if value.chars().any(char::is_control) {
        return Err(ContractError::new(
            "control_character",
            field,
            "must not contain control characters",
        ));
    }
    Ok(())
}

/// A single staged file entry inside a `StagingV1` hash manifest.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StagingFileEntry {
    pub relative_path: SafeRelativePath,
    pub bytes: u64,
    pub sha256: Sha256Digest,
}

/// The immutable-baseline staging receipt (`chatgpt_fix.staging.v1`).
///
/// Produced by `ChatGPT-Fix-Packer stage --probe-json <path> --out <root>`
/// and consumed by `verify --staging <root>`. A staging that fails
/// validation is marked `quarantined` and is never promoted to a baseline.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StagingV1 {
    pub source_package_full_name: String,
    pub source_version: String,
    pub source_hash_manifest: Vec<StagingFileEntry>,
    pub staging_root: String,
    pub baseline_root: String,
    pub files_staged: u64,
    pub total_bytes: u64,
    pub state: StagingState,
    pub created_at_utc: String,
}

impl StagingV1 {
    pub fn validate(&self) -> Result<(), ContractError> {
        validate_text("source_package_full_name", &self.source_package_full_name)?;
        validate_text("source_version", &self.source_version)?;
        validate_text("staging_root", &self.staging_root)?;
        validate_text("baseline_root", &self.baseline_root)?;
        validate_text("created_at_utc", &self.created_at_utc)?;

        if self.source_hash_manifest.is_empty() {
            return Err(ContractError::new(
                "invalid_value",
                "source_hash_manifest",
                "must not be empty",
            ));
        }
        if self.files_staged != self.source_hash_manifest.len() as u64 {
            return Err(ContractError::new(
                "invariant_violation",
                "files_staged",
                "must equal source_hash_manifest length",
            ));
        }

        let mut total: u64 = 0;
        for entry in &self.source_hash_manifest {
            total = total
                .checked_add(entry.bytes)
                .ok_or_else(|| ContractError::new("overflow", "bytes", "total bytes overflow"))?;
        }
        if self.total_bytes != total {
            return Err(ContractError::new(
                "invariant_violation",
                "total_bytes",
                "must equal the sum of manifest entry bytes",
            ));
        }

        Ok(())
    }

    pub fn to_json(&self) -> Result<String, ContractError> {
        let mut output = String::new();
        output.push_str("{\"schema\":");
        write_string(&mut output, STAGING_SCHEMA);
        output.push_str(",\"source_package_full_name\":");
        write_string(&mut output, &self.source_package_full_name);
        output.push_str(",\"source_version\":");
        write_string(&mut output, &self.source_version);
        output.push_str(",\"source_hash_manifest\":[");
        for (i, entry) in self.source_hash_manifest.iter().enumerate() {
            if i != 0 {
                output.push(',');
            }
            output.push_str("{\"relative_path\":");
            write_string(&mut output, entry.relative_path.as_str());
            write!(&mut output, ",\"bytes\":{}", entry.bytes)
                .expect("writing JSON to a String cannot fail");
            output.push_str(",\"sha256\":");
            write_string(&mut output, entry.sha256.as_str());
            output.push('}');
        }
        output.push(']');
        output.push_str(",\"staging_root\":");
        write_string(&mut output, &self.staging_root);
        output.push_str(",\"baseline_root\":");
        write_string(&mut output, &self.baseline_root);
        write!(&mut output, ",\"files_staged\":{}", self.files_staged)
            .expect("writing JSON to a String cannot fail");
        write!(&mut output, ",\"total_bytes\":{}", self.total_bytes)
            .expect("writing JSON to a String cannot fail");
        output.push_str(",\"state\":");
        write_string(&mut output, self.state.as_json_str());
        output.push_str(",\"created_at_utc\":");
        write_string(&mut output, &self.created_at_utc);
        output.push('}');
        Ok(output)
    }

    pub fn from_json(json: &[u8]) -> Result<Self, ContractError> {
        let parsed = JsonParser::new(json)?.parse_top_level()?;
        let _obj = parsed.as_object()?;

        let schema = parsed.field("schema")?.as_str()?;
        if schema != STAGING_SCHEMA {
            return Err(ContractError::new(
                "schema_mismatch",
                "schema",
                format!("expected {}, got {}", STAGING_SCHEMA, schema),
            ));
        }

        let make_err = |field: &str, msg: &str| -> ContractError {
            ContractError::new("json_value", field, msg)
        };

        let get_str = |key: &str| -> Result<String, ContractError> {
            Ok(parsed.field(key)?.as_str()?.to_owned())
        };
        let get_u64 = |key: &str| -> Result<u64, ContractError> {
            let n = parsed.field(key)?.as_i64()?;
            if n < 0 {
                return Err(make_err(key, "must be non-negative"));
            }
            Ok(n as u64)
        };

        let source_package_full_name = get_str("source_package_full_name")?;
        let source_version = get_str("source_version")?;
        let staging_root = get_str("staging_root")?;
        let baseline_root = get_str("baseline_root")?;
        let files_staged = get_u64("files_staged")?;
        let total_bytes = get_u64("total_bytes")?;
        let state = StagingState::from_json_str(get_str("state")?.as_str())?;
        let created_at_utc = get_str("created_at_utc")?;

        let manifest_value = parsed.field("source_hash_manifest")?;
        let entries = manifest_value.as_array()?;
        let mut source_hash_manifest = Vec::with_capacity(entries.len());
        for entry_value in entries {
            let relative_path =
                SafeRelativePath::parse(entry_value.field("relative_path")?.as_str()?)?;
            let bytes = entry_value.field("bytes")?.as_i64()?;
            if bytes < 0 {
                return Err(make_err("bytes", "must be non-negative"));
            }
            let sha256 = Sha256Digest::parse(entry_value.field("sha256")?.as_str()?)
                .map_err(|_| make_err("sha256", "invalid SHA-256"))?;
            source_hash_manifest.push(StagingFileEntry {
                relative_path,
                bytes: bytes as u64,
                sha256,
            });
        }

        let staging = StagingV1 {
            source_package_full_name,
            source_version,
            source_hash_manifest,
            staging_root,
            baseline_root,
            files_staged,
            total_bytes,
            state,
            created_at_utc,
        };
        staging.validate()?;
        Ok(staging)
    }
}

/// A snapshot of the ChatGPT.lnk COM properties that P4 backs up and
/// restores exactly on rollback.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShortcutBackup {
    pub target_path: String,
    pub arguments: String,
    pub working_directory: String,
    pub icon_location: String,
}

impl ShortcutBackup {
    pub fn write_json(&self, output: &mut String) {
        output.push_str("{\"target_path\":");
        write_string(output, &self.target_path);
        output.push_str(",\"arguments\":");
        write_string(output, &self.arguments);
        output.push_str(",\"working_directory\":");
        write_string(output, &self.working_directory);
        output.push_str(",\"icon_location\":");
        write_string(output, &self.icon_location);
        output.push('}');
    }
}

/// The generation transaction receipt (`chatgpt_fix.generation.v1`).
///
/// Produced by `ChatGPT-Fix-Manager activate --baseline <root> [--smoke]`
/// and consumed by `rollback`. The pointer switch and shortcut backup are
/// atomic and fully reversible; a smoke run only observes, never kills.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GenerationV1 {
    pub generation_id: String,
    pub baseline_root: String,
    pub pointer_path: String,
    pub shortcut_backup: ShortcutBackup,
    pub launch_ledger: String,
    pub state: GenerationState,
    pub created_at_utc: String,
}

impl GenerationV1 {
    pub fn validate(&self) -> Result<(), ContractError> {
        validate_text("generation_id", &self.generation_id)?;
        validate_text("baseline_root", &self.baseline_root)?;
        validate_text("pointer_path", &self.pointer_path)?;
        validate_text("launch_ledger", &self.launch_ledger)?;
        validate_text("created_at_utc", &self.created_at_utc)?;
        if self.shortcut_backup.target_path.is_empty() {
            return Err(ContractError::new(
                "empty_field",
                "shortcut_backup.target_path",
                "must not be empty",
            ));
        }
        Ok(())
    }

    pub fn to_json(&self) -> Result<String, ContractError> {
        let mut output = String::new();
        output.push_str("{\"schema\":");
        write_string(&mut output, GENERATION_SCHEMA);
        output.push_str(",\"generation_id\":");
        write_string(&mut output, &self.generation_id);
        output.push_str(",\"baseline_root\":");
        write_string(&mut output, &self.baseline_root);
        output.push_str(",\"pointer_path\":");
        write_string(&mut output, &self.pointer_path);
        output.push_str(",\"shortcut_backup\":");
        self.shortcut_backup.write_json(&mut output);
        output.push_str(",\"launch_ledger\":");
        write_string(&mut output, &self.launch_ledger);
        output.push_str(",\"state\":");
        write_string(&mut output, self.state.as_json_str());
        output.push_str(",\"created_at_utc\":");
        write_string(&mut output, &self.created_at_utc);
        output.push('}');
        Ok(output)
    }

    pub fn from_json(json: &[u8]) -> Result<Self, ContractError> {
        let parsed = JsonParser::new(json)?.parse_top_level()?;
        let _obj = parsed.as_object()?;

        let schema = parsed.field("schema")?.as_str()?;
        if schema != GENERATION_SCHEMA {
            return Err(ContractError::new(
                "schema_mismatch",
                "schema",
                format!("expected {}, got {}", GENERATION_SCHEMA, schema),
            ));
        }

        let get_str = |key: &str| -> Result<String, ContractError> {
            Ok(parsed.field(key)?.as_str()?.to_owned())
        };

        let generation_id = get_str("generation_id")?;
        let baseline_root = get_str("baseline_root")?;
        let pointer_path = get_str("pointer_path")?;
        let launch_ledger = get_str("launch_ledger")?;
        let state = GenerationState::from_json_str(get_str("state")?.as_str())?;
        let created_at_utc = get_str("created_at_utc")?;

        let shortcut_value = parsed.field("shortcut_backup")?;
        let target_path = shortcut_value.field("target_path")?.as_str()?.to_owned();
        let arguments = shortcut_value.field("arguments")?.as_str()?.to_owned();
        let working_directory = shortcut_value
            .field("working_directory")?
            .as_str()?
            .to_owned();
        let icon_location = shortcut_value.field("icon_location")?.as_str()?.to_owned();
        let shortcut_backup = ShortcutBackup {
            target_path,
            arguments,
            working_directory,
            icon_location,
        };

        let generation = GenerationV1 {
            generation_id,
            baseline_root,
            pointer_path,
            shortcut_backup,
            launch_ledger,
            state,
            created_at_utc,
        };
        generation.validate().map_err(|error| {
            ContractError::new(
                "generation_validation_failed",
                "generation",
                format!("generation validation failed: {error}"),
            )
        })?;
        Ok(generation)
    }
}

/// A single Job Object membership record observed for one process.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JobMemberEntry {
    pub pid: u64,
    pub name: String,
    pub in_job: bool,
    pub breakaway: bool,
}

/// A breakaway event (a process leaving the Job Object).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BreakawayEvent {
    pub pid: u64,
    pub name: String,
    pub event: String,
}

/// The ownership observation receipt (`chatgpt_fix.ownership.v1`).
///
/// Produced by `ChatGPT-Fix-Launcher observe --fixture-root <path>`.
/// Observation is report-only: no Job close, no graceful shutdown, no
/// terminate. `exit_report` and ledger reconciliation describe differences
/// without acting on them.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OwnershipV1 {
    pub launch_id: String,
    pub root_pid: u64,
    pub job_members: Vec<JobMemberEntry>,
    pub breakaway_events: Vec<BreakawayEvent>,
    pub exit_report: String,
    pub state: OwnershipState,
    pub created_at_utc: String,
}

impl OwnershipV1 {
    pub fn validate(&self) -> Result<(), ContractError> {
        validate_text("launch_id", &self.launch_id)?;
        validate_text("exit_report", &self.exit_report)?;
        validate_text("created_at_utc", &self.created_at_utc)?;
        if self.root_pid == 0 {
            return Err(ContractError::new(
                "invalid_value",
                "root_pid",
                "must be greater than zero",
            ));
        }
        Ok(())
    }

    pub fn to_json(&self) -> Result<String, ContractError> {
        let mut output = String::new();
        output.push_str("{\"schema\":");
        write_string(&mut output, OWNERSHIP_SCHEMA);
        output.push_str(",\"launch_id\":");
        write_string(&mut output, &self.launch_id);
        write!(&mut output, ",\"root_pid\":{}", self.root_pid)
            .expect("writing JSON to a String cannot fail");
        output.push_str(",\"job_members\":[");
        for (i, member) in self.job_members.iter().enumerate() {
            if i != 0 {
                output.push(',');
            }
            output.push_str("{\"pid\":");
            write!(&mut output, "{}", member.pid).expect("writing JSON to a String cannot fail");
            output.push_str(",\"name\":");
            write_string(&mut output, &member.name);
            write!(
                &mut output,
                ",\"in_job\":{},\"breakaway\":{}}}",
                if member.in_job { "true" } else { "false" },
                if member.breakaway { "true" } else { "false" },
            )
            .expect("writing JSON to a String cannot fail");
        }
        output.push(']');
        output.push_str(",\"breakaway_events\":[");
        for (i, event) in self.breakaway_events.iter().enumerate() {
            if i != 0 {
                output.push(',');
            }
            output.push_str("{\"pid\":");
            write!(&mut output, "{}", event.pid).expect("writing JSON to a String cannot fail");
            output.push_str(",\"name\":");
            write_string(&mut output, &event.name);
            output.push_str(",\"event\":");
            write_string(&mut output, &event.event);
            output.push('}');
        }
        output.push(']');
        output.push_str(",\"exit_report\":");
        write_string(&mut output, &self.exit_report);
        output.push_str(",\"state\":");
        write_string(&mut output, self.state.as_json_str());
        output.push_str(",\"created_at_utc\":");
        write_string(&mut output, &self.created_at_utc);
        output.push('}');
        Ok(output)
    }

    pub fn from_json(json: &[u8]) -> Result<Self, ContractError> {
        let parsed = JsonParser::new(json)?.parse_top_level()?;
        let _obj = parsed.as_object()?;

        let schema = parsed.field("schema")?.as_str()?;
        if schema != OWNERSHIP_SCHEMA {
            return Err(ContractError::new(
                "schema_mismatch",
                "schema",
                format!("expected {}, got {}", OWNERSHIP_SCHEMA, schema),
            ));
        }

        let get_str = |key: &str| -> Result<String, ContractError> {
            Ok(parsed.field(key)?.as_str()?.to_owned())
        };
        let get_u64 = |key: &str| -> Result<u64, ContractError> {
            let n = parsed.field(key)?.as_i64()?;
            if n < 0 {
                return Err(ContractError::new(
                    "json_value",
                    key,
                    "must be non-negative",
                ));
            }
            Ok(n as u64)
        };

        let launch_id = get_str("launch_id")?;
        let root_pid = get_u64("root_pid")?;
        let exit_report = get_str("exit_report")?;
        let state = OwnershipState::from_json_str(get_str("state")?.as_str())?;
        let created_at_utc = get_str("created_at_utc")?;

        let members_value = parsed.field("job_members")?;
        let members_array = members_value.as_array()?;
        let mut job_members = Vec::with_capacity(members_array.len());
        for entry in members_array {
            job_members.push(JobMemberEntry {
                pid: entry.field("pid")?.as_i64()? as u64,
                name: entry.field("name")?.as_str()?.to_owned(),
                in_job: entry.field("in_job")?.as_bool()?,
                breakaway: entry.field("breakaway")?.as_bool()?,
            });
        }

        let events_value = parsed.field("breakaway_events")?;
        let events_array = events_value.as_array()?;
        let mut breakaway_events = Vec::with_capacity(events_array.len());
        for entry in events_array {
            breakaway_events.push(BreakawayEvent {
                pid: entry.field("pid")?.as_i64()? as u64,
                name: entry.field("name")?.as_str()?.to_owned(),
                event: entry.field("event")?.as_str()?.to_owned(),
            });
        }

        let ownership = OwnershipV1 {
            launch_id,
            root_pid,
            job_members,
            breakaway_events,
            exit_report,
            state,
            created_at_utc,
        };
        ownership.validate()?;
        Ok(ownership)
    }
}

/// The shutdown mode for a controlled owned-tree shutdown.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ShutdownMode {
    /// Graceful shutdown: signal the owned root and let the tree exit.
    Graceful,
    /// Job close: close the Job Object so the owned tree is terminated.
    JobClose,
}

impl ShutdownMode {
    pub(crate) fn as_json_str(self) -> &'static str {
        match self {
            Self::Graceful => "graceful",
            Self::JobClose => "job_close",
        }
    }

    pub(crate) fn from_json_str(s: &str) -> Result<Self, ContractError> {
        match s {
            "graceful" => Ok(Self::Graceful),
            "job_close" => Ok(Self::JobClose),
            other => Err(ContractError::new(
                "invalid_value",
                "shutdown_mode",
                format!("expected 'graceful' or 'job_close', got '{other}'"),
            )),
        }
    }
}

/// The shutdown state machine: prepared -> shutdown -> closed, or failed.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ShutdownState {
    Prepared,
    Shutdown,
    Closed,
    Failed,
}

impl ShutdownState {
    fn as_json_str(self) -> &'static str {
        match self {
            Self::Prepared => "prepared",
            Self::Shutdown => "shutdown",
            Self::Closed => "closed",
            Self::Failed => "failed",
        }
    }

    fn from_json_str(s: &str) -> Result<Self, ContractError> {
        match s {
            "prepared" => Ok(Self::Prepared),
            "shutdown" => Ok(Self::Shutdown),
            "closed" => Ok(Self::Closed),
            "failed" => Ok(Self::Failed),
            other => Err(ContractError::new(
                "invalid_value",
                "state",
                format!("expected 'prepared', 'shutdown', 'closed', or 'failed', got '{other}'"),
            )),
        }
    }
}

/// The controlled shutdown receipt (`chatgpt_fix.shutdown.v1`).
///
/// Produced by `ChatGPT-Fix-Launcher shutdown --fixture-root <path>`.
/// Only PIDs that are explicit owned members of the reconciled tree may be
/// shut down (`handled_pids`). Breakaway/outside-Job members are `excluded`;
/// reconciliation failures are `suspected` and any suspected hit fails the
/// shutdown closed (fail closed).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShutdownV1 {
    pub launch_id: String,
    pub root_pid: u64,
    pub shutdown_mode: ShutdownMode,
    pub handled_pids: Vec<u64>,
    pub excluded_pids: Vec<u64>,
    pub suspected_pids: Vec<u64>,
    pub state: ShutdownState,
    pub created_at_utc: String,
}

impl ShutdownV1 {
    pub fn validate(&self) -> Result<(), ContractError> {
        validate_text("launch_id", &self.launch_id)?;
        validate_text("created_at_utc", &self.created_at_utc)?;
        if self.root_pid == 0 {
            return Err(ContractError::new(
                "invalid_value",
                "root_pid",
                "must be greater than zero",
            ));
        }
        for pid in self
            .handled_pids
            .iter()
            .chain(self.excluded_pids.iter())
            .chain(self.suspected_pids.iter())
        {
            if *pid == 0 {
                return Err(ContractError::new(
                    "invalid_value",
                    "pids",
                    "every listed pid must be greater than zero",
                ));
            }
        }
        if self.state != ShutdownState::Failed && !self.handled_pids.contains(&self.root_pid) {
            return Err(ContractError::new(
                "invalid_value",
                "handled_pids",
                "the owned root pid must be part of the handled set",
            ));
        }
        if self.state == ShutdownState::Failed && self.suspected_pids.is_empty() {
            return Err(ContractError::new(
                "invalid_value",
                "suspected_pids",
                "a failed shutdown must record at least one suspected pid",
            ));
        }
        if !self.suspected_pids.is_empty() && self.state != ShutdownState::Failed {
            return Err(ContractError::new(
                "invalid_value",
                "state",
                "a shutdown with suspected pids must be failed",
            ));
        }
        Ok(())
    }

    pub fn to_json(&self) -> Result<String, ContractError> {
        let mut output = String::new();
        output.push_str("{\"schema\":");
        write_string(&mut output, SHUTDOWN_SCHEMA);
        output.push_str(",\"launch_id\":");
        write_string(&mut output, &self.launch_id);
        write!(&mut output, ",\"root_pid\":{}", self.root_pid)
            .expect("writing JSON to a String cannot fail");
        output.push_str(",\"shutdown_mode\":");
        write_string(&mut output, self.shutdown_mode.as_json_str());
        output.push_str(",\"handled_pids\":[");
        for (i, pid) in self.handled_pids.iter().enumerate() {
            if i != 0 {
                output.push(',');
            }
            write!(&mut output, "{pid}").expect("writing JSON to a String cannot fail");
        }
        output.push(']');
        output.push_str(",\"excluded_pids\":[");
        for (i, pid) in self.excluded_pids.iter().enumerate() {
            if i != 0 {
                output.push(',');
            }
            write!(&mut output, "{pid}").expect("writing JSON to a String cannot fail");
        }
        output.push(']');
        output.push_str(",\"suspected_pids\":[");
        for (i, pid) in self.suspected_pids.iter().enumerate() {
            if i != 0 {
                output.push(',');
            }
            write!(&mut output, "{pid}").expect("writing JSON to a String cannot fail");
        }
        output.push(']');
        output.push_str(",\"state\":");
        write_string(&mut output, self.state.as_json_str());
        output.push_str(",\"created_at_utc\":");
        write_string(&mut output, &self.created_at_utc);
        output.push('}');
        Ok(output)
    }

    pub fn from_json(json: &[u8]) -> Result<Self, ContractError> {
        let parsed = JsonParser::new(json)?.parse_top_level()?;
        let _obj = parsed.as_object()?;

        let schema = parsed.field("schema")?.as_str()?;
        if schema != SHUTDOWN_SCHEMA {
            return Err(ContractError::new(
                "schema_mismatch",
                "schema",
                format!("expected {}, got {}", SHUTDOWN_SCHEMA, schema),
            ));
        }

        let get_str = |key: &str| -> Result<String, ContractError> {
            Ok(parsed.field(key)?.as_str()?.to_owned())
        };
        let get_u64 = |key: &str| -> Result<u64, ContractError> {
            let n = parsed.field(key)?.as_i64()?;
            if n < 0 {
                return Err(ContractError::new(
                    "json_value",
                    key,
                    "must be non-negative",
                ));
            }
            Ok(n as u64)
        };

        let launch_id = get_str("launch_id")?;
        let root_pid = get_u64("root_pid")?;
        let shutdown_mode = ShutdownMode::from_json_str(get_str("shutdown_mode")?.as_str())?;
        let state = ShutdownState::from_json_str(get_str("state")?.as_str())?;
        let created_at_utc = get_str("created_at_utc")?;

        let get_pids = |key: &str| -> Result<Vec<u64>, ContractError> {
            let value = parsed.field(key)?;
            let array = value.as_array()?;
            let mut pids = Vec::with_capacity(array.len());
            for entry in array {
                let n = entry.as_i64()?;
                if n <= 0 {
                    return Err(ContractError::new(
                        "invalid_value",
                        key,
                        "every pid must be greater than zero",
                    ));
                }
                pids.push(n as u64);
            }
            Ok(pids)
        };

        let shutdown = ShutdownV1 {
            launch_id,
            root_pid,
            shutdown_mode,
            handled_pids: get_pids("handled_pids")?,
            excluded_pids: get_pids("excluded_pids")?,
            suspected_pids: get_pids("suspected_pids")?,
            state,
            created_at_utc,
        };
        shutdown.validate()?;
        Ok(shutdown)
    }
}

/// The effective-config scope for a proposal.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ConfigScope {
    /// MCP server enablement configuration.
    Mcp,
    /// `CODEX_HOME` state-root redirection.
    CodexHome,
    /// History persistence/max-bytes configuration.
    History,
}

impl ConfigScope {
    pub(crate) fn as_json_str(self) -> &'static str {
        match self {
            Self::Mcp => "mcp",
            Self::CodexHome => "codex_home",
            Self::History => "history",
        }
    }

    pub(crate) fn from_json_str(s: &str) -> Result<Self, ContractError> {
        match s {
            "mcp" => Ok(Self::Mcp),
            "codex_home" => Ok(Self::CodexHome),
            "history" => Ok(Self::History),
            other => Err(ContractError::new(
                "invalid_value",
                "scope",
                format!("expected 'mcp', 'codex_home', or 'history', got '{other}'"),
            )),
        }
    }
}

/// Where a proposal may be applied.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ConfigApplyMode {
    /// Apply to an empty canary profile (verification only).
    Canary,
    /// Apply to the real effective configuration (P8+ only).
    Real,
}

impl ConfigApplyMode {
    pub(crate) fn as_json_str(self) -> &'static str {
        match self {
            Self::Canary => "canary",
            Self::Real => "real",
        }
    }

    pub(crate) fn from_json_str(s: &str) -> Result<Self, ContractError> {
        match s {
            "canary" => Ok(Self::Canary),
            "real" => Ok(Self::Real),
            other => Err(ContractError::new(
                "invalid_value",
                "apply_mode",
                format!("expected 'canary' or 'real', got '{other}'"),
            )),
        }
    }
}

/// The config proposal state machine: proposed -> applied, or rolled_back.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ConfigProposalState {
    Proposed,
    Applied,
    RolledBack,
}

impl ConfigProposalState {
    fn as_json_str(self) -> &'static str {
        match self {
            Self::Proposed => "proposed",
            Self::Applied => "applied",
            Self::RolledBack => "rolled_back",
        }
    }

    fn from_json_str(s: &str) -> Result<Self, ContractError> {
        match s {
            "proposed" => Ok(Self::Proposed),
            "applied" => Ok(Self::Applied),
            "rolled_back" => Ok(Self::RolledBack),
            other => Err(ContractError::new(
                "invalid_value",
                "state",
                format!("expected 'proposed', 'applied', or 'rolled_back', got '{other}'"),
            )),
        }
    }
}

/// The effective-config proposal receipt (`chatgpt_fix.config_proposal.v1`).
///
/// Produced by `ChatGPT-Fix-Manager config-plan --scope <mcp|codex_home|history>`.
/// Applying is only allowed on an empty canary profile (A6 scope); every apply
/// binds an independent backup and rollback. Auth/token/cookie content is never
/// part of a proposal.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConfigProposalV1 {
    pub proposal_id: String,
    pub scope: ConfigScope,
    pub current_digest: String,
    pub proposed_digest: String,
    pub backup_path: String,
    pub apply_mode: ConfigApplyMode,
    pub state: ConfigProposalState,
    pub created_at_utc: String,
}

impl ConfigProposalV1 {
    pub fn validate(&self) -> Result<(), ContractError> {
        validate_text("proposal_id", &self.proposal_id)?;
        validate_text("current_digest", &self.current_digest)?;
        validate_text("proposed_digest", &self.proposed_digest)?;
        validate_text("backup_path", &self.backup_path)?;
        validate_text("created_at_utc", &self.created_at_utc)?;
        if self.current_digest.is_empty() || self.proposed_digest.is_empty() {
            return Err(ContractError::new(
                "invalid_value",
                "digest",
                "digests must not be empty",
            ));
        }
        Ok(())
    }

    pub fn to_json(&self) -> Result<String, ContractError> {
        let mut output = String::new();
        output.push_str("{\"schema\":");
        write_string(&mut output, CONFIG_PROPOSAL_SCHEMA);
        output.push_str(",\"proposal_id\":");
        write_string(&mut output, &self.proposal_id);
        output.push_str(",\"scope\":");
        write_string(&mut output, self.scope.as_json_str());
        output.push_str(",\"current_digest\":");
        write_string(&mut output, &self.current_digest);
        output.push_str(",\"proposed_digest\":");
        write_string(&mut output, &self.proposed_digest);
        output.push_str(",\"backup_path\":");
        write_string(&mut output, &self.backup_path);
        output.push_str(",\"apply_mode\":");
        write_string(&mut output, self.apply_mode.as_json_str());
        output.push_str(",\"state\":");
        write_string(&mut output, self.state.as_json_str());
        output.push_str(",\"created_at_utc\":");
        write_string(&mut output, &self.created_at_utc);
        output.push('}');
        Ok(output)
    }

    pub fn from_json(json: &[u8]) -> Result<Self, ContractError> {
        let parsed = JsonParser::new(json)?.parse_top_level()?;
        let _obj = parsed.as_object()?;

        let schema = parsed.field("schema")?.as_str()?;
        if schema != CONFIG_PROPOSAL_SCHEMA {
            return Err(ContractError::new(
                "schema_mismatch",
                "schema",
                format!("expected {}, got {}", CONFIG_PROPOSAL_SCHEMA, schema),
            ));
        }

        let get_str = |key: &str| -> Result<String, ContractError> {
            Ok(parsed.field(key)?.as_str()?.to_owned())
        };

        let proposal = ConfigProposalV1 {
            proposal_id: get_str("proposal_id")?,
            scope: ConfigScope::from_json_str(get_str("scope")?.as_str())?,
            current_digest: get_str("current_digest")?,
            proposed_digest: get_str("proposed_digest")?,
            backup_path: get_str("backup_path")?,
            apply_mode: ConfigApplyMode::from_json_str(get_str("apply_mode")?.as_str())?,
            state: ConfigProposalState::from_json_str(get_str("state")?.as_str())?,
            created_at_utc: get_str("created_at_utc")?,
        };
        proposal.validate()?;
        Ok(proposal)
    }
}

/// A single dry-run maintenance item.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MaintenanceItem {
    pub action: String,
    pub scope: String,
    pub would_do: String,
}

/// The maintenance dry-run plan (`chatgpt_fix.maintenance_plan.v1`).
///
/// Produced by `maintenance-plan --fixture-root <path>` on Packer, Manager,
/// and Launcher. `dry_run` is always `true`: the plan lists would-do items
/// only and never executes them. Items that require A7 (.codex maintenance),
/// A8 (Setup/privilege/signing), or A9 (publication) are marked `blocked` and
/// stay outside the would-do list.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MaintenancePlanV1 {
    pub plan_id: String,
    pub dry_run: bool,
    pub items: Vec<MaintenanceItem>,
    pub blocked: Vec<String>,
    pub state: String,
    pub created_at_utc: String,
}

impl MaintenancePlanV1 {
    pub fn validate(&self) -> Result<(), ContractError> {
        validate_text("plan_id", &self.plan_id)?;
        validate_text("created_at_utc", &self.created_at_utc)?;
        if !self.dry_run {
            return Err(ContractError::new(
                "invalid_value",
                "dry_run",
                "maintenance plans must always be dry-run",
            ));
        }
        if self.state != "dry_run" {
            return Err(ContractError::new(
                "invalid_value",
                "state",
                "maintenance plan state must be 'dry_run'",
            ));
        }
        for item in &self.items {
            validate_text("action", &item.action)?;
            validate_text("scope", &item.scope)?;
            validate_text("would_do", &item.would_do)?;
        }
        for blocked in &self.blocked {
            validate_text("blocked", blocked)?;
        }
        Ok(())
    }

    pub fn to_json(&self) -> Result<String, ContractError> {
        let mut output = String::new();
        output.push_str("{\"schema\":");
        write_string(&mut output, MAINTENANCE_PLAN_SCHEMA);
        output.push_str(",\"plan_id\":");
        write_string(&mut output, &self.plan_id);
        output.push_str(&format!(
            ",\"dry_run\":{},",
            if self.dry_run { "true" } else { "false" }
        ));
        output.push_str("\"items\":[");
        for (i, item) in self.items.iter().enumerate() {
            if i != 0 {
                output.push(',');
            }
            output.push_str("{\"action\":");
            write_string(&mut output, &item.action);
            output.push_str(",\"scope\":");
            write_string(&mut output, &item.scope);
            output.push_str(",\"would_do\":");
            write_string(&mut output, &item.would_do);
            output.push('}');
        }
        output.push(']');
        output.push_str(",\"blocked\":[");
        for (i, blocked) in self.blocked.iter().enumerate() {
            if i != 0 {
                output.push(',');
            }
            write_string(&mut output, blocked);
        }
        output.push(']');
        output.push_str(",\"state\":");
        write_string(&mut output, &self.state);
        output.push_str(",\"created_at_utc\":");
        write_string(&mut output, &self.created_at_utc);
        output.push('}');
        Ok(output)
    }

    pub fn from_json(json: &[u8]) -> Result<Self, ContractError> {
        let parsed = JsonParser::new(json)?.parse_top_level()?;
        let _obj = parsed.as_object()?;

        let schema = parsed.field("schema")?.as_str()?;
        if schema != MAINTENANCE_PLAN_SCHEMA {
            return Err(ContractError::new(
                "schema_mismatch",
                "schema",
                format!("expected {}, got {}", MAINTENANCE_PLAN_SCHEMA, schema),
            ));
        }

        let get_str = |key: &str| -> Result<String, ContractError> {
            Ok(parsed.field(key)?.as_str()?.to_owned())
        };

        let plan_id = get_str("plan_id")?;
        let dry_run = parsed.field("dry_run")?.as_bool()?;
        let state = get_str("state")?;
        let created_at_utc = get_str("created_at_utc")?;

        let items_value = parsed.field("items")?;
        let items_array = items_value.as_array()?;
        let mut items = Vec::with_capacity(items_array.len());
        for entry in items_array {
            items.push(MaintenanceItem {
                action: entry.field("action")?.as_str()?.to_owned(),
                scope: entry.field("scope")?.as_str()?.to_owned(),
                would_do: entry.field("would_do")?.as_str()?.to_owned(),
            });
        }

        let blocked_value = parsed.field("blocked")?;
        let blocked_array = blocked_value.as_array()?;
        let mut blocked = Vec::with_capacity(blocked_array.len());
        for entry in blocked_array {
            blocked.push(entry.as_str()?.to_owned());
        }

        let plan = MaintenancePlanV1 {
            plan_id,
            dry_run,
            items,
            blocked,
            state,
            created_at_utc,
        };
        plan.validate()?;
        Ok(plan)
    }
}

/// The native token-cost reapply state (read-only registration).
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum NtcReapplyState {
    Clean,
    NeedsReapply,
    RepairReceipt,
}

impl NtcReapplyState {
    pub(crate) fn as_json_str(self) -> &'static str {
        match self {
            Self::Clean => "clean",
            Self::NeedsReapply => "needs_reapply",
            Self::RepairReceipt => "repair_receipt",
        }
    }

    pub(crate) fn from_json_str(s: &str) -> Result<Self, ContractError> {
        match s {
            "clean" => Ok(Self::Clean),
            "needs_reapply" => Ok(Self::NeedsReapply),
            "repair_receipt" => Ok(Self::RepairReceipt),
            other => Err(ContractError::new(
                "invalid_value",
                "reapply_state",
                format!("expected 'clean', 'needs_reapply', or 'repair_receipt', got '{other}'"),
            )),
        }
    }
}

/// The native token-cost manifest (`chatgpt_fix.ntc_manifest.v1`).
///
/// A read-only registration of NTC-NATIVE-20260801 delegated evidence:
/// before/after hashes, backup ref, generation, and reapply state. P8 only
/// registers; it never re-packs app.asar or mutates the helper task.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NtcManifestV1 {
    pub artifact: String,
    pub before_sha256: String,
    pub after_sha256: String,
    pub backup_ref: String,
    pub generation: u64,
    pub reapply_state: NtcReapplyState,
    pub helper_health: String,
    pub created_at_utc: String,
}

impl NtcManifestV1 {
    pub fn validate(&self) -> Result<(), ContractError> {
        validate_text("artifact", &self.artifact)?;
        validate_text("before_sha256", &self.before_sha256)?;
        validate_text("after_sha256", &self.after_sha256)?;
        validate_text("backup_ref", &self.backup_ref)?;
        validate_text("helper_health", &self.helper_health)?;
        validate_text("created_at_utc", &self.created_at_utc)?;
        if self.before_sha256.is_empty() || self.after_sha256.is_empty() {
            return Err(ContractError::new(
                "invalid_value",
                "sha256",
                "before/after hashes must not be empty",
            ));
        }
        Ok(())
    }

    pub fn to_json(&self) -> Result<String, ContractError> {
        let mut output = String::new();
        output.push_str("{\"schema\":");
        write_string(&mut output, NTC_MANIFEST_SCHEMA);
        output.push_str(",\"artifact\":");
        write_string(&mut output, &self.artifact);
        output.push_str(",\"before_sha256\":");
        write_string(&mut output, &self.before_sha256);
        output.push_str(",\"after_sha256\":");
        write_string(&mut output, &self.after_sha256);
        output.push_str(",\"backup_ref\":");
        write_string(&mut output, &self.backup_ref);
        output.push_str(&format!(",\"generation\":{}", self.generation));
        output.push_str(",\"reapply_state\":");
        write_string(&mut output, self.reapply_state.as_json_str());
        output.push_str(",\"helper_health\":");
        write_string(&mut output, &self.helper_health);
        output.push_str(",\"created_at_utc\":");
        write_string(&mut output, &self.created_at_utc);
        output.push('}');
        Ok(output)
    }

    pub fn from_json(json: &[u8]) -> Result<Self, ContractError> {
        let parsed = JsonParser::new(json)?.parse_top_level()?;
        let _obj = parsed.as_object()?;

        let schema = parsed.field("schema")?.as_str()?;
        if schema != NTC_MANIFEST_SCHEMA {
            return Err(ContractError::new(
                "schema_mismatch",
                "schema",
                format!("expected {}, got {}", NTC_MANIFEST_SCHEMA, schema),
            ));
        }

        let get_str = |key: &str| -> Result<String, ContractError> {
            Ok(parsed.field(key)?.as_str()?.to_owned())
        };

        let generation = parsed.field("generation")?.as_i64()?;
        if generation < 0 {
            return Err(ContractError::new(
                "json_value",
                "generation",
                "must be non-negative",
            ));
        }

        let manifest = NtcManifestV1 {
            artifact: get_str("artifact")?,
            before_sha256: get_str("before_sha256")?,
            after_sha256: get_str("after_sha256")?,
            backup_ref: get_str("backup_ref")?,
            generation: generation as u64,
            reapply_state: NtcReapplyState::from_json_str(get_str("reapply_state")?.as_str())?,
            helper_health: get_str("helper_health")?,
            created_at_utc: get_str("created_at_utc")?,
        };
        manifest.validate()?;
        Ok(manifest)
    }
}
