use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::ids::{ActionId, HoldId, MemberId, TaskId};

/// A UTC instant, stored with millisecond precision and serialised as RFC 3339.
pub type Timestamp = OffsetDateTime;

/// What sort of thing a member is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemberKind {
    /// A coding agent harness such as Claude Code, OpenCode or Codex.
    Harness,
    /// A bare model behind an API or local runtime.
    Model,
    /// A script or program that does one job.
    Tool,
    /// A hosted service.
    Service,
    /// A person answering through the board.
    Human,
}

/// How the core reaches a member.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Surface {
    /// A structured protocol such as ACP or MCP.
    Protocol,
    /// A CLI run headless with structured output.
    HeadlessCli,
    /// A model CLI.
    ModelCli,
    /// A vendor SDK.
    Sdk,
    /// A plain HTTP API.
    Api,
    /// A terminal session driven through a pty.
    Terminal,
    /// A website driven through a browser.
    Website,
}

/// The privilege level a member is certified at. Higher levels unlock more.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Level {
    /// Hear: takes a task, returns a result.
    L0,
    /// See: streams structured events.
    L1,
    /// Gate: proposes each action and waits for a decision.
    L2,
    /// Steer: accepts injected context mid-session.
    L3,
    /// Rewind: can checkpoint and roll back its own actions.
    L4,
}

/// Anything that can do work.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Member {
    /// Unique identifier.
    pub id: MemberId,
    /// Display name.
    pub name: String,
    /// What the member is.
    pub kind: MemberKind,
    /// How it is reached.
    pub surface: Surface,
    /// Certified level.
    pub level: Level,
    /// Whether the member is currently connected and able to take work.
    pub live: bool,
}

/// Whether a claim on a resource is for reading or writing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    /// Shared access. Never conflicts.
    Read,
    /// Exclusive write access.
    Write,
}

/// A resource a task intends to touch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceClaim {
    /// A URI or glob over URIs, for example `file:///repo/src/**`.
    pub uri: String,
    /// Read or write.
    pub mode: Mode,
}

/// Where a task is in its life.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskState {
    /// Waiting for a member.
    Queued,
    /// A member is working on it.
    Working,
    /// Reported done, waiting for review or the done check.
    Review,
    /// Accepted.
    Done,
    /// Waiting on a hold.
    Held,
    /// Ended without meeting the done list.
    Failed,
    /// Stopped on purpose.
    Cancelled,
}

/// A goal plus what "done" means.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Task {
    /// Unique identifier.
    pub id: TaskId,
    /// What to achieve.
    pub goal: String,
    /// The done list: every line must hold for the task to count as done.
    pub done: Vec<String>,
    /// Resources the task expects to use.
    pub resources: Vec<ResourceClaim>,
    /// Current state.
    pub state: TaskState,
    /// Member currently responsible, if any.
    pub owner: Option<MemberId>,
    /// Task this one was split from, if any.
    pub parent: Option<TaskId>,
    /// When the task was created.
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: Timestamp,
    /// When the task last changed.
    #[serde(with = "time::serde::rfc3339")]
    pub updated_at: Timestamp,
}

/// Input for [`Store::create_task`](crate::Store::create_task).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NewTask {
    /// What to achieve.
    pub goal: String,
    /// The done list.
    pub done: Vec<String>,
    /// Resources the task expects to use.
    pub resources: Vec<ResourceClaim>,
    /// Parent task, if this is a sub-task.
    pub parent: Option<TaskId>,
}

/// What kind of proof a piece of evidence is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceKind {
    /// A test run.
    Test,
    /// Any other automated check.
    Check,
    /// A source that backs a claim.
    Source,
    /// The member could not finish and says why.
    Blocked,
    /// A free-form remark.
    Note,
}

/// One piece of proof attached to a result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Evidence {
    /// What sort of proof.
    pub kind: EvidenceKind,
    /// The proof itself, for example the test summary line.
    pub detail: String,
}

/// Token and quota usage reported by a member.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Usage {
    /// Input tokens consumed.
    pub tokens_in: u64,
    /// Output tokens produced.
    pub tokens_out: u64,
    /// Percent of the subscription quota left, if the member knows.
    pub quota_left_pct: Option<f32>,
}

/// A member's report on a task.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaskResult {
    /// The task reported on.
    pub task: TaskId,
    /// The state the member claims the task reached.
    pub status: TaskState,
    /// The member's own summary. The board shows verdicts, not this.
    pub content: String,
    /// Proof for the done list.
    pub evidence: Vec<Evidence>,
    /// Usage for this run, if reported.
    pub usage: Option<Usage>,
}

/// What an action does to its resource.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionOp {
    /// Change an existing resource.
    Edit,
    /// Create a new resource.
    Create,
    /// Remove a resource.
    Delete,
    /// Move or rename a resource.
    Move,
    /// Run something, for example a shell command.
    Execute,
    /// Read from outside, for example an HTTP GET.
    Fetch,
    /// Send something out, for example mail or a POST.
    Send,
    /// Anything else.
    Other,
}

impl ActionOp {
    /// Whether the operation changes the target in place, which makes the
    /// member a recent writer of it.
    pub fn writes(self) -> bool {
        matches!(self, Self::Edit | Self::Create | Self::Delete | Self::Move)
    }
}

/// Where an action is in its decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionState {
    /// Waiting for a decision.
    Proposed,
    /// Allowed to proceed.
    Approved,
    /// Refused.
    Denied,
    /// Happened.
    Done,
}

/// An entry in the action record: something a member wants to do or did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Action {
    /// Unique identifier.
    pub id: ActionId,
    /// The task it belongs to.
    pub task: TaskId,
    /// The member doing it.
    pub member: MemberId,
    /// Target URI, for example `file:///repo/a.js`.
    pub resource: String,
    /// What is done to the target.
    pub op: ActionOp,
    /// A preview such as a unified diff.
    pub preview: Option<String>,
    /// Whether the effect can be undone.
    pub reversible: bool,
    /// Decision state.
    pub state: ActionState,
    /// When it was proposed.
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: Timestamp,
}

/// Input for [`Store::propose_action`](crate::Store::propose_action).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewAction {
    /// The task it belongs to.
    pub task: TaskId,
    /// The member doing it.
    pub member: MemberId,
    /// Target URI.
    pub resource: String,
    /// What is done to the target.
    pub op: ActionOp,
    /// A preview such as a unified diff.
    pub preview: Option<String>,
    /// Whether the effect can be undone.
    pub reversible: bool,
}

/// A claim on a set of resources.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reservation {
    /// A glob over resource URIs.
    pub pattern: String,
    /// The member that holds it.
    pub holder: MemberId,
    /// The task it was taken for.
    pub task: Option<TaskId>,
    /// Read or write.
    pub mode: Mode,
    /// When it lapses. `None` means it stays until released.
    #[serde(default, with = "time::serde::rfc3339::option")]
    pub expires_at: Option<Timestamp>,
    /// True when the core took it on the member's behalf rather than the
    /// member asking for it.
    pub auto: bool,
}

/// The five moments that need a human. Exactly these.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HoldKind {
    /// Two tasks disagree.
    Contradiction,
    /// A member hit its quota and work could move to another.
    QuotaFailover,
    /// An edit would overwrite another member's recent work.
    HeldEdit,
    /// A done check failed.
    FailedCheck,
    /// An action cannot be undone.
    Irreversible,
}

/// One way to answer a hold.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HoldOption {
    /// The key the person presses, for example `y`.
    pub key: String,
    /// What the key does.
    pub label: String,
}

/// Something waiting on a person.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hold {
    /// Unique identifier.
    pub id: HoldId,
    /// Which of the five moments this is.
    pub kind: HoldKind,
    /// The task it concerns.
    pub task: TaskId,
    /// What is being asked.
    pub question: String,
    /// The allowed answers.
    pub options: Vec<HoldOption>,
    /// Option key applied automatically when the timeout passes.
    pub default_on_timeout: Option<String>,
    /// When the default applies.
    #[serde(default, with = "time::serde::rfc3339::option")]
    pub timeout_at: Option<Timestamp>,
    /// The chosen option key, once answered.
    pub answer: Option<String>,
    /// When it was opened.
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: Timestamp,
    /// When it was answered.
    #[serde(default, with = "time::serde::rfc3339::option")]
    pub answered_at: Option<Timestamp>,
}

/// Input for [`Store::open_hold`](crate::Store::open_hold).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewHold {
    /// Which of the five moments this is.
    pub kind: HoldKind,
    /// The task it concerns.
    pub task: TaskId,
    /// What is being asked.
    pub question: String,
    /// The allowed answers. May be empty for free-form answers.
    pub options: Vec<HoldOption>,
    /// Option key applied when the timeout passes.
    pub default_on_timeout: Option<String>,
    /// How long until the default applies, counted from now.
    pub timeout: Option<std::time::Duration>,
}
