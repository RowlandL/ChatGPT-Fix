use std::path::Path;
use std::process::Command;

use chatgpt_fix_core::LiveInspectionV1;

/// Path to the P2 worktree root (project root).
const WORKTREE_ROOT: &str = env!("CARGO_MANIFEST_DIR");

/// Resolve a path relative to the project root (two levels up from the
/// core crate manifest).
fn project_root() -> &'static Path {
    Path::new(WORKTREE_ROOT)
        .parent()
        .and_then(|p| p.parent())
        .expect("core crate is two levels deep")
}

/// Load and parse a fixture `probe-output.json`.
fn load_fixture(name: &str) -> LiveInspectionV1 {
    let path = project_root().join(format!(
        "src/chatgpt-fix-core/tests/live-fixtures/{}/probe-output.json",
        name
    ));
    let bytes =
        std::fs::read(&path).unwrap_or_else(|e| panic!("failed to read fixture {}: {}", name, e));
    LiveInspectionV1::from_json(&bytes)
        .unwrap_or_else(|e| panic!("failed to parse fixture {}: {}", name, e))
}

#[test]
fn all_six_fixtures_exist_and_parse() {
    for name in &[
        "valid-current",
        "publisher-mismatch",
        "dependency-missing",
        "shortcut-drift",
        "concurrent-update",
        "reparse-path",
    ] {
        let fixture = load_fixture(name);
        fixture
            .validate()
            .unwrap_or_else(|e| panic!("fixture {} failed validation: {}", name, e));
        assert_eq!(
            fixture.probe_source, "fixture",
            "fixture {} must have probe_source=fixture",
            name
        );
    }
}

#[test]
fn valid_current_has_correct_identity() {
    let fixture = load_fixture("valid-current");
    assert_eq!(fixture.identity_before, fixture.identity_after);
    assert_eq!(fixture.hash_before, fixture.hash_after);
    assert_eq!(fixture.publisher_id, "2p2nqsd0c76g0");
}

#[test]
fn publisher_mismatch_has_different_publisher() {
    let fixture = load_fixture("publisher-mismatch");
    assert_eq!(fixture.publisher, "CN=EvilCorp");
    assert_eq!(fixture.publisher_id, "deadbeef");
    assert_ne!(fixture.publisher_id, "2p2nqsd0c76g0");
}

#[test]
fn dependency_missing_has_unknown_dependency() {
    let fixture = load_fixture("dependency-missing");
    assert!(fixture.manifest_dependencies[0].starts_with("Missing.Dependency"));
}

#[test]
fn shortcut_drift_has_altered_shortcut_properties() {
    let fixture = load_fixture("shortcut-drift");
    assert_eq!(fixture.shortcut_target_path, "C:\\Malicious\\Codex.exe");
    assert_eq!(fixture.shortcut_arguments, "--inject");
}

#[test]
fn concurrent_update_has_different_identity_after() {
    let fixture = load_fixture("concurrent-update");
    assert_ne!(fixture.identity_before, fixture.identity_after);
    assert_ne!(fixture.hash_before, fixture.hash_after);
}

#[test]
fn reparse_path_has_install_location_in_virtual_store() {
    let fixture = load_fixture("reparse-path");
    assert!(fixture.install_location.contains("VirtualStore"));
}

fn probe_script_path() -> String {
    project_root()
        .join("scripts/chatgpt-fix-p2-probe.ps1")
        .to_str()
        .unwrap()
        .to_owned()
}

#[test]
fn probe_script_no_prohibited_cmdlets() {
    let script_path = probe_script_path();
    let script = std::fs::read_to_string(&script_path)
        .unwrap_or_else(|e| panic!("failed to read probe script: {}", e));

    // Prohibited cmdlet patterns (case-insensitive).
    let prohibited_patterns = [
        ("Add-", "mutation cmdlet"),
        ("Remove-", "mutation cmdlet"),
        ("Set-", "mutation cmdlet"),
        ("New-Item", "mutation cmdlet"),
        ("Remove-AppxPackage", "mutation cmdlet"),
        ("Add-AppxPackage", "mutation cmdlet"),
        ("Get-Process", "process cmdlet"),
        ("Stop-Process", "process cmdlet"),
        ("Get-CimInstance", "process cmdlet"),
        ("Get-WmiObject", "process cmdlet"),
        ("Invoke-WebRequest", "network cmdlet"),
        ("Invoke-RestMethod", "network cmdlet"),
        ("curl", "network cmdlet"),
        ("wget", "network cmdlet"),
        ("Get-AppxPackage -AllUsers", "prohibited AllUsers flag"),
        ("Get-AppxPackage -Name", "allowed (fixed package name)"),
    ];

    let lower = script.to_lowercase();

    // Verify that Get-AppxPackage -Name *is* present (required for live mode).
    assert!(
        lower.contains("get-appxpackage -name"),
        "probe script must contain Get-AppxPackage -Name"
    );

    for (pattern, description) in &prohibited_patterns {
        // Skip the allowed pattern and the -Name pattern (already checked).
        if *pattern == "Get-AppxPackage -Name" {
            continue;
        }
        let lower_pattern = pattern.to_lowercase();
        if lower.contains(&lower_pattern) {
            panic!(
                "probe script contains prohibited {}: '{}'",
                description, pattern
            );
        }
    }

    // Verify the script ends with the entry-point block.
    assert!(
        script.contains("Invoke-ChatGptFixP2Probe"),
        "probe script must define Invoke-ChatGptFixP2Probe function"
    );
    assert!(
        script.contains("ConvertTo-Json -Compress"),
        "probe script must output canonical JSON via ConvertTo-Json -Compress"
    );
}

#[test]
#[allow(clippy::needless_borrow)]
fn probe_script_ast_analysis_via_powershell() {
    let script_path = probe_script_path();

    // Use PowerShell's AST parser to check for prohibited cmdlets.
    let ast_check = r#"
param([string]$ScriptPath)
$ast = [System.Management.Automation.Language.Parser]::ParseFile($ScriptPath, [ref]$null, [ref]$null)
$prohibited = @(
    'Add-', 'Remove-', 'Set-', 'New-Item',
    'Remove-AppxPackage', 'Add-AppxPackage',
    'Get-Process', 'Stop-Process', 'Get-CimInstance', 'Get-WmiObject',
    'Invoke-WebRequest', 'Invoke-RestMethod'
)
$errors = @()
$ast.FindAll({ $true }, $true) | Where-Object { $_ -is [System.Management.Automation.Language.CommandAst] } | ForEach-Object {
    $cmdName = $_.GetCommandName()
    if ($cmdName) {
        foreach ($p in $prohibited) {
            if ($cmdName -like "$p*") {
                $errors += "Prohibited cmdlet: $cmdName at line $($_.Extent.StartLineNumber)"
            }
        }
        # Check Get-AppxPackage without -Name or with -AllUsers.
        if ($cmdName -eq 'Get-AppxPackage') {
            $hasName = $false
            $hasAllUsers = $false
            foreach ($param in $_.CommandElements) {
                if ($param -is [System.Management.Automation.Language.CommandParameterAst]) {
                    if ($param.ParameterName -eq 'Name') { $hasName = $true }
                    if ($param.ParameterName -eq 'AllUsers') { $hasAllUsers = $true }
                }
            }
            if (-not $hasName) {
                $errors += "Get-AppxPackage without -Name at line $($_.Extent.StartLineNumber)"
            }
            if ($hasAllUsers) {
                $errors += "Get-AppxPackage with -AllUsers at line $($_.Extent.StartLineNumber)"
            }
        }
    }
}
if ($errors.Count -gt 0) {
    $errors -join "`n"
    exit 1
} else {
    "AST_PASS"
    exit 0
}
"#;

    let output = Command::new("pwsh.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            &ast_check,
            &script_path,
        ])
        .output()
        .expect("failed to execute PowerShell AST check");

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "PowerShell AST check failed:\nstdout: {}\nstderr: {}",
        stdout,
        stderr
    );
    assert_eq!(stdout, "AST_PASS", "AST check: {}", stdout);
}

#[test]
fn fixture_mode_roundtrip_via_powershell() {
    // Test that Invoke-ChatGptFixP2Probe -Mode Fixture reads the fixture
    // and outputs valid JSON.
    let fixture_root =
        project_root().join("src/chatgpt-fix-core/tests/live-fixtures/valid-current");
    let fixture_root_str = fixture_root.to_str().unwrap().to_owned();
    let script_path = probe_script_path();

    let ps_script = format!(
        r#". '{}'; $result = Invoke-ChatGptFixP2Probe -Mode Fixture -FixtureRoot '{}'; $result | ConvertTo-Json -Compress -Depth 10"#,
        script_path.replace('\'', "''"),
        fixture_root_str.replace('\'', "''"),
    );

    let output = Command::new("pwsh.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            &ps_script,
        ])
        .output()
        .expect("failed to execute PowerShell fixture mode test");

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "PowerShell fixture mode failed:\nstdout: {}\nstderr: {}",
        stdout,
        stderr
    );

    // Parse the output as LiveInspectionV1.
    let parsed = LiveInspectionV1::from_json(stdout.as_bytes())
        .unwrap_or_else(|e| panic!("failed to parse fixture output: {}\nJSON: {}", e, stdout));
    parsed.validate().unwrap();
    assert_eq!(parsed.probe_source, "fixture");
    assert_eq!(
        parsed.package_full_name,
        "OpenAI.Codex_1.2.3.0_x64__2p2nqsd0c76g0"
    );
}
