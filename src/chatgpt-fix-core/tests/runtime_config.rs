use std::path::Path;

use chatgpt_fix_core::{sanitize_codex_config_text, sanitize_codex_config_text_for_home};

#[test]
fn normalizes_machine_bound_marketplace_source_without_removing_plugins() {
    let input = concat!(
        "approval_policy = \"never\"\n",
        "\n",
        "[marketplaces.openai-bundled]\n",
        "source_type = \"local\"\n",
        "source = 'D:\\\\project\\\\bundled-marketplaces\\\\openai-bundled'\n",
        "\n",
        "[plugins.\"browser@openai-bundled\"]\n",
        "enabled = true\n",
    );

    let (output, changed) = sanitize_codex_config_text_for_home(
        input,
        Some(Path::new(r#"C:\Users\Alice\.codex"#)),
    );

    assert!(changed);
    assert!(output.contains("approval_policy = \"never\""));
    assert!(output.contains("[plugins.\"browser@openai-bundled\"]"));
    assert!(output.contains("[marketplaces.openai-bundled]"));
    assert!(output.contains("[plugins.\"browser@openai-bundled\"]"));
    assert!(output.contains("C:\\\\Users\\\\Alice\\\\.codex\\\\.tmp\\\\bundled-marketplaces\\\\openai-bundled"));
    assert!(!output.contains("D:\\\\project"));
}

#[test]
fn upgrades_legacy_low_windows_sandbox_without_touching_full_access_mode() {
    let input = concat!(
        "approval_policy = \"never\"\n",
        "sandbox_mode = \"danger-full-access\"\n",
        "\n",
        "[windows]\n",
        "sandbox = \"low\"\n",
    );

    let (output, changed) = sanitize_codex_config_text(input);

    assert!(changed);
    assert!(output.contains("approval_policy = \"never\""));
    assert!(output.contains("sandbox_mode = \"danger-full-access\""));
    assert!(output.contains("sandbox = \"elevated\""));
    assert!(!output.contains("sandbox = \"low\""));
}

#[test]
fn leaves_portable_configuration_unchanged() {
    let input = concat!(
        "approval_policy = \"never\"\n",
        "sandbox_mode = \"danger-full-access\"\n",
        "\n",
        "[windows]\n",
        "sandbox = \"elevated\"\n",
    );

    let (output, changed) = sanitize_codex_config_text(input);

    assert!(!changed);
    assert_eq!(output, input);
}
