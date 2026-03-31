use std::fmt::{Display, Formatter};
use std::io;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ForgeError {
    Message(String),
}

impl ForgeError {
    pub fn message(message: impl Into<String>) -> Self {
        Self::Message(message.into())
    }
}

impl Display for ForgeError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Message(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for ForgeError {}

impl From<io::Error> for ForgeError {
    fn from(value: io::Error) -> Self {
        Self::message(value.to_string())
    }
}

impl From<toml::de::Error> for ForgeError {
    fn from(value: toml::de::Error) -> Self {
        Self::message(format!("TOML parse error: {value}"))
    }
}

impl From<serde_json::Error> for ForgeError {
    fn from(value: serde_json::Error) -> Self {
        Self::message(value.to_string())
    }
}
