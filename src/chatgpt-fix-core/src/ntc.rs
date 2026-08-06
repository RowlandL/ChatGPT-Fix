use std::fs;
use std::io::Read;
use std::path::Path;
use std::process::Command;

use crate::json::write_string;
use crate::{ContractError, NtcReapplyState};

/// File name of the NTC health receipt inside a launcher-owned baseline dir.
const NTC_HEALTH_FILE: &str = "ntc-health.json";

/// Probe the native token-cost helper over its loopback health endpoint.
///
/// Returns a JSON health receipt (`chatgpt_fix.ntc_health.v1`) with the real
/// HTTP status, helper source, and the Scheduled Task state. P8 scope: this
/// is read-only — it never starts/stops the task and never writes `.codex`.
pub fn ntc_health_check() -> Result<NtcHealthV1, ContractError> {
    // 1. HTTP health probe to the helper loopback endpoint.
    let url = "http://127.0.0.1:17888/health";
    let (reachable, body) = match fetch_http() {
        Ok(body) => (true, body),
        Err(error) => (false, error),
    };
    let healthy = reachable && body.contains("\"ok\":true");

    // 2. Scheduled Task state (read-only query).
    let task_state = match Command::new("schtasks")
        .args(["/Query", "/TN", "CodexTokenCostHelper", "/FO", "LIST", "/V"])
        .output()
    {
        Ok(output) if output.status.success() => {
            let text = String::from_utf8_lossy(&output.stdout).to_string();
            if text.contains("就绪") || text.contains("Ready") {
                "ready".to_owned()
            } else if text.contains("正在运行") || text.contains("Running") {
                "running".to_owned()
            } else {
                "unknown".to_owned()
            }
        }
        _ => "missing".to_owned(),
    };

    let health = NtcHealthV1 {
        helper_url: url.to_owned(),
        reachable,
        healthy,
        body: body.chars().take(256).collect(),
        task_state,
        reapply_state: if healthy {
            NtcReapplyState::Clean
        } else {
            NtcReapplyState::NeedsReapply
        },
        created_at_utc: crate::utc_now_rfc3339(),
    };
    health.validate()?;
    Ok(health)
}

/// Persist a health receipt into a baseline root (ntc-health.json).
pub fn write_ntc_health(baseline_root: &Path, health: &NtcHealthV1) -> Result<(), ContractError> {
    let json = health.to_json()?;
    let path = baseline_root.join(NTC_HEALTH_FILE);
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, json).map_err(|error| {
        ContractError::new(
            "ntc_health_write",
            "ntc-health.json",
            format!("cannot write health receipt: {error}"),
        )
    })?;
    fs::rename(&tmp, &path).map_err(|error| {
        ContractError::new(
            "ntc_health_write",
            "ntc-health.json",
            format!("cannot finalize health receipt: {error}"),
        )
    })?;
    Ok(())
}

/// Minimal HTTP GET via TcpStream (no external crate; std-only crate).
fn fetch_http() -> Result<String, String> {
    use std::net::TcpStream;
    use std::time::Duration;

    let host = "127.0.0.1";
    let port = 17888;
    let mut stream =
        TcpStream::connect((host, port)).map_err(|e| format!("connect failed: {e}"))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .map_err(|e| format!("timeout set failed: {e}"))?;

    let request =
        format!("GET /health HTTP/1.1\r\nHost: {host}:{port}\r\nConnection: close\r\n\r\n");
    use std::io::Write;
    stream
        .write_all(request.as_bytes())
        .map_err(|e| format!("write failed: {e}"))?;

    let mut response = Vec::new();
    stream
        .read_to_end(&mut response)
        .map_err(|e| format!("read failed: {e}"))?;
    let text = String::from_utf8_lossy(&response).to_string();
    // Body after the blank line.
    match text.find("\r\n\r\n") {
        Some(idx) => Ok(text[idx + 4..].to_owned()),
        None => Ok(text),
    }
}

// ---------------------------------------------------------------------------
// NtcHealthV1 schema
// ---------------------------------------------------------------------------

pub const NTC_HEALTH_SCHEMA: &str = "chatgpt_fix.ntc_health.v1";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NtcHealthV1 {
    pub helper_url: String,
    pub reachable: bool,
    pub healthy: bool,
    pub body: String,
    pub task_state: String,
    pub reapply_state: NtcReapplyState,
    pub created_at_utc: String,
}

impl NtcHealthV1 {
    pub fn validate(&self) -> Result<(), ContractError> {
        if self.helper_url.is_empty() {
            return Err(ContractError::new(
                "invalid_value",
                "helper_url",
                "must not be empty",
            ));
        }
        if self.created_at_utc.is_empty() {
            return Err(ContractError::new(
                "invalid_value",
                "created_at_utc",
                "must not be empty",
            ));
        }
        Ok(())
    }

    pub fn to_json(&self) -> Result<String, ContractError> {
        let mut output = String::new();
        output.push_str("{\"schema\":");
        write_string(&mut output, NTC_HEALTH_SCHEMA);
        output.push_str(",\"helper_url\":");
        write_string(&mut output, &self.helper_url);
        output.push_str(",\"reachable\":");
        output.push_str(if self.reachable { "true" } else { "false" });
        output.push_str(",\"healthy\":");
        output.push_str(if self.healthy { "true" } else { "false" });
        output.push_str(",\"body\":");
        write_string(&mut output, &self.body);
        output.push_str(",\"task_state\":");
        write_string(&mut output, &self.task_state);
        output.push_str(",\"reapply_state\":");
        write_string(&mut output, self.reapply_state.as_json_str());
        output.push_str(",\"created_at_utc\":");
        write_string(&mut output, &self.created_at_utc);
        output.push('}');
        Ok(output)
    }
}

/// Parse the `chatgpt_fix.ntc_inject_receipt.v1` JSON emitted by the Node
/// inject script, returning `(before_sha256, after_sha256)`.
pub fn json_parse_ntc_receipt(output: &str) -> Option<(String, String)> {
    use crate::json::JsonParser;
    let parsed = JsonParser::new(output.as_bytes())
        .ok()?
        .parse_top_level()
        .ok()?;
    let obj = parsed.as_object().ok()?;
    let _ = obj;
    let before = parsed
        .field("before_sha256")
        .ok()?
        .as_str()
        .ok()?
        .to_owned();
    let after = parsed.field("after_sha256").ok()?.as_str().ok()?.to_owned();
    Some((before, after))
}
