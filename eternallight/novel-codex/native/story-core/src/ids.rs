use std::error::Error;
use std::fmt;

const MAX_ID_BYTES: usize = 160;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IdError {
    kind: &'static str,
    reason: &'static str,
}

impl IdError {
    fn new(kind: &'static str, reason: &'static str) -> Self {
        Self { kind, reason }
    }
}

impl fmt::Display for IdError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{} {}", self.kind, self.reason)
    }
}

impl Error for IdError {}

macro_rules! identifier {
    ($name:ident, $kind:literal) => {
        #[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name(String);

        impl $name {
            pub fn parse(value: impl Into<String>) -> Result<Self, IdError> {
                let value = value.into();
                if value.is_empty() {
                    return Err(IdError::new($kind, "must not be empty"));
                }
                if value.len() > MAX_ID_BYTES {
                    return Err(IdError::new($kind, "exceeds 160 bytes"));
                }
                if value.trim() != value || value.chars().any(char::is_control) {
                    return Err(IdError::new(
                        $kind,
                        "contains invalid whitespace or control characters",
                    ));
                }
                Ok(Self(value))
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(&self.0)
            }
        }
    };
}

identifier!(ProjectId, "project id");
identifier!(BranchId, "branch id");
identifier!(ObjectId, "object id");
identifier!(RevisionId, "revision id");
identifier!(SnapshotId, "snapshot id");
identifier!(CommitId, "commit id");
identifier!(IdempotencyKey, "idempotency key");

impl BranchId {
    /// Current single-branch application policy belongs to Story Core.
    pub fn main() -> Self { Self("main".into()) }
}
