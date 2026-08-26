use std::ffi::OsString;
use std::fmt;
use std::path::Path;
use std::str::FromStr;

use crate::ContractError;

/// Windows long-path helper: absolute paths longer than MAX_PATH (260 chars)
/// fail in `std::fs` because Rust does not auto-prefix them. This converts an
/// absolute path to its `\\?\`-prefixed form so every std fs call works for
/// deep trees (observed: OpenAI.Codex 26.814 ships a 371-char path under
/// `resources/cua_node/.../pnpm-store/...`). Non-Windows and relative paths
/// pass through unchanged; already-prefixed and device paths are left alone.
pub fn long_path(path: &Path) -> OsString {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::{OsStrExt, OsStringExt};
        let text = path.to_string_lossy();
        if !path.is_absolute() || text.starts_with(r"\\?\") || text.starts_with(r"\\.\") {
            return path.as_os_str().to_os_string();
        }
        let mut wide: Vec<u16> = path.as_os_str().encode_wide().collect();
        // Only prefix paths that actually need it. The `\\?\` form disables
        // path normalization, so `.`/`..` segments and forward slashes that
        // would be resolved by the OS are NOT processed for prefixed paths
        // (os errors 161/123). Prefixing only long paths keeps short paths
        // on the plain normalized path and confines these semantics to the
        // >MAX_PATH case for which \\?\ was invented.
        if wide.len() < 240 {
            return path.as_os_str().to_os_string();
        }
        // Normalize `/` to `\\` for the prefixed form; callers may pass
        // SafeRelativePath-style forward-slash segments joined onto a root.
        for w in &mut wide {
            if *w == b'/' as u16 {
                *w = b'\\' as u16;
            }
        }
        let mut out = Vec::with_capacity(wide.len() + 8);
        if text.starts_with(r"\\") {
            // UNC: \\server\share -> \\?\UNC\server\share
            out.extend_from_slice(&[b'\\' as u16, b'\\' as u16, b'?' as u16, b'\\' as u16]);
            out.extend_from_slice(&[b'U' as u16, b'N' as u16, b'C' as u16, b'\\' as u16]);
            out.extend_from_slice(&wide[2..]);
        } else {
            out.extend_from_slice(&[b'\\' as u16, b'\\' as u16, b'?' as u16, b'\\' as u16]);
            out.extend_from_slice(&wide);
        }
        OsString::from_wide(&out)
    }
    #[cfg(not(windows))]
    {
        path.as_os_str().to_os_string()
    }
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SafeRelativePath(String);

impl SafeRelativePath {
    pub fn parse(value: &str) -> Result<Self, ContractError> {
        if value.is_empty() {
            return Err(invalid_path("must not be empty"));
        }
        if value.starts_with('/') {
            return Err(invalid_path("must not start with '/'"));
        }
        if value.contains('\\') {
            return Err(invalid_path("must use forward slashes only"));
        }
        if value.contains(':') {
            return Err(invalid_path("must not contain ':'"));
        }
        if value.chars().any(char::is_control) {
            return Err(invalid_path("must not contain control characters"));
        }

        for segment in value.split('/') {
            if segment.is_empty() {
                return Err(invalid_path("must not contain empty segments"));
            }
            if matches!(segment, "." | "..") {
                return Err(invalid_path("must not contain '.' or '..' segments"));
            }
        }

        Ok(Self(value.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

fn invalid_path(message: &'static str) -> ContractError {
    ContractError::new("invalid_relative_path", "path", message)
}

impl fmt::Display for SafeRelativePath {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for SafeRelativePath {
    type Err = ContractError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}

impl TryFrom<&str> for SafeRelativePath {
    type Error = ContractError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Sha256Digest(String);

impl Sha256Digest {
    pub fn parse(value: &str) -> Result<Self, ContractError> {
        if value.len() != 64 || !value.as_bytes().iter().all(u8::is_ascii_hexdigit) {
            return Err(ContractError::new(
                "invalid_sha256",
                "sha256",
                "must contain exactly 64 ASCII hexadecimal characters",
            ));
        }

        Ok(Self(value.to_ascii_lowercase()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Sha256Digest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for Sha256Digest {
    type Err = ContractError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}

impl TryFrom<&str> for Sha256Digest {
    type Error = ContractError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}
