use std::collections::HashSet;
use std::fmt::Write;

use crate::json::write_string;
use crate::{ContractError, SafeRelativePath, Sha256Digest};

pub const PLAN_SCHEMA: &str = "chatgpt_fix.plan.v1";
pub const BASELINE_SCHEMA: &str = "chatgpt_fix.baseline.v2";
pub const LAUNCH_SCHEMA: &str = "chatgpt_fix.launch.v1";
pub const RECEIPT_SCHEMA: &str = "chatgpt_fix.receipt.v1";

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
