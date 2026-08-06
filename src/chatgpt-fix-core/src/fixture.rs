use std::cmp::Reverse;
use std::fmt;
use std::fs::{self, Metadata};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use crate::sha256::sha256_file;
use crate::{
    BaselineV2, PlanAction, PlanActionKind, PlanDecision, PlanV1, SafeRelativePath, Sha256Digest,
};

pub const FIXTURE_SCHEMA: &str = "chatgpt_fix.fixture.v1";

const FIXTURE_MARKER: &str = "chatgpt-fix.fixture";
const EXPECTED_ARCHITECTURE: &str = "x64";
const EXPECTED_PUBLISHER: &str = "CN=OpenAI";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FixtureError {
    pub code: String,
    pub path: PathBuf,
    pub message: String,
}

impl FixtureError {
    pub(crate) fn new(code: &'static str, path: impl Into<PathBuf>, message: impl Into<String>) -> Self {
        Self {
            code: code.to_owned(),
            path: path.into(),
            message: message.into(),
        }
    }
}

impl fmt::Display for FixtureError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{} at {}: {}",
            self.code,
            self.path.display(),
            self.message
        )
    }
}

impl std::error::Error for FixtureError {}

pub fn plan_fixture(root: &Path) -> Result<PlanV1, FixtureError> {
    let root_metadata = fs::symlink_metadata(root).map_err(|error| {
        FixtureError::new(
            "fixture_root_unreadable",
            root,
            format!("fixture root cannot be read: {error}"),
        )
    })?;
    if is_reparse_point(&root_metadata) {
        return Err(FixtureError::new(
            "reparse_path",
            root,
            "fixture root must not be a reparse point or symbolic link",
        ));
    }
    if !root_metadata.is_dir() {
        return Err(FixtureError::new(
            "fixture_root_not_directory",
            root,
            "fixture root must be a directory",
        ));
    }

    let marker = root.join(FIXTURE_MARKER);
    let marker_metadata = match fs::symlink_metadata(&marker) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == ErrorKind::NotFound => {
            return Err(FixtureError::new(
                "not_fixture_root",
                &marker,
                "fixture marker is missing",
            ));
        }
        Err(error) => {
            return Err(FixtureError::new(
                "fixture_marker_unreadable",
                &marker,
                format!("fixture marker cannot be read: {error}"),
            ));
        }
    };
    if is_reparse_point(&marker_metadata) {
        return Err(FixtureError::new(
            "reparse_path",
            &marker,
            "fixture marker must not be a reparse point or symbolic link",
        ));
    }
    if !marker_metadata.is_file() {
        return Err(FixtureError::new(
            "fixture_marker_not_file",
            &marker,
            "fixture marker must be a regular file",
        ));
    }

    let marker_bytes = fs::read(&marker).map_err(|error| {
        FixtureError::new(
            "fixture_marker_unreadable",
            &marker,
            format!("fixture marker cannot be read: {error}"),
        )
    })?;
    let marker_text = String::from_utf8(marker_bytes).map_err(|error| {
        FixtureError::new(
            "fixture_invalid_utf8",
            &marker,
            format!("fixture marker must be UTF-8: {error}"),
        )
    })?;
    let mut fixture = ParsedFixture::parse(&marker_text, &marker)?;

    if fixture.root_kind == RootKind::Reparse {
        return validated_plan(rejected_plan(&fixture.fixture_id, "reparse_path"), &marker);
    }
    if fixture.manifest_state == ManifestState::Corrupt {
        return validated_plan(
            rejected_plan(&fixture.fixture_id, "corrupt_manifest"),
            &marker,
        );
    }

    let canonical_root = fs::canonicalize(root).map_err(|error| {
        FixtureError::new(
            "fixture_root_unreadable",
            root,
            format!("fixture root cannot be canonicalized: {error}"),
        )
    })?;
    fixture
        .candidates
        .sort_by_key(|candidate| Reverse(candidate.version_parts));

    let mut first_rejection = None;
    for candidate in &fixture.candidates {
        match validate_candidate(root, &canonical_root, fixture.available_bytes, candidate) {
            Ok(baseline) => {
                let plan = ready_plan(&fixture.fixture_id, baseline, &marker)?;
                return validated_plan(plan, &marker);
            }
            Err(CandidateError::Rejected(code)) => {
                first_rejection.get_or_insert(code);
            }
            Err(CandidateError::Structural(error)) => return Err(error),
        }
    }

    validated_plan(
        rejected_plan(
            &fixture.fixture_id,
            first_rejection.unwrap_or("no_valid_candidate"),
        ),
        &marker,
    )
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RootKind {
    Directory,
    Reparse,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ManifestState {
    Valid,
    Corrupt,
}

struct ParsedFixture {
    fixture_id: String,
    root_kind: RootKind,
    manifest_state: ManifestState,
    available_bytes: u64,
    candidates: Vec<Candidate>,
}

impl ParsedFixture {
    fn parse(text: &str, marker: &Path) -> Result<Self, FixtureError> {
        let mut schema = None;
        let mut fixture_id = None;
        let mut root_kind = None;
        let mut manifest_state = None;
        let mut available_bytes = None;
        let mut candidates = Vec::new();

        for (index, raw_line) in text.lines().enumerate() {
            let line_number = index + 1;
            let line = raw_line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            let (key, value) = line.split_once('=').ok_or_else(|| {
                malformed(
                    marker,
                    line_number,
                    "each non-comment line must contain key=value",
                )
            })?;
            if key.is_empty() || value.is_empty() {
                return Err(malformed(
                    marker,
                    line_number,
                    "fixture keys and values must not be empty",
                ));
            }

            match key {
                "schema" => set_single(&mut schema, value, key, marker, line_number)?,
                "fixture_id" => set_single(&mut fixture_id, value, key, marker, line_number)?,
                "root_kind" => set_single(&mut root_kind, value, key, marker, line_number)?,
                "manifest_state" => {
                    set_single(&mut manifest_state, value, key, marker, line_number)?
                }
                "available_bytes" => {
                    set_single(&mut available_bytes, value, key, marker, line_number)?
                }
                "candidate" => candidates.push(Candidate::parse(value, marker, line_number)?),
                _ => {
                    return Err(malformed(
                        marker,
                        line_number,
                        format!("unknown fixture key: {key}"),
                    ));
                }
            }
        }

        let schema = required(schema, "schema", marker)?;
        if schema != FIXTURE_SCHEMA {
            return Err(FixtureError::new(
                "unsupported_fixture_schema",
                marker,
                format!("expected schema {FIXTURE_SCHEMA}, found {schema}"),
            ));
        }

        let fixture_id = required(fixture_id, "fixture_id", marker)?;
        let root_kind = match required(root_kind, "root_kind", marker)?.as_str() {
            "directory" => RootKind::Directory,
            "reparse" => RootKind::Reparse,
            value => {
                return Err(FixtureError::new(
                    "malformed_fixture",
                    marker,
                    format!("invalid root_kind: {value}"),
                ));
            }
        };
        let manifest_state = match required(manifest_state, "manifest_state", marker)?.as_str() {
            "valid" => ManifestState::Valid,
            "corrupt" => ManifestState::Corrupt,
            value => {
                return Err(FixtureError::new(
                    "malformed_fixture",
                    marker,
                    format!("invalid manifest_state: {value}"),
                ));
            }
        };
        let available_bytes = required(available_bytes, "available_bytes", marker)?
            .parse::<u64>()
            .map_err(|error| {
                FixtureError::new(
                    "malformed_fixture",
                    marker,
                    format!("available_bytes must be u64: {error}"),
                )
            })?;
        if candidates.is_empty() {
            return Err(FixtureError::new(
                "malformed_fixture",
                marker,
                "fixture must contain at least one candidate",
            ));
        }

        Ok(Self {
            fixture_id,
            root_kind,
            manifest_state,
            available_bytes,
            candidates,
        })
    }
}

struct Candidate {
    baseline_id: String,
    package_full_name: String,
    version: String,
    version_parts: [u32; 4],
    architecture: String,
    publisher: String,
    source: String,
    expected_bytes: u64,
    expected_sha256: Sha256Digest,
}

impl Candidate {
    fn parse(value: &str, marker: &Path, line_number: usize) -> Result<Self, FixtureError> {
        let fields = value.split('|').collect::<Vec<_>>();
        if fields.len() != 8 {
            return Err(malformed(
                marker,
                line_number,
                "candidate must contain exactly eight pipe-delimited fields",
            ));
        }
        if fields
            .iter()
            .any(|field| field.is_empty() || field.chars().any(char::is_control))
        {
            return Err(malformed(
                marker,
                line_number,
                "candidate fields must be nonempty and contain no control characters",
            ));
        }

        let version_parts = parse_version(fields[2]).ok_or_else(|| {
            malformed(
                marker,
                line_number,
                "candidate version must contain exactly four u32 components",
            )
        })?;
        let expected_bytes = fields[6].parse::<u64>().map_err(|error| {
            malformed(
                marker,
                line_number,
                format!("candidate expected_bytes must be u64: {error}"),
            )
        })?;
        if expected_bytes == 0 {
            return Err(malformed(
                marker,
                line_number,
                "candidate expected_bytes must be greater than zero",
            ));
        }
        let expected_sha256 = Sha256Digest::parse(fields[7]).map_err(|error| {
            malformed(
                marker,
                line_number,
                format!("candidate expected_sha256 is invalid: {error}"),
            )
        })?;

        Ok(Self {
            baseline_id: fields[0].to_owned(),
            package_full_name: fields[1].to_owned(),
            version: fields[2].to_owned(),
            version_parts,
            architecture: fields[3].to_owned(),
            publisher: fields[4].to_owned(),
            source: fields[5].to_owned(),
            expected_bytes,
            expected_sha256,
        })
    }
}

enum CandidateError {
    Rejected(&'static str),
    Structural(FixtureError),
}

fn validate_candidate(
    root: &Path,
    canonical_root: &Path,
    available_bytes: u64,
    candidate: &Candidate,
) -> Result<BaselineV2, CandidateError> {
    if candidate.architecture != EXPECTED_ARCHITECTURE {
        return Err(CandidateError::Rejected("architecture_mismatch"));
    }
    if candidate.publisher != EXPECTED_PUBLISHER {
        return Err(CandidateError::Rejected("unknown_publisher"));
    }

    let safe_source = SafeRelativePath::parse(&candidate.source)
        .map_err(|_| CandidateError::Rejected("unsafe_path"))?;
    if available_bytes < candidate.expected_bytes {
        return Err(CandidateError::Rejected("insufficient_disk"));
    }

    let source_path = root.join(safe_source.as_str());
    ensure_no_reparse_components(root, &safe_source, &source_path)?;
    let source_metadata = fs::symlink_metadata(&source_path).map_err(|error| {
        CandidateError::Structural(FixtureError::new(
            "fixture_source_unreadable",
            &source_path,
            format!("fixture source cannot be read: {error}"),
        ))
    })?;
    if !source_metadata.is_file() {
        return Err(CandidateError::Structural(FixtureError::new(
            "fixture_source_not_file",
            &source_path,
            "fixture source must be a regular file",
        )));
    }

    let canonical_source = fs::canonicalize(&source_path).map_err(|error| {
        CandidateError::Structural(FixtureError::new(
            "fixture_source_unreadable",
            &source_path,
            format!("fixture source cannot be canonicalized: {error}"),
        ))
    })?;
    if !canonical_source.starts_with(canonical_root) {
        return Err(CandidateError::Rejected("unsafe_path"));
    }
    if source_metadata.len() != candidate.expected_bytes {
        return Err(CandidateError::Rejected("size_mismatch"));
    }

    let actual_sha256 = sha256_file(&source_path).map_err(|error| {
        CandidateError::Structural(FixtureError::new(
            "fixture_source_unreadable",
            &source_path,
            format!("fixture source cannot be hashed: {error}"),
        ))
    })?;
    if actual_sha256 != candidate.expected_sha256 {
        return Err(CandidateError::Rejected("hash_mismatch"));
    }

    Ok(BaselineV2 {
        baseline_id: candidate.baseline_id.clone(),
        package_full_name: candidate.package_full_name.clone(),
        version: candidate.version.clone(),
        architecture: candidate.architecture.clone(),
        publisher: candidate.publisher.clone(),
        source: safe_source,
        bytes: candidate.expected_bytes,
        sha256: candidate.expected_sha256.clone(),
    })
}

fn ensure_no_reparse_components(
    root: &Path,
    source: &SafeRelativePath,
    source_path: &Path,
) -> Result<(), CandidateError> {
    let mut current = root.to_owned();
    for component in source.as_str().split('/') {
        current.push(component);
        let metadata = fs::symlink_metadata(&current).map_err(|error| {
            CandidateError::Structural(FixtureError::new(
                "fixture_source_unreadable",
                source_path,
                format!("fixture source cannot be read: {error}"),
            ))
        })?;
        if is_reparse_point(&metadata) {
            return Err(CandidateError::Rejected("reparse_path"));
        }
    }
    Ok(())
}

fn ready_plan(
    fixture_id: &str,
    baseline: BaselineV2,
    marker: &Path,
) -> Result<PlanV1, FixtureError> {
    let baseline_root = format!("baselines/{}", baseline.baseline_id);
    let actions = vec![
        action(PlanActionKind::WouldCopy, &baseline_root, marker)?,
        action(
            PlanActionKind::WouldWrite,
            &format!("{baseline_root}/chatgpt-fix-baseline.json"),
            marker,
        )?,
        action(PlanActionKind::WouldSwitch, "current.json", marker)?,
        action(PlanActionKind::WouldStart, "launcher/ChatGPT.exe", marker)?,
        action(PlanActionKind::WouldTerminate, "owned-processes", marker)?,
    ];

    Ok(PlanV1 {
        fixture_id: fixture_id.to_owned(),
        decision: PlanDecision::Ready,
        baseline: Some(baseline),
        actions,
        errors: Vec::new(),
    })
}

fn action(kind: PlanActionKind, target: &str, marker: &Path) -> Result<PlanAction, FixtureError> {
    let target = SafeRelativePath::parse(target).map_err(|error| {
        FixtureError::new(
            "malformed_fixture",
            marker,
            format!("candidate cannot form a safe action target: {error}"),
        )
    })?;
    Ok(PlanAction {
        kind,
        target,
        execute: false,
    })
}

fn rejected_plan(fixture_id: &str, code: &'static str) -> PlanV1 {
    PlanV1 {
        fixture_id: fixture_id.to_owned(),
        decision: PlanDecision::Rejected,
        baseline: None,
        actions: Vec::new(),
        errors: vec![code.to_owned()],
    }
}

fn validated_plan(plan: PlanV1, marker: &Path) -> Result<PlanV1, FixtureError> {
    plan.validate().map_err(|error| {
        FixtureError::new(
            "invalid_generated_plan",
            marker,
            format!("generated plan violates its contract: {error}"),
        )
    })?;
    Ok(plan)
}

fn parse_version(value: &str) -> Option<[u32; 4]> {
    let mut parts = value.split('.');
    let version = [
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
    ];
    if parts.next().is_some() {
        return None;
    }
    Some(version)
}

fn set_single(
    slot: &mut Option<String>,
    value: &str,
    key: &str,
    marker: &Path,
    line_number: usize,
) -> Result<(), FixtureError> {
    if slot.is_some() {
        return Err(malformed(
            marker,
            line_number,
            format!("duplicate single-value field: {key}"),
        ));
    }
    if value.chars().any(char::is_control) {
        return Err(malformed(
            marker,
            line_number,
            format!("field contains a control character: {key}"),
        ));
    }
    *slot = Some(value.to_owned());
    Ok(())
}

fn required(value: Option<String>, key: &str, marker: &Path) -> Result<String, FixtureError> {
    value.ok_or_else(|| {
        FixtureError::new(
            "malformed_fixture",
            marker,
            format!("missing required field: {key}"),
        )
    })
}

fn malformed(marker: &Path, line_number: usize, message: impl Into<String>) -> FixtureError {
    FixtureError::new(
        "malformed_fixture",
        marker,
        format!("line {line_number}: {}", message.into()),
    )
}

fn is_reparse_point(metadata: &Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;

        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
        metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    }

    #[cfg(not(windows))]
    {
        metadata.file_type().is_symlink()
    }
}
