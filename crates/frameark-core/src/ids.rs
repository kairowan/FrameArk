use std::fmt::{Debug, Display, Formatter};

/// Failure returned when a public identifier is malformed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum IdentifierError {
    /// The identifier is empty or contains only whitespace.
    Empty,
    /// The identifier exceeds the bounded core limit.
    TooLong,
    /// The identifier contains a control character.
    ControlCharacter,
}

impl Display for IdentifierError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => formatter.write_str("identifier is empty"),
            Self::TooLong => formatter.write_str("identifier exceeds 128 characters"),
            Self::ControlCharacter => {
                formatter.write_str("identifier contains a control character")
            }
        }
    }
}

impl std::error::Error for IdentifierError {}

macro_rules! id_type {
    ($name:ident, $doc:literal) => {
        #[doc = $doc]
        #[derive(Clone, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name(String);

        impl $name {
            /// Creates a bounded, non-empty identifier.
            pub fn new(value: impl Into<String>) -> std::result::Result<Self, IdentifierError> {
                let value = value.into();
                if value.trim().is_empty() {
                    return Err(IdentifierError::Empty);
                }
                if value.chars().count() > 128 {
                    return Err(IdentifierError::TooLong);
                }
                if value.chars().any(char::is_control) {
                    return Err(IdentifierError::ControlCharacter);
                }
                Ok(Self(value))
            }

            /// Returns the identifier as a string slice.
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl TryFrom<String> for $name {
            type Error = IdentifierError;

            fn try_from(value: String) -> std::result::Result<Self, Self::Error> {
                Self::new(value)
            }
        }

        impl TryFrom<&str> for $name {
            type Error = IdentifierError;

            fn try_from(value: &str) -> std::result::Result<Self, Self::Error> {
                Self::new(value)
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                self.as_str()
            }
        }

        impl Display for $name {
            fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
                formatter.write_str(self.as_str())
            }
        }

        impl Debug for $name {
            fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
                formatter
                    .debug_tuple(stringify!($name))
                    .field(&self.0)
                    .finish()
            }
        }
    };
}

id_type!(
    DeviceId,
    "A stable identifier for a discovered or trusted device."
);
id_type!(SessionId, "A unique identifier for one media session.");
id_type!(TrackId, "A unique identifier for one media track.");
