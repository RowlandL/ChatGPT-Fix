use chatgpt_fix_core::{
    NtcReapplyState, json_parse_ntc_receipt, ntc_health_check, write_ntc_health,
};

#[test]
fn ntc_receipt_parses_before_after_hashes() {
    let receipt = r#"{"schema":"chatgpt_fix.ntc_inject_receipt.v1","ok":true,"before_sha256":"AAAA","after_sha256":"BBBB","artifact":"app.asar"}"#;
    let (before, after) = json_parse_ntc_receipt(receipt).expect("receipt must parse");
    assert_eq!(before, "AAAA");
    assert_eq!(after, "BBBB");
}

#[test]
fn ntc_receipt_rejects_garbage() {
    assert!(json_parse_ntc_receipt("not json").is_none());
    assert!(json_parse_ntc_receipt(r#"{"ok":true}"#).is_none());
}

#[test]
fn ntc_health_check_returns_receipt() {
    // The health probe runs against the real loopback endpoint; when the
    // helper is absent it must still return a receipt (not fail).
    let health = ntc_health_check().expect("health check must produce a receipt");
    assert!(health.created_at_utc.contains('T'), "timestamp present");
    assert!(health.helper_url.contains("17888"));
    // reapply_state must be consistent with healthy.
    if health.healthy {
        assert_eq!(health.reapply_state, NtcReapplyState::Clean);
    } else {
        assert_eq!(health.reapply_state, NtcReapplyState::NeedsReapply);
    }
}

#[test]
fn ntc_health_roundtrips_through_json() {
    let health = ntc_health_check().expect("health check");
    let json = health.to_json().expect("to_json");
    assert!(json.contains("chatgpt_fix.ntc_health.v1"));
    assert!(json.contains("\"healthy\""));
}

#[test]
fn write_ntc_health_persists_receipt() {
    let dir = std::env::temp_dir().join(format!("chatgpt-fix-ntc-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create dir");
    let health = ntc_health_check().expect("health check");
    write_ntc_health(&dir, &health).expect("write health");
    let stored = std::fs::read_to_string(dir.join("ntc-health.json")).expect("read back");
    assert!(stored.contains("chatgpt_fix.ntc_health.v1"));
    let _ = std::fs::remove_dir_all(&dir);
}
