use crate::json::write_string;
use crate::ContractError;

pub const DOCTOR_SCHEMA: &str = "chatgpt_fix.doctor.v2";

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum DoctorStatus {
    Ok,
    Warning,
    HighRisk,
}

impl DoctorStatus {
    fn as_json_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Warning => "warning",
            Self::HighRisk => "high-risk",
        }
    }

    fn from_json_str(s: &str) -> Result<Self, ContractError> {
        match s {
            "ok" => Ok(Self::Ok),
            "warning" => Ok(Self::Warning),
            "high-risk" => Ok(Self::HighRisk),
            other => Err(ContractError::new(
                "invalid_value",
                "overall_status",
                format!("expected 'ok', 'warning', or 'high-risk', got '{}'", other),
            )),
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum DoctorLevel {
    Ok,
    Warning,
    High,
}

impl DoctorLevel {
    fn as_json_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Warning => "warning",
            Self::High => "high",
        }
    }

    fn from_json_str(s: &str) -> Result<Self, ContractError> {
        match s {
            "ok" => Ok(Self::Ok),
            "warning" => Ok(Self::Warning),
            "high" => Ok(Self::High),
            other => Err(ContractError::new(
                "invalid_value",
                "level",
                format!("expected 'ok', 'warning', or 'high', got '{}'", other),
            )),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DoctorFinding {
    pub level: DoctorLevel,
    pub code: String,
    pub message: String,
    pub evidence_json: Option<String>,
    pub would_do: Option<String>,
}

impl DoctorFinding {
    pub fn validate(&self) -> Result<(), ContractError> {
        validate_doctor_text("code", &self.code)?;
        validate_doctor_text("message", &self.message)?;
        if let Some(ref evidence) = self.evidence_json {
            // Validate that evidence is a non-empty string.
            if evidence.is_empty() {
                return Err(ContractError::new(
                    "empty_field",
                    "evidence_json",
                    "must not be empty when present",
                ));
            }
        }
        if let Some(ref would_do) = self.would_do {
            validate_doctor_text("would_do", would_do)?;
        }
        Ok(())
    }

    fn write_json(&self, output: &mut String) {
        output.push_str("{\"level\":");
        write_string(output, self.level.clone().as_json_str());
        output.push_str(",\"code\":");
        write_string(output, &self.code);
        output.push_str(",\"message\":");
        write_string(output, &self.message);
        output.push_str(",\"evidence_json\":");
        match &self.evidence_json {
            Some(e) => {
                // Store as a JSON string value; the content is pre-serialized JSON.
                write_string(output, e);
            }
            None => output.push_str("null"),
        }
        output.push_str(",\"would_do\":");
        match &self.would_do {
            Some(w) => write_string(output, w),
            None => output.push_str("null"),
        }
        output.push('}');
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DoctorV2 {
    pub read_only: bool,
    pub timestamp_utc: String,
    pub findings: Vec<DoctorFinding>,
    pub overall_status: DoctorStatus,
}

impl DoctorV2 {
    pub fn validate(&self) -> Result<(), ContractError> {
        if !self.read_only {
            return Err(ContractError::new(
                "invariant_violation",
                "read_only",
                "P2 doctor must be read-only",
            ));
        }
        validate_doctor_text("timestamp_utc", &self.timestamp_utc)?;

        if self.findings.is_empty() {
            return Err(ContractError::new(
                "invariant_violation",
                "findings",
                "doctor must contain at least one finding",
            ));
        }

        for finding in &self.findings {
            finding.validate()?;
        }

        // Verify overall_status consistency with findings.
        let computed_status = compute_status(&self.findings);
        if computed_status != self.overall_status {
            return Err(ContractError::new(
                "invariant_violation",
                "overall_status",
                format!(
                    "computed {:?} from findings, but declared {:?}",
                    computed_status, self.overall_status
                ),
            ));
        }

        Ok(())
    }

    pub fn to_json(&self) -> Result<String, ContractError> {
        self.validate()?;

        let mut output = String::new();
        output.push_str("{\"schema\":");
        write_string(&mut output, DOCTOR_SCHEMA);
        output.push_str(",\"read_only\":");
        output.push_str(if self.read_only { "true" } else { "false" });
        output.push_str(",\"timestamp_utc\":");
        write_string(&mut output, &self.timestamp_utc);
        output.push_str(",\"overall_status\":");
        write_string(&mut output, self.overall_status.as_json_str());

        output.push_str(",\"findings\":[");
        for (i, finding) in self.findings.iter().enumerate() {
            if i != 0 {
                output.push(',');
            }
            finding.write_json(&mut output);
        }
        output.push_str("]}");
        Ok(output)
    }

    pub fn from_json(json: &[u8]) -> Result<Self, ContractError> {
        use crate::json::{JsonParser, JsonValue};

        let parsed = JsonParser::new(json)?.parse_top_level()?;
        let _obj = parsed.as_object()?;

        let schema = parsed.field("schema")?.as_str()?;
        if schema != DOCTOR_SCHEMA {
            return Err(ContractError::new(
                "schema_mismatch",
                "schema",
                format!("expected {}, got {}", DOCTOR_SCHEMA, schema),
            ));
        }

        let get_str = |key: &str| -> Result<String, ContractError> {
            Ok(parsed.field(key)?.as_str()?.to_owned())
        };
        let get_bool = |key: &str| -> Result<bool, ContractError> {
            parsed.field(key)?.as_bool()
        };

        let read_only = get_bool("read_only")?;
        let timestamp_utc = get_str("timestamp_utc")?;
        let overall_status_str = get_str("overall_status")?;
        let overall_status = DoctorStatus::from_json_str(&overall_status_str)?;

        let findings_arr = parsed.field("findings")?.as_array()?;
        let mut findings = Vec::with_capacity(findings_arr.len());
        for finding_val in findings_arr {
            let _obj = finding_val.as_object()?;

            let level_str = finding_val.field("level")?.as_str()?;
            let level = DoctorLevel::from_json_str(level_str)?;
            let code = finding_val.field("code")?.as_str()?.to_owned();
            let message = finding_val.field("message")?.as_str()?.to_owned();

            let evidence_json = match finding_val.field("evidence_json")? {
                JsonValue::Null => None,
                _ => Some(finding_val.field("evidence_json")?.as_str()?.to_owned()),
            };

            let would_do = match finding_val.field("would_do")? {
                JsonValue::Null => None,
                _ => Some(finding_val.field("would_do")?.as_str()?.to_owned()),
            };

            findings.push(DoctorFinding {
                level,
                code,
                message,
                evidence_json,
                would_do,
            });
        }

        let doctor = DoctorV2 {
            read_only,
            timestamp_utc,
            findings,
            overall_status,
        };

        doctor.validate()?;
        Ok(doctor)
    }
}

fn compute_status(findings: &[DoctorFinding]) -> DoctorStatus {
    let mut has_high = false;
    let mut has_warning = false;
    for finding in findings {
        match finding.level {
            DoctorLevel::High => has_high = true,
            DoctorLevel::Warning => has_warning = true,
            DoctorLevel::Ok => {}
        }
    }
    if has_high {
        DoctorStatus::HighRisk
    } else if has_warning {
        DoctorStatus::Warning
    } else {
        DoctorStatus::Ok
    }
}

fn validate_doctor_text(field: &'static str, value: &str) -> Result<(), ContractError> {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_doctor() -> DoctorV2 {
        DoctorV2 {
            read_only: true,
            timestamp_utc: "2026-08-06T12:00:00Z".to_owned(),
            findings: vec![
                DoctorFinding {
                    level: DoctorLevel::Ok,
                    code: "doctor_available".to_owned(),
                    message: "P2 doctor v2 proposal is available".to_owned(),
                    evidence_json: None,
                    would_do: None,
                },
                DoctorFinding {
                    level: DoctorLevel::High,
                    code: "version_drift".to_owned(),
                    message: "local baseline version differs from official package".to_owned(),
                    evidence_json: Some(
                        "{\"official\":\"26.721.4979.0\",\"baseline\":\"26.707.8479.0\"}"
                            .to_owned(),
                    ),
                    would_do: Some(
                        "P2 would report drift; P3 would stage new baseline".to_owned(),
                    ),
                },
            ],
            overall_status: DoctorStatus::HighRisk,
        }
    }

    #[test]
    fn validates_ok_doctor() {
        let doctor = sample_doctor();
        assert!(doctor.validate().is_ok());
    }

    #[test]
    fn rejects_read_only_false() {
        let doctor = DoctorV2 {
            read_only: false,
            ..sample_doctor()
        };
        let err = doctor.validate().unwrap_err();
        assert!(err.message.contains("read-only"), "{}", err.message);
    }

    #[test]
    fn rejects_empty_findings() {
        let doctor = DoctorV2 {
            findings: vec![],
            overall_status: DoctorStatus::Ok,
            ..sample_doctor()
        };
        let err = doctor.validate().unwrap_err();
        assert!(err.message.contains("at least one finding"), "{}", err.message);
    }

    #[test]
    fn rejects_status_mismatch() {
        let doctor = DoctorV2 {
            overall_status: DoctorStatus::Ok, // has High findings
            ..sample_doctor()
        };
        let err = doctor.validate().unwrap_err();
        assert!(
            err.message.contains("computed"),
            "expected computed status mismatch: {}",
            err.message
        );
    }

    #[test]
    fn computes_status_correctly() {
        let all_ok = vec![
            DoctorFinding {
                level: DoctorLevel::Ok,
                code: "a".to_owned(),
                message: "ok".to_owned(),
                evidence_json: None,
                would_do: None,
            },
        ];
        assert_eq!(compute_status(&all_ok), DoctorStatus::Ok);

        let with_warning = vec![
            DoctorFinding {
                level: DoctorLevel::Ok,
                code: "a".to_owned(),
                message: "ok".to_owned(),
                evidence_json: None,
                would_do: None,
            },
            DoctorFinding {
                level: DoctorLevel::Warning,
                code: "b".to_owned(),
                message: "warn".to_owned(),
                evidence_json: None,
                would_do: None,
            },
        ];
        assert_eq!(compute_status(&with_warning), DoctorStatus::Warning);

        let with_high = vec![
            DoctorFinding {
                level: DoctorLevel::Ok,
                code: "a".to_owned(),
                message: "ok".to_owned(),
                evidence_json: None,
                would_do: None,
            },
            DoctorFinding {
                level: DoctorLevel::High,
                code: "c".to_owned(),
                message: "high".to_owned(),
                evidence_json: None,
                would_do: None,
            },
        ];
        assert_eq!(compute_status(&with_high), DoctorStatus::HighRisk);
    }

    #[test]
    fn to_json_round_trips() {
        let doctor = sample_doctor();
        let json = doctor.to_json().expect("serialize");
        let parsed = DoctorV2::from_json(json.as_bytes()).expect("deserialize");
        assert_eq!(parsed, doctor);
    }

    #[test]
    fn to_json_is_canonical() {
        let doctor = sample_doctor();
        let json1 = doctor.to_json().expect("serialize");
        let json2 = doctor.to_json().expect("serialize again");
        assert_eq!(json1, json2, "canonical JSON must be deterministic");
    }

    #[test]
    fn rejects_empty_code() {
        let finding = DoctorFinding {
            level: DoctorLevel::Ok,
            code: "".to_owned(),
            message: "test".to_owned(),
            evidence_json: None,
            would_do: None,
        };
        let err = finding.validate().unwrap_err();
        assert!(err.message.contains("must not be empty"), "{}", err.message);
    }

    #[test]
    fn rejects_control_char_in_message() {
        let finding = DoctorFinding {
            level: DoctorLevel::Ok,
            code: "test".to_owned(),
            message: "bad\u{0000}char".to_owned(),
            evidence_json: None,
            would_do: None,
        };
        let err = finding.validate().unwrap_err();
        assert!(err.message.contains("control"), "{}", err.message);
    }

    #[test]
    fn from_json_rejects_wrong_schema() {
        let bad_json = b"{\"schema\":\"wrong.schema\",\"read_only\":true,\"timestamp_utc\":\"t\",\"overall_status\":\"ok\",\"findings\":[{\"level\":\"ok\",\"code\":\"a\",\"message\":\"b\",\"evidence_json\":null,\"would_do\":null}]}";
        let err = DoctorV2::from_json(bad_json).unwrap_err();
        assert_eq!(err.code, "schema_mismatch", "unexpected code: {}", err.code);
    }

    #[test]
    fn from_json_rejects_noncanonical() {
        // Trailing data.
        let bad_json = b"{\"schema\":\"chatgpt_fix.doctor.v2\",\"read_only\":true,\"timestamp_utc\":\"t\",\"overall_status\":\"ok\",\"findings\":[{\"level\":\"ok\",\"code\":\"a\",\"message\":\"b\",\"evidence_json\":null,\"would_do\":null}]} trailing";
        let err = DoctorV2::from_json(bad_json).unwrap_err();
        assert!(err.message.contains("trailing data"), "{}", err.message);
    }

    #[test]
    fn from_json_rejects_duplicate_key() {
        let bad_json = b"{\"schema\":\"chatgpt_fix.doctor.v2\",\"schema\":\"chatgpt_fix.doctor.v2\",\"read_only\":true,\"timestamp_utc\":\"t\",\"overall_status\":\"ok\",\"findings\":[{\"level\":\"ok\",\"code\":\"a\",\"message\":\"b\",\"evidence_json\":null,\"would_do\":null}]}";
        let err = DoctorV2::from_json(bad_json).unwrap_err();
        assert_eq!(err.code, "json_duplicate_key", "unexpected code: {}", err.code);
    }

    #[test]
    fn from_json_rejects_empty_findings() {
        let bad_json = b"{\"schema\":\"chatgpt_fix.doctor.v2\",\"read_only\":true,\"timestamp_utc\":\"t\",\"overall_status\":\"ok\",\"findings\":[]}";
        let err = DoctorV2::from_json(bad_json).unwrap_err();
        assert!(
            err.message.contains("at least one finding"),
            "{}",
            err.message
        );
    }
}