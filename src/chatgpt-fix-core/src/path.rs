use std::fmt;
use std::str::FromStr;

use crate::ContractError;

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
