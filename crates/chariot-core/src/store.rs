use std::path::Path;
use std::time::Duration;

use globset::{GlobBuilder, GlobMatcher};
use rusqlite::{Connection, OptionalExtension, Row, params, types::Type};
use serde::Serialize;
use serde::de::DeserializeOwned;
use time::OffsetDateTime;

use crate::error::{ClaimError, Conflict, Error, Result};
use crate::event::{Event, EventKind};
use crate::ids::{ActionId, HoldId, MemberId, TaskId};
use crate::model::{
    Action, ActionState, Hold, Member, Mode, NewAction, NewHold, NewTask, Reservation, Task,
    TaskResult, TaskState, Timestamp,
};

const SCHEMA_VERSION: i64 = 1;

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS schema_version (version INTEGER NOT NULL);

CREATE TABLE IF NOT EXISTS members (
    id   TEXT PRIMARY KEY,
    live INTEGER NOT NULL,
    data TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS tasks (
    id         TEXT PRIMARY KEY,
    state      TEXT NOT NULL,
    owner      TEXT REFERENCES members(id),
    parent     TEXT REFERENCES tasks(id),
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    data       TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS tasks_state ON tasks(state);

CREATE TABLE IF NOT EXISTS actions (
    id         TEXT PRIMARY KEY,
    task       TEXT NOT NULL REFERENCES tasks(id),
    member     TEXT NOT NULL REFERENCES members(id),
    state      TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    data       TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS reservations (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    pattern    TEXT NOT NULL,
    holder     TEXT NOT NULL REFERENCES members(id),
    task       TEXT REFERENCES tasks(id),
    mode       TEXT NOT NULL,
    expires_at INTEGER,
    auto       INTEGER NOT NULL,
    UNIQUE (holder, pattern, mode)
);

CREATE TABLE IF NOT EXISTS holds (
    id          TEXT PRIMARY KEY,
    task        TEXT NOT NULL REFERENCES tasks(id),
    timeout_at  INTEGER,
    answered_at INTEGER,
    created_at  INTEGER NOT NULL,
    data        TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS results (
    id   INTEGER PRIMARY KEY AUTOINCREMENT,
    task TEXT NOT NULL REFERENCES tasks(id),
    at   INTEGER NOT NULL,
    data TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS writes (
    id     INTEGER PRIMARY KEY AUTOINCREMENT,
    member TEXT NOT NULL REFERENCES members(id),
    task   TEXT NOT NULL REFERENCES tasks(id),
    uri    TEXT NOT NULL,
    at     INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS writes_uri_at ON writes(uri, at);

CREATE TABLE IF NOT EXISTS events (
    seq    INTEGER PRIMARY KEY AUTOINCREMENT,
    at     INTEGER NOT NULL,
    task   TEXT,
    member TEXT,
    data   TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS events_task ON events(task, seq);
";

/// Answer to [`Store::check_write`]: may this member write this URI?
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WriteCheck {
    /// No other member holds a live write reservation covering the URI.
    Allowed,
    /// Another member holds a write reservation covering the URI.
    HeldBy(MemberId),
}

/// The Chariot state store: SQLite in WAL mode, with the event log kept in
/// the same database.
///
/// Mutating methods take `&mut self` and run in one transaction together with
/// the [`Event`] they append. Read methods take `&self`.
pub struct Store {
    conn: Connection,
    clock: Box<dyn Fn() -> Timestamp + Send>,
}

impl Store {
    /// Opens or creates a database file. The schema is created idempotently.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::init(Connection::open(path)?)
    }

    /// Opens a private in-memory database. Handy for tests.
    pub fn open_in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(mut conn: Connection) -> Result<Self> {
        conn.busy_timeout(Duration::from_secs(5))?;
        conn.pragma_update(None, "foreign_keys", true)?;
        conn.pragma_update_and_check(None, "journal_mode", "WAL", |r| r.get::<_, String>(0))?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;

        let tx = conn.transaction()?;
        tx.execute_batch(SCHEMA)?;
        let found: Option<i64> =
            tx.query_row("SELECT MAX(version) FROM schema_version", [], |r| r.get(0))?;
        match found {
            None => {
                tx.execute(
                    "INSERT INTO schema_version(version) VALUES (?1)",
                    [SCHEMA_VERSION],
                )?;
            }
            Some(v) if v > SCHEMA_VERSION => {
                return Err(Error::SchemaTooNew {
                    found: v,
                    supported: SCHEMA_VERSION,
                });
            }
            Some(_) => {}
        }
        tx.commit()?;

        Ok(Self {
            conn,
            clock: Box::new(OffsetDateTime::now_utc),
        })
    }

    /// Replaces the clock used for every timestamp the store creates. The
    /// default is the system clock. Meant for tests and simulation.
    pub fn set_clock(&mut self, clock: impl Fn() -> Timestamp + Send + 'static) {
        self.clock = Box::new(clock);
    }

    /// The store's current time, truncated to milliseconds.
    pub fn now(&self) -> Timestamp {
        from_ms(to_ms((self.clock)()))
    }

    // ---------------------------------------------------------------- members

    /// Enrols a member. Fails with [`Error::Exists`] if the id is taken.
    pub fn add_member(&mut self, member: &Member) -> Result<()> {
        let at = self.now();
        let tx = self.conn.transaction()?;
        if member_exists(&tx, &member.id)? {
            return Err(Error::Exists {
                kind: "member",
                id: member.id.to_string(),
            });
        }
        tx.execute(
            "INSERT INTO members(id, live, data) VALUES (?1, ?2, ?3)",
            params![
                member.id.as_str(),
                member.live,
                serde_json::to_string(member)?
            ],
        )?;
        append(
            &tx,
            at,
            None,
            Some(&member.id),
            &EventKind::MemberAdded {
                name: member.name.clone(),
                kind: member.kind,
                level: member.level,
            },
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Marks a member connected or disconnected.
    pub fn set_member_live(&mut self, id: &MemberId, live: bool) -> Result<()> {
        let at = self.now();
        let tx = self.conn.transaction()?;
        let mut member = get_member(&tx, id)?.ok_or_else(|| not_found("member", id))?;
        if member.live == live {
            return Ok(());
        }
        member.live = live;
        tx.execute(
            "UPDATE members SET live = ?2, data = ?3 WHERE id = ?1",
            params![id.as_str(), live, serde_json::to_string(&member)?],
        )?;
        append(
            &tx,
            at,
            None,
            Some(id),
            &EventKind::MemberLiveChanged { live },
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Looks up one member.
    pub fn member(&self, id: &MemberId) -> Result<Option<Member>> {
        get_member(&self.conn, id)
    }

    /// All members, in enrolment order.
    pub fn members(&self) -> Result<Vec<Member>> {
        let mut stmt = self
            .conn
            .prepare("SELECT data FROM members ORDER BY rowid")?;
        let rows = stmt.query_map([], |r| json_col(r, 0))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    // ------------------------------------------------------------------ tasks

    /// Creates a task in the [`Queued`](TaskState::Queued) state.
    pub fn create_task(&mut self, new: NewTask) -> Result<Task> {
        let at = self.now();
        let tx = self.conn.transaction()?;
        if let Some(parent) = &new.parent {
            require_task(&tx, parent)?;
        }
        let task = Task {
            id: TaskId::generate(),
            goal: new.goal,
            done: new.done,
            resources: new.resources,
            state: TaskState::Queued,
            owner: None,
            parent: new.parent,
            created_at: at,
            updated_at: at,
        };
        tx.execute(
            "INSERT INTO tasks(id, state, owner, parent, created_at, updated_at, data)
             VALUES (?1, ?2, NULL, ?3, ?4, ?4, ?5)",
            params![
                task.id.as_str(),
                enum_to_str(&task.state),
                task.parent.as_ref().map(TaskId::as_str),
                to_ms(at),
                serde_json::to_string(&task)?,
            ],
        )?;
        append(
            &tx,
            at,
            Some(&task.id),
            None,
            &EventKind::TaskCreated {
                goal: task.goal.clone(),
                parent: task.parent.clone(),
            },
        )?;
        tx.commit()?;
        Ok(task)
    }

    /// Looks up one task.
    pub fn task(&self, id: &TaskId) -> Result<Option<Task>> {
        get_task(&self.conn, id)
    }

    /// Tasks in creation order, optionally only those in `state`.
    pub fn tasks(&self, state: Option<TaskState>) -> Result<Vec<Task>> {
        let mut stmt = self
            .conn
            .prepare("SELECT data FROM tasks WHERE (?1 IS NULL OR state = ?1) ORDER BY rowid")?;
        let rows = stmt.query_map([state.as_ref().map(enum_to_str)], |r| json_col(r, 0))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Moves a task to `state`. Setting the state it already has does nothing
    /// and logs nothing.
    pub fn set_task_state(&mut self, id: &TaskId, state: TaskState) -> Result<()> {
        let at = self.now();
        let tx = self.conn.transaction()?;
        let mut task = get_task(&tx, id)?.ok_or_else(|| not_found("task", id))?;
        if task.state == state {
            return Ok(());
        }
        let from = task.state;
        task.state = state;
        task.updated_at = at;
        put_task(&tx, &task)?;
        append(
            &tx,
            at,
            Some(id),
            task.owner.as_ref(),
            &EventKind::TaskStateChanged { from, to: state },
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Sets or clears the member responsible for a task.
    pub fn set_owner(&mut self, id: &TaskId, owner: Option<&MemberId>) -> Result<()> {
        let at = self.now();
        let tx = self.conn.transaction()?;
        let mut task = get_task(&tx, id)?.ok_or_else(|| not_found("task", id))?;
        if let Some(owner) = owner {
            require_member(&tx, owner)?;
        }
        if task.owner.as_ref() == owner {
            return Ok(());
        }
        task.owner = owner.cloned();
        task.updated_at = at;
        put_task(&tx, &task)?;
        append(
            &tx,
            at,
            Some(id),
            owner,
            &EventKind::TaskOwnerChanged {
                owner: owner.cloned(),
            },
        )?;
        tx.commit()?;
        Ok(())
    }

    // ---------------------------------------------------------------- actions

    /// Records an action in the [`Proposed`](ActionState::Proposed) state.
    pub fn propose_action(&mut self, new: NewAction) -> Result<Action> {
        let at = self.now();
        let tx = self.conn.transaction()?;
        require_task(&tx, &new.task)?;
        require_member(&tx, &new.member)?;
        let action = Action {
            id: ActionId::generate(),
            task: new.task,
            member: new.member,
            resource: new.resource,
            op: new.op,
            preview: new.preview,
            reversible: new.reversible,
            state: ActionState::Proposed,
            created_at: at,
        };
        put_action(&tx, &action)?;
        append(
            &tx,
            at,
            Some(&action.task),
            Some(&action.member),
            &EventKind::ActionProposed {
                action: action.id.clone(),
                resource: action.resource.clone(),
                op: action.op,
            },
        )?;
        tx.commit()?;
        Ok(action)
    }

    /// Approves or denies a proposed action. A decision is final.
    pub fn decide_action(
        &mut self,
        id: &ActionId,
        approved: bool,
        reason: Option<&str>,
    ) -> Result<Action> {
        let at = self.now();
        let tx = self.conn.transaction()?;
        let mut action = get_action(&tx, id)?.ok_or_else(|| not_found("action", id))?;
        if action.state != ActionState::Proposed {
            return Err(Error::InvalidState(format!(
                "action {id} was already decided ({:?})",
                action.state
            )));
        }
        action.state = if approved {
            ActionState::Approved
        } else {
            ActionState::Denied
        };
        put_action(&tx, &action)?;
        append(
            &tx,
            at,
            Some(&action.task),
            Some(&action.member),
            &EventKind::ActionDecided {
                action: id.clone(),
                approved,
                reason: reason.map(str::to_owned),
            },
        )?;
        tx.commit()?;
        Ok(action)
    }

    /// Marks an action as having happened. Allowed from `Proposed` (members
    /// below L2 never wait for a decision) and from `Approved`. If the action
    /// changes its target, the member also becomes a recent writer of it, see
    /// [`recent_writers`](Self::recent_writers).
    pub fn complete_action(&mut self, id: &ActionId) -> Result<Action> {
        let at = self.now();
        let tx = self.conn.transaction()?;
        let mut action = get_action(&tx, id)?.ok_or_else(|| not_found("action", id))?;
        if !matches!(action.state, ActionState::Proposed | ActionState::Approved) {
            return Err(Error::InvalidState(format!(
                "action {id} cannot be completed from {:?}",
                action.state
            )));
        }
        action.state = ActionState::Done;
        put_action(&tx, &action)?;
        if action.op.writes() {
            insert_write(&tx, &action.member, &action.task, &action.resource, at)?;
        }
        append(
            &tx,
            at,
            Some(&action.task),
            Some(&action.member),
            &EventKind::ActionDone {
                action: id.clone(),
                resource: action.resource.clone(),
            },
        )?;
        tx.commit()?;
        Ok(action)
    }

    /// Looks up one action.
    pub fn action(&self, id: &ActionId) -> Result<Option<Action>> {
        get_action(&self.conn, id)
    }

    // ----------------------------------------------------------- reservations

    /// Reserves `pattern` (a glob over resource URIs) for `holder`.
    ///
    /// * Read claims never conflict.
    /// * A write claim conflicts with another member's live write reservation
    ///   when the two patterns may cover the same concrete URI. This is
    ///   approximated: they conflict if the patterns are equal, or if either
    ///   glob matches the other's literal pattern text. That catches the
    ///   usual cases (`src/**` against `src/a.js`, `src/*.js` against
    ///   `src/**`) but can miss two globs that overlap only through URIs
    ///   neither names, such as `src/a*` against `src/*b`. A miss is still
    ///   caught per file by [`check_write`](Self::check_write).
    /// * `*` does not cross `/`; `**` does.
    /// * Claiming again with the same holder, pattern and mode refreshes the
    ///   expiry instead of conflicting. Overlap with the holder's own other
    ///   reservations is fine.
    /// * Lapsed reservations are ignored and swept away first.
    ///
    /// `ttl` of `None` keeps the reservation until released. A refused claim
    /// logs a [`ReservationDenied`](EventKind::ReservationDenied) event.
    pub fn claim(
        &mut self,
        holder: &MemberId,
        task: Option<&TaskId>,
        pattern: &str,
        mode: Mode,
        ttl: Option<Duration>,
    ) -> Result<Reservation, ClaimError> {
        self.claim_inner(holder, task, pattern, mode, ttl, false)
    }

    /// Like [`claim`](Self::claim), but marks the reservation as taken by the
    /// core on the member's behalf (`auto = true`).
    pub fn claim_auto(
        &mut self,
        holder: &MemberId,
        task: Option<&TaskId>,
        pattern: &str,
        mode: Mode,
        ttl: Option<Duration>,
    ) -> Result<Reservation, ClaimError> {
        self.claim_inner(holder, task, pattern, mode, ttl, true)
    }

    fn claim_inner(
        &mut self,
        holder: &MemberId,
        task: Option<&TaskId>,
        pattern: &str,
        mode: Mode,
        ttl: Option<Duration>,
        auto: bool,
    ) -> Result<Reservation, ClaimError> {
        let new_glob = matcher(pattern)?;
        let now = self.now();
        let expires_at = ttl.map(|d| add(now, d)).transpose()?;
        let tx = self.conn.transaction()?;
        require_member(&tx, holder)?;
        if let Some(task) = task {
            require_task(&tx, task)?;
        }
        sweep(&tx, now)?;

        if mode == Mode::Write {
            let mut stmt = tx.prepare(
                "SELECT pattern, holder FROM reservations
                 WHERE mode = 'write' AND holder <> ?1 ORDER BY id",
            )?;
            let others = stmt
                .query_map([holder.as_str()], |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        MemberId::new(r.get::<_, String>(1)?),
                    ))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            drop(stmt);
            for (other_pattern, other_holder) in others {
                let overlaps = other_pattern == pattern
                    || matcher(&other_pattern)?.is_match(pattern)
                    || new_glob.is_match(&other_pattern);
                if overlaps {
                    append(
                        &tx,
                        now,
                        task,
                        Some(holder),
                        &EventKind::ReservationDenied {
                            holder: other_holder.clone(),
                            pattern: pattern.to_owned(),
                        },
                    )?;
                    tx.commit()?;
                    return Err(Conflict {
                        holder: other_holder,
                        pattern: other_pattern,
                    }
                    .into());
                }
            }
        }

        tx.execute(
            "INSERT INTO reservations(pattern, holder, task, mode, expires_at, auto)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(holder, pattern, mode) DO UPDATE SET
                task = excluded.task,
                expires_at = excluded.expires_at,
                auto = excluded.auto",
            params![
                pattern,
                holder.as_str(),
                task.map(TaskId::as_str),
                enum_to_str(&mode),
                expires_at.map(to_ms),
                auto,
            ],
        )?;
        append(
            &tx,
            now,
            task,
            Some(holder),
            &EventKind::ReservationTaken {
                pattern: pattern.to_owned(),
                mode,
                expires_at,
                auto,
            },
        )?;
        tx.commit()?;
        Ok(Reservation {
            pattern: pattern.to_owned(),
            holder: holder.clone(),
            task: task.cloned(),
            mode,
            expires_at,
            auto,
        })
    }

    /// Releases every reservation `holder` has on exactly `pattern`, whatever
    /// the mode. Returns whether anything was released.
    pub fn release(&mut self, holder: &MemberId, pattern: &str) -> Result<bool> {
        let at = self.now();
        let tx = self.conn.transaction()?;
        let released = delete_reservations(
            &tx,
            at,
            "holder = ?1 AND pattern = ?2",
            params![holder.as_str(), pattern],
        )?;
        tx.commit()?;
        Ok(released > 0)
    }

    /// Releases every reservation taken for `task`. Returns how many.
    pub fn release_task(&mut self, task: &TaskId) -> Result<usize> {
        let at = self.now();
        let tx = self.conn.transaction()?;
        let released = delete_reservations(&tx, at, "task = ?1", params![task.as_str()])?;
        tx.commit()?;
        Ok(released)
    }

    /// Removes lapsed reservations now, logging each. Claims do this on their
    /// own; call it from a periodic tick if the log should show expiry
    /// promptly. Returns how many were removed.
    pub fn sweep_reservations(&mut self) -> Result<usize> {
        let now = self.now();
        let tx = self.conn.transaction()?;
        let n = sweep(&tx, now)?;
        tx.commit()?;
        Ok(n)
    }

    /// All live reservations, oldest first. Lapsed ones are left out.
    pub fn reservations(&self) -> Result<Vec<Reservation>> {
        let mut stmt = self.conn.prepare(
            "SELECT pattern, holder, task, mode, expires_at, auto FROM reservations
             WHERE expires_at IS NULL OR expires_at > ?1 ORDER BY id",
        )?;
        let rows = stmt.query_map([to_ms(self.now())], reservation_from_row)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// The hard gate: may `member` write `uri` right now?
    ///
    /// Returns [`WriteCheck::HeldBy`] when another member holds a live write
    /// reservation whose glob matches `uri`. A `#fragment` such as `#L10-40`
    /// is ignored. Members with no reservations in the way are allowed; this
    /// does not require the member to hold a reservation itself.
    pub fn check_write(&self, member: &MemberId, uri: &str) -> Result<WriteCheck> {
        let uri = strip_fragment(uri);
        let mut stmt = self.conn.prepare(
            "SELECT pattern, holder FROM reservations
             WHERE mode = 'write' AND holder <> ?1
               AND (expires_at IS NULL OR expires_at > ?2) ORDER BY id",
        )?;
        let rows = stmt
            .query_map(params![member.as_str(), to_ms(self.now())], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for (pattern, holder) in rows {
            if matcher(&pattern)?.is_match(uri) {
                return Ok(WriteCheck::HeldBy(MemberId::new(holder)));
            }
        }
        Ok(WriteCheck::Allowed)
    }

    // ------------------------------------------------------------------ holds

    /// Opens a hold. Holds do not change task state; the caller decides
    /// whether to also move the task to [`Held`](TaskState::Held).
    ///
    /// If `options` is not empty, `default_on_timeout` must be one of its keys.
    pub fn open_hold(&mut self, new: NewHold) -> Result<Hold> {
        let at = self.now();
        let timeout_at = new.timeout.map(|d| add(at, d)).transpose()?;
        let id = HoldId::generate();
        if let Some(default) = &new.default_on_timeout {
            check_answer(&id, &new.options, default)?;
        }
        let tx = self.conn.transaction()?;
        require_task(&tx, &new.task)?;
        let hold = Hold {
            id,
            kind: new.kind,
            task: new.task,
            question: new.question,
            options: new.options,
            default_on_timeout: new.default_on_timeout,
            timeout_at,
            answer: None,
            created_at: at,
            answered_at: None,
        };
        put_hold(&tx, &hold)?;
        append(
            &tx,
            at,
            Some(&hold.task),
            None,
            &EventKind::HoldOpened {
                hold: hold.id.clone(),
                kind: hold.kind,
                question: hold.question.clone(),
            },
        )?;
        tx.commit()?;
        Ok(hold)
    }

    /// Answers an open hold with one of its option keys (any text if the
    /// hold has no options). A hold can be answered once.
    pub fn answer_hold(&mut self, id: &HoldId, answer: &str) -> Result<Hold> {
        let at = self.now();
        let tx = self.conn.transaction()?;
        let mut hold = get_hold(&tx, id)?.ok_or_else(|| not_found("hold", id))?;
        apply_answer(&tx, &mut hold, answer, at, false)?;
        tx.commit()?;
        Ok(hold)
    }

    /// Looks up one hold.
    pub fn hold(&self, id: &HoldId) -> Result<Option<Hold>> {
        get_hold(&self.conn, id)
    }

    /// Holds still waiting for an answer, oldest first.
    pub fn open_holds(&self) -> Result<Vec<Hold>> {
        let mut stmt = self
            .conn
            .prepare("SELECT data FROM holds WHERE answered_at IS NULL ORDER BY rowid")?;
        let rows = stmt.query_map([], |r| json_col(r, 0))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Applies `default_on_timeout` to every open hold whose timeout is at or
    /// before `now`, and returns those holds, answered. Holds that timed out
    /// but have no default stay open: nothing is decided for the person.
    pub fn expire_holds(&mut self, now: Timestamp) -> Result<Vec<Hold>> {
        let now = from_ms(to_ms(now));
        let tx = self.conn.transaction()?;
        let due: Vec<Hold> = {
            let mut stmt = tx.prepare(
                "SELECT data FROM holds
                 WHERE answered_at IS NULL AND timeout_at IS NOT NULL AND timeout_at <= ?1
                 ORDER BY rowid",
            )?;
            let rows = stmt.query_map([to_ms(now)], |r| json_col::<Hold>(r, 0))?;
            rows.collect::<rusqlite::Result<_>>()?
        };
        let mut expired = Vec::new();
        for mut hold in due {
            let Some(default) = hold.default_on_timeout.clone() else {
                continue;
            };
            apply_answer(&tx, &mut hold, &default, now, true)?;
            expired.push(hold);
        }
        tx.commit()?;
        Ok(expired)
    }

    // ---------------------------------------------------------------- results

    /// Stores a member's result for a task and logs it. It does not change
    /// the task's state; the core decides that after checking the evidence.
    pub fn record_result(&mut self, result: &TaskResult) -> Result<()> {
        let at = self.now();
        let tx = self.conn.transaction()?;
        let task = get_task(&tx, &result.task)?.ok_or_else(|| not_found("task", &result.task))?;
        tx.execute(
            "INSERT INTO results(task, at, data) VALUES (?1, ?2, ?3)",
            params![
                result.task.as_str(),
                to_ms(at),
                serde_json::to_string(result)?
            ],
        )?;
        append(
            &tx,
            at,
            Some(&result.task),
            task.owner.as_ref(),
            &EventKind::ResultRecorded {
                status: result.status,
            },
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Results recorded for a task, oldest first.
    pub fn results(&self, task: &TaskId) -> Result<Vec<TaskResult>> {
        let mut stmt = self
            .conn
            .prepare("SELECT data FROM results WHERE task = ?1 ORDER BY id")?;
        let rows = stmt.query_map([task.as_str()], |r| json_col(r, 0))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    // ---------------------------------------------------------- revert watch

    /// Notes that `member` wrote `uri` for `task`, for revert detection.
    /// [`complete_action`](Self::complete_action) does this by itself for
    /// writing operations; call this directly for writes that did not come
    /// through an action. A `#fragment` is dropped.
    pub fn record_write(&mut self, member: &MemberId, task: &TaskId, uri: &str) -> Result<()> {
        let at = self.now();
        let tx = self.conn.transaction()?;
        require_member(&tx, member)?;
        require_task(&tx, task)?;
        insert_write(&tx, member, task, uri, at)?;
        append(
            &tx,
            at,
            Some(task),
            Some(member),
            &EventKind::WriteRecorded {
                uri: strip_fragment(uri).to_owned(),
            },
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Members other than `excluding` that wrote `uri` within the last
    /// `within`, newest first, one row per member and task with the time of
    /// its latest write. A non-empty answer is the cue to open a
    /// [`HeldEdit`](crate::HoldKind::HeldEdit) hold when `excluding` is about
    /// to edit the same file. Matching is on the exact URI without fragment,
    /// so it knows files, not line ranges (v0).
    pub fn recent_writers(
        &self,
        uri: &str,
        within: Duration,
        excluding: &MemberId,
    ) -> Result<Vec<(MemberId, TaskId, Timestamp)>> {
        let since =
            to_ms(self.now()).saturating_sub(within.as_millis().min(i64::MAX as u128) as i64);
        let mut stmt = self.conn.prepare(
            "SELECT member, task, MAX(at) AS last FROM writes
             WHERE uri = ?1 AND at >= ?2 AND member <> ?3
             GROUP BY member, task ORDER BY last DESC, MAX(id) DESC",
        )?;
        let rows = stmt.query_map(
            params![strip_fragment(uri), since, excluding.as_str()],
            |r| {
                Ok((
                    MemberId::new(r.get::<_, String>(0)?),
                    TaskId::new(r.get::<_, String>(1)?),
                    from_ms(r.get(2)?),
                ))
            },
        )?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    // ----------------------------------------------------------------- events

    /// Adds a free-form note to the log.
    pub fn note(
        &mut self,
        task: Option<&TaskId>,
        member: Option<&MemberId>,
        text: &str,
    ) -> Result<()> {
        let at = self.now();
        let tx = self.conn.transaction()?;
        append(
            &tx,
            at,
            task,
            member,
            &EventKind::Note {
                text: text.to_owned(),
            },
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Events with `seq` greater than `since`, in order. Pass 0 for all.
    pub fn events(&self, since: u64) -> Result<Vec<Event>> {
        let mut stmt = self.conn.prepare(
            "SELECT seq, at, task, member, data FROM events WHERE seq > ?1 ORDER BY seq",
        )?;
        let rows = stmt.query_map([since.min(i64::MAX as u64) as i64], event_from_row)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Every event that concerns `task`, in order.
    pub fn events_for_task(&self, task: &TaskId) -> Result<Vec<Event>> {
        let mut stmt = self.conn.prepare(
            "SELECT seq, at, task, member, data FROM events WHERE task = ?1 ORDER BY seq",
        )?;
        let rows = stmt.query_map([task.as_str()], event_from_row)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }
}

// ------------------------------------------------------------------ helpers

fn to_ms(t: Timestamp) -> i64 {
    (t.unix_timestamp_nanos() / 1_000_000).clamp(i64::MIN as i128, i64::MAX as i128) as i64
}

fn from_ms(ms: i64) -> Timestamp {
    OffsetDateTime::from_unix_timestamp_nanos(i128::from(ms) * 1_000_000)
        .unwrap_or(OffsetDateTime::UNIX_EPOCH)
}

fn add(t: Timestamp, d: Duration) -> Result<Timestamp> {
    let d = time::Duration::try_from(d).map_err(|_| Error::TimeOutOfRange)?;
    t.checked_add(d).ok_or(Error::TimeOutOfRange)
}

fn strip_fragment(uri: &str) -> &str {
    uri.split_once('#').map_or(uri, |(base, _)| base)
}

fn not_found(kind: &'static str, id: impl std::fmt::Display) -> Error {
    Error::NotFound {
        kind,
        id: id.to_string(),
    }
}

fn matcher(pattern: &str) -> Result<GlobMatcher> {
    GlobBuilder::new(pattern)
        .literal_separator(true)
        .build()
        .map(|g| g.compile_matcher())
        .map_err(|source| Error::InvalidGlob {
            pattern: pattern.to_owned(),
            source,
        })
}

fn conversion_error(
    idx: usize,
    e: impl std::error::Error + Send + Sync + 'static,
) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(idx, Type::Text, Box::new(e))
}

fn json_col<T: DeserializeOwned>(row: &Row<'_>, idx: usize) -> rusqlite::Result<T> {
    let text: String = row.get(idx)?;
    serde_json::from_str(&text).map_err(|e| conversion_error(idx, e))
}

/// Serialises a unit-variant enum to its snake_case name, for indexed columns.
fn enum_to_str<T: Serialize>(value: &T) -> String {
    match serde_json::to_value(value) {
        Ok(serde_json::Value::String(s)) => s,
        _ => unreachable!("only unit-variant enums are stored as column strings"),
    }
}

fn enum_from_str<T: DeserializeOwned>(idx: usize, s: String) -> rusqlite::Result<T> {
    serde_json::from_value(serde_json::Value::String(s)).map_err(|e| conversion_error(idx, e))
}

fn append(
    conn: &Connection,
    at: Timestamp,
    task: Option<&TaskId>,
    member: Option<&MemberId>,
    kind: &EventKind,
) -> Result<()> {
    conn.execute(
        "INSERT INTO events(at, task, member, data) VALUES (?1, ?2, ?3, ?4)",
        params![
            to_ms(at),
            task.map(TaskId::as_str),
            member.map(MemberId::as_str),
            serde_json::to_string(kind)?,
        ],
    )?;
    Ok(())
}

fn event_from_row(r: &Row<'_>) -> rusqlite::Result<Event> {
    Ok(Event {
        seq: r.get::<_, i64>(0)?.unsigned_abs(),
        at: from_ms(r.get(1)?),
        task: r.get::<_, Option<String>>(2)?.map(TaskId::new),
        member: r.get::<_, Option<String>>(3)?.map(MemberId::new),
        kind: json_col(r, 4)?,
    })
}

fn reservation_from_row(r: &Row<'_>) -> rusqlite::Result<Reservation> {
    Ok(Reservation {
        pattern: r.get(0)?,
        holder: MemberId::new(r.get::<_, String>(1)?),
        task: r.get::<_, Option<String>>(2)?.map(TaskId::new),
        mode: enum_from_str(3, r.get(3)?)?,
        expires_at: r.get::<_, Option<i64>>(4)?.map(from_ms),
        auto: r.get(5)?,
    })
}

fn member_exists(conn: &Connection, id: &MemberId) -> Result<bool> {
    Ok(conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM members WHERE id = ?1)",
        [id.as_str()],
        |r| r.get(0),
    )?)
}

fn require_member(conn: &Connection, id: &MemberId) -> Result<()> {
    if member_exists(conn, id)? {
        Ok(())
    } else {
        Err(not_found("member", id))
    }
}

fn require_task(conn: &Connection, id: &TaskId) -> Result<()> {
    let exists: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM tasks WHERE id = ?1)",
        [id.as_str()],
        |r| r.get(0),
    )?;
    if exists {
        Ok(())
    } else {
        Err(not_found("task", id))
    }
}

fn get_member(conn: &Connection, id: &MemberId) -> Result<Option<Member>> {
    Ok(conn
        .query_row(
            "SELECT data FROM members WHERE id = ?1",
            [id.as_str()],
            |r| json_col(r, 0),
        )
        .optional()?)
}

fn get_task(conn: &Connection, id: &TaskId) -> Result<Option<Task>> {
    Ok(conn
        .query_row("SELECT data FROM tasks WHERE id = ?1", [id.as_str()], |r| {
            json_col(r, 0)
        })
        .optional()?)
}

fn put_task(conn: &Connection, task: &Task) -> Result<()> {
    conn.execute(
        "UPDATE tasks SET state = ?2, owner = ?3, updated_at = ?4, data = ?5 WHERE id = ?1",
        params![
            task.id.as_str(),
            enum_to_str(&task.state),
            task.owner.as_ref().map(MemberId::as_str),
            to_ms(task.updated_at),
            serde_json::to_string(task)?,
        ],
    )?;
    Ok(())
}

fn get_action(conn: &Connection, id: &ActionId) -> Result<Option<Action>> {
    Ok(conn
        .query_row(
            "SELECT data FROM actions WHERE id = ?1",
            [id.as_str()],
            |r| json_col(r, 0),
        )
        .optional()?)
}

fn put_action(conn: &Connection, a: &Action) -> Result<()> {
    conn.execute(
        "INSERT INTO actions(id, task, member, state, created_at, data)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(id) DO UPDATE SET state = excluded.state, data = excluded.data",
        params![
            a.id.as_str(),
            a.task.as_str(),
            a.member.as_str(),
            enum_to_str(&a.state),
            to_ms(a.created_at),
            serde_json::to_string(a)?,
        ],
    )?;
    Ok(())
}

fn get_hold(conn: &Connection, id: &HoldId) -> Result<Option<Hold>> {
    Ok(conn
        .query_row("SELECT data FROM holds WHERE id = ?1", [id.as_str()], |r| {
            json_col(r, 0)
        })
        .optional()?)
}

fn put_hold(conn: &Connection, h: &Hold) -> Result<()> {
    conn.execute(
        "INSERT INTO holds(id, task, timeout_at, answered_at, created_at, data)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(id) DO UPDATE SET answered_at = excluded.answered_at, data = excluded.data",
        params![
            h.id.as_str(),
            h.task.as_str(),
            h.timeout_at.map(to_ms),
            h.answered_at.map(to_ms),
            to_ms(h.created_at),
            serde_json::to_string(h)?,
        ],
    )?;
    Ok(())
}

fn check_answer(hold: &HoldId, options: &[crate::model::HoldOption], answer: &str) -> Result<()> {
    if options.is_empty() || options.iter().any(|o| o.key == answer) {
        Ok(())
    } else {
        Err(Error::InvalidAnswer {
            hold: hold.to_string(),
            answer: answer.to_owned(),
        })
    }
}

fn apply_answer(
    conn: &Connection,
    hold: &mut Hold,
    answer: &str,
    at: Timestamp,
    defaulted: bool,
) -> Result<()> {
    if hold.answer.is_some() {
        return Err(Error::InvalidState(format!(
            "hold {} was already answered",
            hold.id
        )));
    }
    check_answer(&hold.id, &hold.options, answer)?;
    hold.answer = Some(answer.to_owned());
    hold.answered_at = Some(at);
    put_hold(conn, hold)?;
    append(
        conn,
        at,
        Some(&hold.task),
        None,
        &EventKind::HoldAnswered {
            hold: hold.id.clone(),
            answer: answer.to_owned(),
            defaulted,
        },
    )
}

fn insert_write(
    conn: &Connection,
    member: &MemberId,
    task: &TaskId,
    uri: &str,
    at: Timestamp,
) -> Result<()> {
    conn.execute(
        "INSERT INTO writes(member, task, uri, at) VALUES (?1, ?2, ?3, ?4)",
        params![
            member.as_str(),
            task.as_str(),
            strip_fragment(uri),
            to_ms(at)
        ],
    )?;
    Ok(())
}

/// Deletes the reservations matching `filter` and logs a release for each.
fn delete_reservations(
    conn: &Connection,
    at: Timestamp,
    filter: &str,
    args: impl rusqlite::Params,
) -> Result<usize> {
    delete_and_log(conn, at, filter, args, false)
}

/// Removes lapsed reservations and logs each as expired.
fn sweep(conn: &Connection, now: Timestamp) -> Result<usize> {
    delete_and_log(
        conn,
        now,
        "expires_at IS NOT NULL AND expires_at <= ?1",
        [to_ms(now)],
        true,
    )
}

fn delete_and_log(
    conn: &Connection,
    at: Timestamp,
    filter: &str,
    args: impl rusqlite::Params,
    expired: bool,
) -> Result<usize> {
    let rows: Vec<(i64, String, String, Option<String>)> = {
        let mut stmt = conn.prepare(&format!(
            "SELECT id, pattern, holder, task FROM reservations WHERE {filter} ORDER BY id"
        ))?;
        stmt.query_map(args, |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
            .collect::<rusqlite::Result<_>>()?
    };
    for (id, pattern, holder, task) in &rows {
        conn.execute("DELETE FROM reservations WHERE id = ?1", [id])?;
        append(
            conn,
            at,
            task.clone().map(TaskId::new).as_ref(),
            Some(&MemberId::new(holder.clone())),
            &EventKind::ReservationReleased {
                pattern: pattern.clone(),
                expired,
            },
        )?;
    }
    Ok(rows.len())
}
