use serde::{Deserialize, Serialize};

use crate::ids::{ActionId, HoldId, MemberId, TaskId};
use crate::model::{ActionOp, HoldKind, Level, MemberKind, Mode, TaskState, Timestamp};

/// One entry in the append-only event log.
///
/// The envelope is the same for every event; what happened is in
/// [`kind`](Event::kind).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Event {
    /// Position in the log. Strictly increasing, starts at 1.
    pub seq: u64,
    /// When it happened.
    #[serde(with = "time::serde::rfc3339")]
    pub at: Timestamp,
    /// The task it concerns, if any.
    pub task: Option<TaskId>,
    /// The member it concerns, if any.
    pub member: Option<MemberId>,
    /// What happened.
    pub kind: EventKind,
}

/// What happened. One variant per kind of state change.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EventKind {
    /// A member was enrolled.
    MemberAdded {
        /// Display name.
        name: String,
        /// What it is.
        kind: MemberKind,
        /// Certified level.
        level: Level,
    },
    /// A member connected or disconnected.
    MemberLiveChanged {
        /// The new value.
        live: bool,
    },
    /// A task was created.
    TaskCreated {
        /// The task's goal.
        goal: String,
        /// Parent task, if any.
        parent: Option<TaskId>,
    },
    /// A task moved between states.
    TaskStateChanged {
        /// Previous state.
        from: TaskState,
        /// New state.
        to: TaskState,
    },
    /// A task changed owner.
    TaskOwnerChanged {
        /// The new owner, or none.
        owner: Option<MemberId>,
    },
    /// A member proposed an action.
    ActionProposed {
        /// The action.
        action: ActionId,
        /// Its target URI.
        resource: String,
        /// What it does.
        op: ActionOp,
    },
    /// A proposed action was approved or denied.
    ActionDecided {
        /// The action.
        action: ActionId,
        /// True for approved.
        approved: bool,
        /// Why, if a reason was given.
        reason: Option<String>,
    },
    /// An action happened.
    ActionDone {
        /// The action.
        action: ActionId,
        /// Its target URI.
        resource: String,
    },
    /// A write was recorded against a URI for revert detection.
    WriteRecorded {
        /// The URI, without any fragment.
        uri: String,
    },
    /// A reservation was taken or refreshed.
    ReservationTaken {
        /// The reserved glob.
        pattern: String,
        /// Read or write.
        mode: Mode,
        /// When it lapses, if it does.
        #[serde(default, with = "time::serde::rfc3339::option")]
        expires_at: Option<Timestamp>,
        /// True when the core took it automatically.
        auto: bool,
    },
    /// A reservation was released or lapsed.
    ReservationReleased {
        /// The released glob.
        pattern: String,
        /// True when it lapsed instead of being released.
        expired: bool,
    },
    /// A write claim was refused. The event's member is the requester.
    ReservationDenied {
        /// The member that holds the overlapping reservation.
        holder: MemberId,
        /// The pattern that was asked for.
        pattern: String,
    },
    /// A hold was opened.
    HoldOpened {
        /// The hold.
        hold: HoldId,
        /// Which of the five moments.
        kind: HoldKind,
        /// The question asked.
        question: String,
    },
    /// A hold was answered.
    HoldAnswered {
        /// The hold.
        hold: HoldId,
        /// The chosen option key.
        answer: String,
        /// True when the timeout default was applied.
        defaulted: bool,
    },
    /// A member reported a result.
    ResultRecorded {
        /// The state the member claims.
        status: TaskState,
    },
    /// A free-form note on the record.
    Note {
        /// The text.
        text: String,
    },
}
