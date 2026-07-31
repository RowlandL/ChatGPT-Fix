use std::fmt;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContractError {
    pub code: String,
    pub field: String,
    pub message: String,
}

impl ContractError {
    pub(crate) fn new(
        code: &'static str,
        field: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            code: code.to_owned(),
            field: field.into(),
            message: message.into(),
        }
    }
}

impl fmt::Display for ContractError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{} at {}: {}",
            self.code, self.field, self.message
        )
    }
}

impl std::error::Error for ContractError {}
