use std::collections::HashSet;
use std::fmt::Write;

use crate::json::{JsonParser, JsonValue, write_string};
use crate::{ContractError, SafeRelativePath, Sha256Digest};

pub const PLAN_SCHEMA: &str = "chatgpt_fix.plan.v1";
pub const BASELINE_SCHEMA: &str = "chatgpt_fix.baseline.v2";
pub const LAUNCH_SCHEMA: &str = "chatgpt_fix.launch.v1";
pub const RECEIPT_SCHEMA: &str = "chatgpt_fix.receipt.v1";
pub const LIVE_INSPECTION_SCHEMA: &str = "chatgpt_fix.live_inspection.v1";

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
