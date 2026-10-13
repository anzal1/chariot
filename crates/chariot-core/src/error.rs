use thiserror::Error;

use crate::ids::MemberId;

/// Result alias for store operations.
pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Everything that can go wrong in the store, other than a reservation
/// conflict (see [`ClaimError`]).
#[derive(Debug, Error)]
pub enum Error {
    /// The database failed.
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    /// A value could not be serialised.
    #[error("serialisation error: {0}")]
    Json(#[from] serde_json::Error),
    /// The referenced record does not exist.
    #[error("{kind} not found: {id}")]
    NotFound {
        /// What was looked up, for example `task`.
        kind: &'static str,
        /// The identifier that was not found.
        id: String,
    },
    /// A record with this identifier already exists.
    #[error("{kind} already exists: {id}")]
    Exists {
        /// What was inserted, for example `member`.
        kind: &'static str,
        /// The duplicate identifier.
        id: String,
    },
    /// The operation is not valid in the record's current state.
    #[error("{0}")]
    InvalidState(String),
    /// A hold answer is not one of the hold's option keys.
    #[error("`{answer}` is not an option of hold {hold}")]
    InvalidAnswer {
        /// The hold being answered.
        hold: String,
        /// The rejected answer.
        answer: String,
    },
    /// A reservation pattern is not a valid glob.
    #[error("invalid glob `{pattern}`: {source}")]
    InvalidGlob {
        /// The rejected pattern.
        pattern: String,
        /// The parser's complaint.
        source: globset::Error,
    },
    /// A time or duration does not fit the supported range.
    #[error("time out of range")]
    TimeOutOfRange,
    /// The database was written by a newer version of Chariot.
    #[error("database schema version {found} is newer than supported version {supported}")]
    SchemaTooNew {
        /// Version found in the file.
        found: i64,
        /// Highest version this build understands.
        supported: i64,
    },
}

/// Why a write claim was refused: another member holds an overlapping
/// write reservation.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("`{pattern}` is write-reserved by {holder}")]
pub struct Conflict {
    /// The member that holds the overlapping reservation.
    pub holder: MemberId,
    /// The pattern that member reserved.
    pub pattern: String,
}

/// Error type of [`Store::claim`](crate::Store::claim): either a plain
/// conflict, which is an expected outcome, or a real failure.
#[derive(Debug, Error)]
pub enum ClaimError {
    /// An overlapping write reservation is held by someone else.
    #[error(transparent)]
    Conflict(#[from] Conflict),
    /// The store failed.
    #[error(transparent)]
    Store(#[from] Error),
}

impl From<rusqlite::Error> for ClaimError {
    fn from(e: rusqlite::Error) -> Self {
        Error::from(e).into()
    }
}

impl From<serde_json::Error> for ClaimError {
    fn from(e: serde_json::Error) -> Self {
        Error::from(e).into()
    }
}
