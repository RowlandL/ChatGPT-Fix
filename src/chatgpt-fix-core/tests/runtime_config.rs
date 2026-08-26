use std::path::Path;

use chatgpt_fix_core::{
    preserve_desktop_section_at, sanitize_codex_config_text, sanitize_codex_config_text_for_home,
};

fn temp_dir(tag: &str) -> std::path::PathBuf {
    let dir =
        std::env::temp_dir().join(format!("chatgpt-fix-desktop-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

#[test]
fn desktop_section_snapshot_then_restore_when_dropped() {
    let dir = temp_dir("snapshot-restore");
    let config = dir.join("config.toml");
    let state = dir.join("state");
    let base = concat!(
        "model = \"gpt-5.6-sol\"\n",
        "\n",
        "[desktop]\n",
        "appearanceTheme = \"dark\"\n",
        "localeOverride = \"zh-CN\"\n",
    );
    std::fs::write(&config, base).expect("write config");

    // Section present: refresh the copy, no rewrite of config.toml.
    let changed = preserve_desktop_section_at(&config, &state).expect("snapshot");
    assert!(!changed);
    assert_eq!(std::fs::read_to_string(&config).expect("read"), base);
    assert!(state.join("desktop-config.toml").exists());

    // Simulate a config-manager rewrite that drops [desktop].
    let stripped = concat!(
        "model = \"gpt-5.6-sol\"\n",
        "\n",
        "[model_providers.custom]\n",
        "base_url = \"http://127.0.0.1:15721/v1\"\n",
    );
    std::fs::write(&config, stripped).expect("write stripped config");

    // Section lost: restore from the copy.
    let changed = preserve_desktop_section_at(&config, &state).expect("restore");
    assert!(changed);
    let restored = std::fs::read_to_string(&config).expect("read restored");
    assert!(restored.contains("model = \"gpt-5.6-sol\""));
    assert!(restored.contains("[model_providers.custom]"));
    assert!(restored.contains("[desktop]"));
    assert!(restored.contains("appearanceTheme = \"dark\""));
    assert!(restored.contains("localeOverride = \"zh-CN\""));
    // One-time backup of the stripped original.
    assert!(dir.join("config.toml.bak-chatgpt-fix-desktop").exists());

    // Idempotent: a further run only refreshes the copy.
    let changed = preserve_desktop_section_at(&config, &state).expect("refresh");
    assert!(!changed);
}

#[test]
fn desktop_section_live_value_wins_over_stale_copy() {
    let dir = temp_dir("live-wins");
    let config = dir.join("config.toml");
    let state = dir.join("state");
    std::fs::write(
        &config,
        concat!("[desktop]\n", "appearanceTheme = \"light\"\n"),
    )
    .expect("write config");
    preserve_desktop_section_at(&config, &state).expect("snapshot");

    // User changes the theme in the app; the app writes the new value at
    // exit. The next launcher run refreshes the copy from the live value.
    std::fs::write(
        &config,
        concat!(
            "[desktop]\n",
            "appearanceTheme = \"dark\"\n",
            "localeOverride = \"zh-CN\"\n"
        ),
    )
    .expect("write updated config");
    preserve_desktop_section_at(&config, &state).expect("refresh from live");

    // External rewrite drops the section again.
    std::fs::write(&config, "model = \"gpt-5.6-sol\"\n").expect("strip");
    preserve_desktop_section_at(&config, &state).expect("restore");

    let restored = std::fs::read_to_string(&config).expect("read");
    assert!(restored.contains("appearanceTheme = \"dark\""));
    assert!(restored.contains("localeOverride = \"zh-CN\""));
    assert!(!restored.contains("appearanceTheme = \"light\""));
}

#[test]
fn desktop_section_absent_without_copy_is_a_noop() {
    let dir = temp_dir("noop");
    let config = dir.join("config.toml");
    let state = dir.join("state");
    std::fs::write(&config, "model = \"gpt-5.6-sol\"\n").expect("write config");

    let changed = preserve_desktop_section_at(&config, &state).expect("noop");
    assert!(!changed);
    assert_eq!(
        std::fs::read_to_string(&config).expect("read"),
        "model = \"gpt-5.6-sol\"\n"
    );
}

#[test]
fn removes_reserved_bundled_marketplace_and_orphan_plugin_sections() {
    let input = concat!(
        "approval_policy = \"never\"\n",
        "\n",
        "[marketplaces.openai-bundled]\n",
        "source_type = \"local\"\n",
        "source = 'D:\\\\project\\\\bundled-marketplaces\\\\openai-bundled'\n",
        "\n",
        "[plugins.\"browser@openai-bundled\"]\n",
        "enabled = true\n",
        "\n",
        "[plugins.\"computer-use@openai-bundled\"]\n",
        "enabled = true\n",
    );

    let (output, changed) =
        sanitize_codex_config_text_for_home(input, Some(Path::new(r#"C:\Users\Alice\.codex"#)));

    assert!(changed);
    assert!(output.contains("approval_policy = \"never\""));
    // Reserved marketplace and its orphan plugin enablements must not remain:
    // the app-server drops bundled plugins whose marketplace is "configured
    // but never discovered", uninstalling them from the app.
    assert!(!output.contains("[marketplaces.openai-bundled]"));
    assert!(!output.contains("[plugins.\"browser@openai-bundled\"]"));
    assert!(!output.contains("[plugins.\"computer-use@openai-bundled\"]"));
    assert!(!output.contains("source_type"));
    assert!(!output.contains("source ="));
    assert!(!output.contains("D:\\\\project"));
    assert!(!output.contains("C:\\\\Users\\\\Alice"));
}

#[test]
fn keeps_non_bundled_plugin_enablements() {
    let input = concat!(
        "[plugins.\"github@openai-api-curated\"]\n",
        "enabled = true\n",
        "\n",
        "[plugins.\"ponytail@ponytail\"]\n",
        "enabled = true\n",
        "\n",
        "[plugins.\"browser@openai-bundled\"]\n",
        "enabled = true\n",
    );

    let (output, changed) = sanitize_codex_config_text(input);

    assert!(changed);
    // User/curated marketplaces are preserved untouched.
    assert!(output.contains("[plugins.\"github@openai-api-curated\"]"));
    assert!(output.contains("[plugins.\"ponytail@ponytail\"]"));
    // The reserved-marketplace orphan is removed.
    assert!(!output.contains("browser@openai-bundled"));
}

#[test]
fn reserved_marketplace_removal_is_idempotent() {
    let input = concat!(
        "[marketplaces.openai-bundled]\n",
        "source_type = \"local\"\n",
        "source = 'C:\\\\Users\\\\Alice\\\\.codex\\\\.tmp\\\\bundled-marketplaces\\\\openai-bundled'\n",
        "\n",
        "[plugins.\"browser@openai-bundled\"]\n",
        "enabled = true\n",
    );

    let (first, changed) = sanitize_codex_config_text(input);
    assert!(changed);
    assert!(!first.contains("marketplaces.openai-bundled"));
    assert!(!first.contains("browser@openai-bundled"));

    let (second, changed_again) = sanitize_codex_config_text(&first);
    assert!(!changed_again);
    assert_eq!(second, first);
}

#[test]
fn removes_reserved_marketplace_with_any_source_kind() {
    let input = concat!(
        "[marketplaces.openai-bundled]\n",
        "source_type = \"git\"\n",
        "source = \"https://example.invalid/openai-bundled\"\n",
        "\n",
        "[marketplaces.ponytail]\n",
        "source_type = \"local\"\n",
        "source = \"C:\\\\Users\\\\Alice\\\\.codex\\\\.tmp\\\\marketplaces\\\\ponytail\"\n",
    );

    let (output, changed) = sanitize_codex_config_text(input);

    assert!(changed);
    assert!(!output.contains("openai-bundled"));
    // Unrelated user marketplaces are preserved untouched.
    assert!(output.contains("[marketplaces.ponytail]"));
    assert!(output.contains("source_type = \"local\""));
    assert!(output.contains("ponytail"));
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
