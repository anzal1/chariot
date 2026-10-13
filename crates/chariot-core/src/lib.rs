//! Chariot core: the state and rules behind a team of AI coding agents.
//!
//! This crate is the synchronous heart of Chariot. It holds the vocabulary
//! (members, tasks, actions, reservations, holds, events) and a SQLite backed
//! [`Store`] that keeps all of it, including an append-only event log. Every
//! mutating [`Store`] method writes its event in the same transaction as the
//! change, so the log never disagrees with the state.
//!
//! There is no async and no I/O beyond SQLite. Drivers, the board and the
//! protocol layer live in other crates and talk to this one.

mod error;
mod event;
mod ids;
mod model;
mod store;

pub use error::{ClaimError, Conflict, Error, Result};
pub use event::{Event, EventKind};
pub use ids::{ActionId, HoldId, MemberId, TaskId};
pub use model::{
    Action, ActionOp, ActionState, Evidence, EvidenceKind, Hold, HoldKind, HoldOption, Level,
    Member, MemberKind, Mode, NewAction, NewHold, NewTask, Reservation, ResourceClaim, Surface,
    Task, TaskResult, TaskState, Timestamp, Usage,
};
pub use store::{Store, WriteCheck};
