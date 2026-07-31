use std::error::Error;

use chatgpt_fix_core::{
    BASELINE_SCHEMA, BaselineV2, ContractError, LAUNCH_SCHEMA, LaunchV1, PLAN_SCHEMA, PlanAction,
    PlanActionKind, PlanDecision, PlanV1, RECEIPT_SCHEMA, ReceiptV1, SafeRelativePath,
    Sha256Digest,
};

fn path(value: &str) -> SafeRelativePath {
    SafeRelativePath::parse(value).expect("test path must be valid")
}

fn digest(value: char) -> Sha256Digest {
    Sha256Digest::parse(&value.to_string().repeat(64)).expect("test digest must be valid")
}

fn baseline() -> BaselineV2 {
    BaselineV2 {
        baseline_id: "baseline-001".to_owned(),
        package_full_name: "OpenAI.ChatGPT_1.2.3.0_x64__publisher".to_owned(),
        version: "1.2.3.0".to_owned(),
        architecture: "x64".to_owned(),
        publisher: "CN=OpenAI".to_owned(),
        source: path("packages/ChatGPT"),
        bytes: 42,
        sha256: digest('a'),
    }
}

fn ready_plan() -> PlanV1 {
    PlanV1 {
        fixture_id: "fixture-ready".to_owned(),
        decision: PlanDecision::Ready,
        baseline: Some(baseline()),
        actions: vec![PlanAction {
            kind: PlanActionKind::WouldCopy,
            target: path("staging/ChatGPT.exe"),
            execute: false,
        }],
        errors: Vec::new(),
    }
}

fn receipt() -> ReceiptV1 {
    ReceiptV1 {
        operation: "plan".to_owned(),
        status: "ok".to_owned(),
        plan_sha256: digest('b'),
        artifact: Some(path("records/plan.json")),
        artifact_sha256: Some(digest('c')),
        source_commit: "e3dc189".to_owned(),
        toolchain: "rustc-test".to_owned(),
        signing_status: "unsigned".to_owned(),
    }
}

fn assert_contract_error(error: &ContractError, expected: (&str, &str, &str)) {
    let (code, field, message) = expected;
    assert_eq!(error.code.as_str(), code);
    assert_eq!(error.field.as_str(), field);
    assert_eq!(error.message.as_str(), message);
    assert_eq!(error.to_string(), format!("{code} at {field}: {message}"));
    let _: &(dyn Error + 'static) = error;
}

#[test]
fn schema_constants_are_frozen() {
    assert_eq!(PLAN_SCHEMA, "chatgpt_fix.plan.v1");
    assert_eq!(BASELINE_SCHEMA, "chatgpt_fix.baseline.v2");
    assert_eq!(LAUNCH_SCHEMA, "chatgpt_fix.launch.v1");
    assert_eq!(RECEIPT_SCHEMA, "chatgpt_fix.receipt.v1");
}

#[test]
fn safe_relative_path_accepts_canonical_forward_slash_paths() {
    for value in ["ChatGPT.exe", "bin/ChatGPT.exe", "unicode/文件.txt"] {
        let parsed = SafeRelativePath::parse(value).unwrap();
        assert_eq!(parsed.as_str(), value);
        assert_eq!(parsed.to_string(), value);
    }
}

#[test]
fn safe_relative_path_rejects_unsafe_or_noncanonical_paths() {
    let invalid = [
        ("", "must not be empty"),
        ("/absolute", "must not start with '/'"),
        (r"dir\file", "must use forward slashes only"),
        ("C:/absolute", "must not contain ':'"),
        ("dir//file", "must not contain empty segments"),
        ("dir/", "must not contain empty segments"),
        (".", "must not contain '.' or '..' segments"),
        ("..", "must not contain '.' or '..' segments"),
        ("dir/./file", "must not contain '.' or '..' segments"),
        ("dir/../file", "must not contain '.' or '..' segments"),
        ("nul\0byte", "must not contain control characters"),
        ("control\u{001f}byte", "must not contain control characters"),
    ];

    for (value, message) in invalid {
        let error = SafeRelativePath::parse(value).unwrap_err();
        assert_contract_error(&error, ("invalid_relative_path", "path", message));
    }
}

#[test]
fn sha256_digest_normalizes_ascii_hex_to_lowercase() {
    let uppercase = "ABCDEF0123456789".repeat(4);
    let parsed = Sha256Digest::parse(&uppercase).unwrap();
    assert_eq!(parsed.as_str(), uppercase.to_ascii_lowercase());
}

#[test]
fn sha256_digest_rejects_wrong_length_non_hex_and_non_ascii() {
    for value in [
        "a".repeat(63),
        "a".repeat(65),
        format!("{}g", "a".repeat(63)),
        "é".repeat(64),
    ] {
        let error = Sha256Digest::parse(&value).unwrap_err();
        assert_contract_error(
            &error,
            (
                "invalid_sha256",
                "sha256",
                "must contain exactly 64 ASCII hexadecimal characters",
            ),
        );
    }
}

#[test]
fn plan_action_validation_accepts_a_safe_target() {
    PlanAction {
        kind: PlanActionKind::WouldWrite,
        target: path("actions/state.json"),
        execute: false,
    }
    .validate()
    .unwrap();
}

#[test]
fn plan_action_json_is_exact_compact_and_escapes_a_quoted_target() {
    let action = PlanAction {
        kind: PlanActionKind::WouldWrite,
        target: path("actions/quoted\"name"),
        execute: true,
    };

    assert_eq!(
        action.to_json(),
        r#"{"kind":"would-write","target":"actions/quoted\"name","execute":true}"#
    );
}

#[test]
fn baseline_validation_accepts_valid_values() {
    baseline().validate().unwrap();
}

#[test]
fn baseline_validation_rejects_empty_control_text_and_zero_bytes() {
    let mut value = baseline();
    value.baseline_id.clear();
    assert_contract_error(
        &value.validate().unwrap_err(),
        ("empty_field", "baseline_id", "must not be empty"),
    );

    let mut value = baseline();
    value.publisher = "CN=OpenAI\nInjected".to_owned();
    assert_contract_error(
        &value.validate().unwrap_err(),
        (
            "control_character",
            "publisher",
            "must not contain control characters",
        ),
    );

    let mut value = baseline();
    value.bytes = 0;
    assert_contract_error(
        &value.validate().unwrap_err(),
        ("invalid_value", "bytes", "must be greater than zero"),
    );
}

#[test]
fn plan_validation_accepts_ready_and_rejected_shapes() {
    ready_plan().validate().unwrap();

    PlanV1 {
        fixture_id: "fixture-rejected".to_owned(),
        decision: PlanDecision::Rejected,
        baseline: None,
        actions: Vec::new(),
        errors: vec!["publisher rejected".to_owned()],
    }
    .validate()
    .unwrap();
}

#[test]
fn plan_validation_rejects_invalid_ready_and_rejected_shapes() {
    let mut ready = ready_plan();
    ready.baseline = None;
    assert_contract_error(
        &ready.validate().unwrap_err(),
        (
            "invariant_violation",
            "baseline",
            "ready plans require a baseline",
        ),
    );

    let mut ready = ready_plan();
    ready.actions.clear();
    assert_contract_error(
        &ready.validate().unwrap_err(),
        (
            "invariant_violation",
            "actions",
            "ready plans require at least one action",
        ),
    );

    let mut ready = ready_plan();
    ready.errors.push("unexpected".to_owned());
    assert_contract_error(
        &ready.validate().unwrap_err(),
        (
            "invariant_violation",
            "errors",
            "ready plans must not contain errors",
        ),
    );

    let mut ready = ready_plan();
    ready.baseline.as_mut().unwrap().bytes = 0;
    assert_contract_error(
        &ready.validate().unwrap_err(),
        ("invalid_value", "bytes", "must be greater than zero"),
    );

    let mut rejected = PlanV1 {
        fixture_id: "fixture-rejected".to_owned(),
        decision: PlanDecision::Rejected,
        baseline: None,
        actions: Vec::new(),
        errors: vec!["rejected".to_owned()],
    };
    rejected.baseline = Some(baseline());
    assert_contract_error(
        &rejected.validate().unwrap_err(),
        (
            "invariant_violation",
            "baseline",
            "rejected plans must not contain a baseline",
        ),
    );

    rejected.baseline = None;
    rejected.actions.push(PlanAction {
        kind: PlanActionKind::WouldWrite,
        target: path("state/current.json"),
        execute: false,
    });
    assert_contract_error(
        &rejected.validate().unwrap_err(),
        (
            "invariant_violation",
            "actions",
            "rejected plans must not contain actions",
        ),
    );

    rejected.actions.clear();
    rejected.errors.clear();
    assert_contract_error(
        &rejected.validate().unwrap_err(),
        (
            "invariant_violation",
            "errors",
            "rejected plans require at least one error",
        ),
    );

    rejected.errors = vec![String::new()];
    assert_contract_error(
        &rejected.validate().unwrap_err(),
        ("empty_field", "errors", "must not be empty"),
    );

    rejected.errors = vec!["rejected\nreason".to_owned()];
    assert_contract_error(
        &rejected.validate().unwrap_err(),
        (
            "control_character",
            "errors",
            "must not contain control characters",
        ),
    );
}

#[test]
fn plan_validation_rejects_duplicate_action_kinds() {
    let mut value = ready_plan();
    value.actions.push(PlanAction {
        kind: PlanActionKind::WouldCopy,
        target: path("staging/second.exe"),
        execute: false,
    });

    let error = value.validate().unwrap_err();
    assert_contract_error(
        &error,
        (
            "duplicate_action_kind",
            "actions",
            "action kinds must be unique",
        ),
    );
}

#[test]
fn launch_validation_accepts_both_reason_shapes() {
    LaunchV1 {
        launch_id: "launch-001".to_owned(),
        generation: 1,
        baseline_id: path("baselines/baseline-001"),
        executable: path("app/ChatGPT.exe"),
        would_start: true,
        reason: None,
    }
    .validate()
    .unwrap();

    LaunchV1 {
        launch_id: "launch-002".to_owned(),
        generation: 2,
        baseline_id: path("baselines/baseline-002"),
        executable: path("app/ChatGPT.exe"),
        would_start: false,
        reason: Some("dry-run only".to_owned()),
    }
    .validate()
    .unwrap();
}

#[test]
fn launch_validation_rejects_generation_and_reason_mismatches() {
    let mut value = LaunchV1 {
        launch_id: "launch-001".to_owned(),
        generation: 0,
        baseline_id: path("baselines/baseline-001"),
        executable: path("app/ChatGPT.exe"),
        would_start: true,
        reason: None,
    };
    assert_contract_error(
        &value.validate().unwrap_err(),
        ("invalid_value", "generation", "must be greater than zero"),
    );

    value.generation = 1;
    value.reason = Some("must be absent".to_owned());
    assert_contract_error(
        &value.validate().unwrap_err(),
        (
            "invariant_violation",
            "reason",
            "must be absent when would_start is true",
        ),
    );

    value.would_start = false;
    value.reason = None;
    assert_contract_error(
        &value.validate().unwrap_err(),
        (
            "invariant_violation",
            "reason",
            "must be present when would_start is false",
        ),
    );

    value.reason = Some(String::new());
    assert_contract_error(
        &value.validate().unwrap_err(),
        ("empty_field", "reason", "must not be empty"),
    );
}

#[test]
fn receipt_validation_accepts_paired_or_absent_artifacts() {
    receipt().validate().unwrap();

    let mut value = receipt();
    value.artifact = None;
    value.artifact_sha256 = None;
    value.validate().unwrap();
}

#[test]
fn receipt_validation_rejects_artifact_pairing_and_empty_text() {
    let mut value = receipt();
    value.artifact_sha256 = None;
    assert_contract_error(
        &value.validate().unwrap_err(),
        (
            "invariant_violation",
            "artifact",
            "artifact and artifact_sha256 must both be present or both be absent",
        ),
    );

    let mut value = receipt();
    value.artifact = None;
    assert_contract_error(
        &value.validate().unwrap_err(),
        (
            "invariant_violation",
            "artifact",
            "artifact and artifact_sha256 must both be present or both be absent",
        ),
    );

    let mut value = receipt();
    value.signing_status.clear();
    assert_contract_error(
        &value.validate().unwrap_err(),
        ("empty_field", "signing_status", "must not be empty"),
    );
}

#[test]
fn baseline_json_is_exact_compact_and_canonical() {
    assert_eq!(
        baseline().to_json().unwrap(),
        concat!(
            r#"{"schema":"chatgpt_fix.baseline.v2","baseline_id":"baseline-001","package_full_name":"OpenAI.ChatGPT_1.2.3.0_x64__publisher","version":"1.2.3.0","architecture":"x64","publisher":"CN=OpenAI","source":"packages/ChatGPT","bytes":42,"sha256":""#,
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            r#""}"#,
        )
    );
}

#[test]
fn plan_json_is_exact_and_uses_frozen_action_values() {
    let value = PlanV1 {
        fixture_id: "fixture-all-actions".to_owned(),
        decision: PlanDecision::Ready,
        baseline: Some(baseline()),
        actions: vec![
            PlanAction {
                kind: PlanActionKind::WouldCopy,
                target: path("actions/copy"),
                execute: false,
            },
            PlanAction {
                kind: PlanActionKind::WouldWrite,
                target: path("actions/write"),
                execute: false,
            },
            PlanAction {
                kind: PlanActionKind::WouldSwitch,
                target: path("actions/switch"),
                execute: false,
            },
            PlanAction {
                kind: PlanActionKind::WouldStart,
                target: path("actions/start"),
                execute: false,
            },
            PlanAction {
                kind: PlanActionKind::WouldTerminate,
                target: path("actions/terminate"),
                execute: false,
            },
        ],
        errors: Vec::new(),
    };

    assert_eq!(
        value.to_json().unwrap(),
        concat!(
            r#"{"schema":"chatgpt_fix.plan.v1","fixture_id":"fixture-all-actions","decision":"ready","baseline":{"schema":"chatgpt_fix.baseline.v2","baseline_id":"baseline-001","package_full_name":"OpenAI.ChatGPT_1.2.3.0_x64__publisher","version":"1.2.3.0","architecture":"x64","publisher":"CN=OpenAI","source":"packages/ChatGPT","bytes":42,"sha256":""#,
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            r#""},"actions":[{"kind":"would-copy","target":"actions/copy","execute":false},{"kind":"would-write","target":"actions/write","execute":false},{"kind":"would-switch","target":"actions/switch","execute":false},{"kind":"would-start","target":"actions/start","execute":false},{"kind":"would-terminate","target":"actions/terminate","execute":false}],"errors":[]}"#,
        )
    );
}

#[test]
fn launch_json_is_exact_compact_and_canonical() {
    let value = LaunchV1 {
        launch_id: "launch-001".to_owned(),
        generation: 7,
        baseline_id: path("baselines/baseline-001"),
        executable: path("app/ChatGPT.exe"),
        would_start: false,
        reason: Some("offline fixture".to_owned()),
    };

    assert_eq!(
        value.to_json().unwrap(),
        r#"{"schema":"chatgpt_fix.launch.v1","launch_id":"launch-001","generation":7,"baseline_id":"baselines/baseline-001","executable":"app/ChatGPT.exe","would_start":false,"reason":"offline fixture"}"#
    );
}

#[test]
fn receipt_json_is_exact_compact_and_canonical() {
    assert_eq!(
        receipt().to_json().unwrap(),
        concat!(
            r#"{"schema":"chatgpt_fix.receipt.v1","operation":"plan","status":"ok","plan_sha256":""#,
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            r#"","artifact":"records/plan.json","artifact_sha256":""#,
            "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
            r#"","source_commit":"e3dc189","toolchain":"rustc-test","signing_status":"unsigned"}"#,
        )
    );
}

#[test]
fn json_escaping_handles_all_required_escapes_and_preserves_unicode() {
    let mut value = baseline();
    value.baseline_id =
        "quote\" slash\\ b\u{0008} f\u{000c} n\n r\r t\t nul\u{0000} unit\u{001f} 雪".to_owned();

    let json = value.to_json().unwrap();
    assert!(json.contains(
        r#""baseline_id":"quote\" slash\\ b\b f\f n\n r\r t\t nul\u0000 unit\u001f 雪""#
    ));
    assert!(!json.contains('\n'));
    assert!(!json.contains('\r'));
}

#[test]
fn validation_and_serialization_are_separate_contract_operations() {
    let mut value = ready_plan();
    value.fixture_id.clear();
    assert_contract_error(
        &value.validate().unwrap_err(),
        ("empty_field", "fixture_id", "must not be empty"),
    );
    assert!(
        value
            .to_json()
            .unwrap()
            .starts_with(r#"{"schema":"chatgpt_fix.plan.v1","fixture_id":"""#)
    );
}
