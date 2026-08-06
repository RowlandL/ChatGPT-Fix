use std::fs;
use std::path::{Path, PathBuf};

use chatgpt_fix_core::{ReceiptV1, SafeRelativePath, Sha256Digest, sha256_bytes};

const SOURCE_COMMIT: &str = "89543eb8c87d5f47a4b5563ea8c1d27d0674769d";
const TOOLCHAIN: &str = "rustc 1.97.1 (8bab26f4f 2026-07-14); x86_64-pc-windows-msvc";
const RELEASE_DIR: &str = "records/builds/0.4.0-win-x64";

const ARTIFACTS: [(&str, u64, &str); 3] = [
    (
        "ChatGPT-Fix-Launcher.exe",
        247_296,
        "cc28a185cc08096457a199af5069fc2d069b943243f4089a08414f2aa0a8751a",
    ),
    (
        "ChatGPT-Fix-Manager.exe",
        286_208,
        "42f0159e7edbdb82431e498cea53ca2ee79419bed4dd03ddd2396bdf9d26ba10",
    ),
    (
        "ChatGPT-Fix-Packer.exe",
        311_808,
        "311331b3748eea52c37d7af961913f3459ea52bcc730600351f405911d05b815",
    ),
];

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn tracked_build_evidence_is_exact_and_self_consistent() {
    let root = repository_root();
    let release_dir = root.join(RELEASE_DIR);
    let build_plan = fs::read(release_dir.join("build-plan.txt")).expect("read build plan");
    let build_plan_text = std::str::from_utf8(&build_plan).expect("build plan must be UTF-8");

    assert_eq!(
        build_plan_text,
        concat!(
            "schema=chatgpt_fix.build_plan.v1\n",
            "version=0.4.0\n",
            "target=x86_64-pc-windows-msvc\n",
            "profile=release\n",
            "source_commit=89543eb8c87d5f47a4b5563ea8c1d27d0674769d\n",
            "chatgpt_fix_source_commit=89543eb8c87d5f47a4b5563ea8c1d27d0674769d\n",
            "cargo_lock_sha256=d35112d2f419c8eb673d1f05603aa5b0546481c729165beac3b97033ddb14e56\n",
            "rustc=rustc 1.97.1 (8bab26f4f 2026-07-14)\n",
            "cargo=cargo 1.97.1 (c980f4866 2026-06-30)\n",
            "command=cargo build --offline --locked --workspace --release --target x86_64-pc-windows-msvc\n",
            "cargo_net_offline=true\n",
            "cargo_home=out/0.4.0/cargo-home\n",
            "cargo_target_dir=out/0.4.0/cargo-target\n",
            "artifact_dir=out/0.4.0/win-x64\n",
            "signing_status=unsigned\n",
        )
    );
    let plan_sha256 = sha256_bytes(&build_plan);

    let expected_checksums = ARTIFACTS
        .iter()
        .map(|(name, _, sha256)| format!("{sha256}  out/0.4.0/win-x64/{name}\n"))
        .collect::<String>();
    assert_eq!(
        fs::read_to_string(root.join("checksums/0.4.0-win-x64.sha256"))
            .expect("read release checksums"),
        expected_checksums
    );

    for (name, _, artifact_sha256) in ARTIFACTS {
        let receipt = ReceiptV1 {
            operation: "build".to_owned(),
            status: "success".to_owned(),
            plan_sha256: plan_sha256.clone(),
            artifact: Some(
                SafeRelativePath::parse(&format!("out/0.4.0/win-x64/{name}"))
                    .expect("artifact path must be safe"),
            ),
            artifact_sha256: Some(
                Sha256Digest::parse(artifact_sha256).expect("artifact hash must be valid"),
            ),
            source_commit: SOURCE_COMMIT.to_owned(),
            toolchain: TOOLCHAIN.to_owned(),
            signing_status: "unsigned".to_owned(),
        };
        receipt.validate().expect("receipt must validate");
        let expected = format!("{}\n", receipt.to_json().expect("receipt must serialize"));
        let actual = fs::read_to_string(release_dir.join(format!("{name}.receipt.json")))
            .expect("read artifact receipt");
        assert_eq!(actual, expected, "receipt for {name}");
    }

    let licenses = fs::read_to_string(release_dir.join("DEPENDENCY-LICENSES.txt"))
        .expect("read dependency licenses");
    assert_eq!(
        licenses.lines().collect::<Vec<_>>(),
        [
            "schema=chatgpt_fix.dependency_licenses.v1",
            "release=0.4.0",
            "source_commit=89543eb8c87d5f47a4b5563ea8c1d27d0674769d",
            "third_party_cargo_packages=0",
            "workspace_package=chatgpt-fix-core@0.4.0|license=NOASSERTION",
            "workspace_package=chatgpt-fix-launcher@0.4.0|license=NOASSERTION",
            "workspace_package=chatgpt-fix-manager@0.4.0|license=NOASSERTION",
            "workspace_package=chatgpt-fix-packer@0.4.0|license=NOASSERTION",
            "toolchain_component=rust-standard-library@1.97.1|license=Apache-2.0 OR MIT",
            "note=Cargo.lock contains only workspace packages; no third-party Cargo crate is redistributed.",
        ]
    );

    let sbom = fs::read_to_string(release_dir.join("sbom.spdx.json")).expect("read SPDX SBOM");
    assert!(sbom.contains(r#""spdxVersion": "SPDX-2.3""#));
    assert_eq!(sbom.matches(r#""SPDXID": "SPDXRef-Package-"#).count(), 5);
    assert!(sbom.contains(r#""name": "rust-standard-library""#));
    assert!(sbom.contains(r#""versionInfo": "1.97.1""#));
    assert!(sbom.contains(r#""licenseDeclared": "Apache-2.0 OR MIT""#));
    for (name, _, sha256) in ARTIFACTS {
        assert!(sbom.contains(name), "SBOM missing {name}");
        assert!(sbom.contains(sha256), "SBOM missing hash for {name}");
    }
}

#[test]
fn built_artifacts_match_evidence_when_artifact_dir_is_supplied() {
    let Some(directory) = std::env::var_os("CHATGPT_FIX_ARTIFACT_DIR") else {
        return;
    };
    let directory = Path::new(&directory);

    for (name, expected_bytes, expected_sha256) in ARTIFACTS {
        let bytes = fs::read(directory.join(name)).expect("read built artifact");
        assert_eq!(bytes.len() as u64, expected_bytes, "size for {name}");
        assert_eq!(
            sha256_bytes(&bytes).as_str(),
            expected_sha256,
            "SHA-256 for {name}"
        );
    }
}
