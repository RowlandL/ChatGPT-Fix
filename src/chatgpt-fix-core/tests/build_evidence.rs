use std::fs;
use std::path::{Path, PathBuf};

use chatgpt_fix_core::{ReceiptV1, SafeRelativePath, Sha256Digest, sha256_bytes};

const SOURCE_COMMIT: &str = "873b880f98640e40cc43baeb8d4ea1d0a78b0eac";
const TOOLCHAIN: &str = "rustc 1.97.1 (8bab26f4f 2026-07-14); x86_64-pc-windows-msvc";
const RELEASE_DIR: &str = "records/builds/0.4.0-win-x64";

const ARTIFACTS: [(&str, u64, &str); 3] = [
    (
        "ChatGPT-Fix-Launcher.exe",
        246_784,
        "8930bb38b06287c0fd08d77c5a137634ebf2ef8cc845e4ab68e6808eed86c16d",
    ),
    (
        "ChatGPT-Fix-Manager.exe",
        285_184,
        "409ca63b4cfdc4a4f026eecc3dd88e42fc4f9805ff70f469f40632ce403a7540",
    ),
    (
        "ChatGPT-Fix-Packer.exe",
        417_792,
        "840e92350d037ffacfcb979108a8fa3f2906f8f1bd03370a079b9502a8966da5",
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
            "source_commit=873b880f98640e40cc43baeb8d4ea1d0a78b0eac\n",
            "chatgpt_fix_source_commit=873b880f98640e40cc43baeb8d4ea1d0a78b0eac\n",
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
            "source_commit=873b880f98640e40cc43baeb8d4ea1d0a78b0eac",
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

// ---------------------------------------------------------------------------
// P3 build evidence (0.5.0)
// ---------------------------------------------------------------------------

const P3_SOURCE_COMMIT: &str = "ba3365e7cca15ae15a8d6f19427cdff12cd3aefd";
const P3_RELEASE_DIR: &str = "records/builds/0.5.0-win-x64";

const P3_ARTIFACTS: [(&str, u64, &str); 3] = [
    (
        "ChatGPT-Fix-Launcher.exe",
        246_784,
        "4235f8fb155af07266550bbcd6d4c7572bf99c06ebb07002a97fd9e7b5b90c05",
    ),
    (
        "ChatGPT-Fix-Manager.exe",
        284_672,
        "d4c3ac921375463134d7f3550f5bbb76b166930ed24a6aeab4ca941e5c81ebb2",
    ),
    (
        "ChatGPT-Fix-Packer.exe",
        497_664,
        "8d7669fce33a410028577d6c18c6315444453c81125f7db4ab49fc971173674b",
    ),
];

#[test]
fn p3_tracked_build_evidence_is_exact_and_self_consistent() {
    let root = repository_root();
    let release_dir = root.join(P3_RELEASE_DIR);
    let build_plan = fs::read(release_dir.join("build-plan.txt")).expect("read build plan");
    let build_plan_text = std::str::from_utf8(&build_plan).expect("build plan must be UTF-8");

    assert_eq!(
        build_plan_text,
        concat!(
            "schema=chatgpt_fix.build_plan.v1\n",
            "version=0.5.0\n",
            "target=x86_64-pc-windows-msvc\n",
            "profile=release\n",
            "source_commit=ba3365e7cca15ae15a8d6f19427cdff12cd3aefd\n",
            "chatgpt_fix_source_commit=ba3365e7cca15ae15a8d6f19427cdff12cd3aefd\n",
            "cargo_lock_sha256=8415fd1430c13ebff672d400b09dc412c2f2568ed8644f2c4e4beed2d17f2c79\n",
            "rustc=rustc 1.97.1 (8bab26f4f 2026-07-14)\n",
            "cargo=cargo 1.97.1 (c980f4866 2026-06-30)\n",
            "command=cargo build --offline --locked --workspace --release --target x86_64-pc-windows-msvc\n",
            "cargo_net_offline=true\n",
            "cargo_home=out/0.5.0/cargo-home\n",
            "cargo_target_dir=out/0.5.0/cargo-target\n",
            "artifact_dir=out/0.5.0/win-x64\n",
            "signing_status=unsigned\n",
        )
    );
    let plan_sha256 = sha256_bytes(&build_plan);

    let expected_checksums = P3_ARTIFACTS
        .iter()
        .map(|(name, _, sha256)| format!("{sha256}  out/0.5.0/win-x64/{name}\n"))
        .collect::<String>();
    assert_eq!(
        fs::read_to_string(root.join("checksums/0.5.0-win-x64.sha256"))
            .expect("read release checksums"),
        expected_checksums
    );

    for (name, _, artifact_sha256) in P3_ARTIFACTS {
        let receipt = ReceiptV1 {
            operation: "build".to_owned(),
            status: "success".to_owned(),
            plan_sha256: plan_sha256.clone(),
            artifact: Some(
                SafeRelativePath::parse(&format!("out/0.5.0/win-x64/{name}"))
                    .expect("artifact path must be safe"),
            ),
            artifact_sha256: Some(
                Sha256Digest::parse(artifact_sha256).expect("artifact hash must be valid"),
            ),
            source_commit: P3_SOURCE_COMMIT.to_owned(),
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
            "release=0.5.0",
            "source_commit=ba3365e7cca15ae15a8d6f19427cdff12cd3aefd",
            "third_party_cargo_packages=0",
            "workspace_package=chatgpt-fix-core@0.5.0|license=NOASSERTION",
            "workspace_package=chatgpt-fix-launcher@0.5.0|license=NOASSERTION",
            "workspace_package=chatgpt-fix-manager@0.5.0|license=NOASSERTION",
            "workspace_package=chatgpt-fix-packer@0.5.0|license=NOASSERTION",
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
    for (name, _, sha256) in P3_ARTIFACTS {
        assert!(sbom.contains(name), "SBOM missing {name}");
        assert!(sbom.contains(sha256), "SBOM missing hash for {name}");
    }
}

// ---------------------------------------------------------------------------
// P4 build evidence (0.6.0)
// ---------------------------------------------------------------------------

const P4_SOURCE_COMMIT: &str = "ddba3c16a0188a5a21c14cad5320d653a827d50f";
const P4_RELEASE_DIR: &str = "records/builds/0.6.0-win-x64";

const P4_ARTIFACTS: [(&str, u64, &str); 3] = [
    (
        "ChatGPT-Fix-Launcher.exe",
        246_784,
        "2cc357743588e36f306f9451f9e920ea7396e45305f40a2b905725c4bfde9f05",
    ),
    (
        "ChatGPT-Fix-Manager.exe",
        365_056,
        "0167b0b2ab5eaeb5eca5d4cf5032f41cfeecd63278abe8a35609f9fb105cfe26",
    ),
    (
        "ChatGPT-Fix-Packer.exe",
        496_640,
        "3a8774c88626b1d342e42ff97fbf04f885eb8cda655308f5dab2a26d2462a41c",
    ),
];

#[test]
fn p4_tracked_build_evidence_is_exact_and_self_consistent() {
    let root = repository_root();
    let release_dir = root.join(P4_RELEASE_DIR);
    let build_plan = fs::read(release_dir.join("build-plan.txt")).expect("read build plan");
    let build_plan_text = std::str::from_utf8(&build_plan).expect("build plan must be UTF-8");

    assert_eq!(
        build_plan_text,
        concat!(
            "schema=chatgpt_fix.build_plan.v1\n",
            "version=0.6.0\n",
            "target=x86_64-pc-windows-msvc\n",
            "profile=release\n",
            "source_commit=ddba3c16a0188a5a21c14cad5320d653a827d50f\n",
            "chatgpt_fix_source_commit=ddba3c16a0188a5a21c14cad5320d653a827d50f\n",
            "cargo_lock_sha256=2389d3f8f8bba976e1de3b71abf97b9168043b69bf42a094dec744bf187128d1\n",
            "rustc=rustc 1.97.1 (8bab26f4f 2026-07-14)\n",
            "cargo=cargo 1.97.1 (c980f4866 2026-06-30)\n",
            "command=cargo build --offline --locked --workspace --release --target x86_64-pc-windows-msvc\n",
            "cargo_net_offline=true\n",
            "cargo_home=out/0.6.0/cargo-home\n",
            "cargo_target_dir=out/0.6.0/cargo-target\n",
            "artifact_dir=out/0.6.0/win-x64\n",
            "signing_status=unsigned\n",
        )
    );
    let plan_sha256 = sha256_bytes(&build_plan);

    let expected_checksums = P4_ARTIFACTS
        .iter()
        .map(|(name, _, sha256)| format!("{sha256}  out/0.6.0/win-x64/{name}\n"))
        .collect::<String>();
    assert_eq!(
        fs::read_to_string(root.join("checksums/0.6.0-win-x64.sha256"))
            .expect("read release checksums"),
        expected_checksums
    );

    for (name, _, artifact_sha256) in P4_ARTIFACTS {
        let receipt = ReceiptV1 {
            operation: "build".to_owned(),
            status: "success".to_owned(),
            plan_sha256: plan_sha256.clone(),
            artifact: Some(
                SafeRelativePath::parse(&format!("out/0.6.0/win-x64/{name}"))
                    .expect("artifact path must be safe"),
            ),
            artifact_sha256: Some(
                Sha256Digest::parse(artifact_sha256).expect("artifact hash must be valid"),
            ),
            source_commit: P4_SOURCE_COMMIT.to_owned(),
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
            "release=0.6.0",
            "source_commit=ddba3c16a0188a5a21c14cad5320d653a827d50f",
            "third_party_cargo_packages=0",
            "workspace_package=chatgpt-fix-core@0.6.0|license=NOASSERTION",
            "workspace_package=chatgpt-fix-launcher@0.6.0|license=NOASSERTION",
            "workspace_package=chatgpt-fix-manager@0.6.0|license=NOASSERTION",
            "workspace_package=chatgpt-fix-packer@0.6.0|license=NOASSERTION",
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
    for (name, _, sha256) in P4_ARTIFACTS {
        assert!(sbom.contains(name), "SBOM missing {name}");
        assert!(sbom.contains(sha256), "SBOM missing hash for {name}");
    }
}
