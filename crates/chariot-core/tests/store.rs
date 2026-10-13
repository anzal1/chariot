use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::Duration;

use chariot_core::{
    ActionOp, ActionState, ClaimError, Conflict, Error, Event, EventKind, HoldKind, HoldOption,
    Level, Member, MemberId, MemberKind, Mode, NewAction, NewHold, NewTask, Store, Surface, Task,
    TaskId, TaskResult, TaskState, Timestamp, WriteCheck,
};
use time::OffsetDateTime;

const MIN: Duration = Duration::from_secs(60);

/// A store whose clock only moves when the test says so.
struct Fixture {
    store: Store,
    now_ms: Arc<AtomicI64>,
}

impl Fixture {
    fn new() -> Self {
        let mut store = Store::open_in_memory().unwrap();
        let now_ms = Arc::new(AtomicI64::new(1_800_000_000_000));
        wire_clock(&mut store, &now_ms);
        Self { store, now_ms }
    }

    fn advance(&self, d: Duration) {
        self.now_ms
            .fetch_add(d.as_millis() as i64, Ordering::SeqCst);
    }

    fn now(&self) -> Timestamp {
        self.store.now()
    }

    fn member(&mut self, id: &str) -> MemberId {
        add_member(&mut self.store, id)
    }

    fn task(&mut self, goal: &str) -> TaskId {
        self.store
            .create_task(NewTask {
                goal: goal.into(),
                ..NewTask::default()
            })
            .unwrap()
            .id
    }
}

fn wire_clock(store: &mut Store, now_ms: &Arc<AtomicI64>) {
    let clock = Arc::clone(now_ms);
    store.set_clock(move || {
        OffsetDateTime::from_unix_timestamp_nanos(
            i128::from(clock.load(Ordering::SeqCst)) * 1_000_000,
        )
        .unwrap()
    });
}

fn add_member(store: &mut Store, id: &str) -> MemberId {
    let member = Member {
        id: MemberId::new(id),
        name: id.to_owned(),
        kind: MemberKind::Harness,
        surface: Surface::Protocol,
        level: Level::L2,
        live: true,
    };
    store.add_member(&member).unwrap();
    member.id
}

fn conflict(err: ClaimError) -> Conflict {
    match err {
        ClaimError::Conflict(c) => c,
        other => panic!("expected a conflict, got {other:?}"),
    }
}

// ------------------------------------------------------------------ members

#[test]
fn members_roundtrip_and_reject_duplicates() {
    let mut fx = Fixture::new();
    let a = fx.member("claude-code");
    assert_eq!(fx.store.member(&a).unwrap().unwrap().name, "claude-code");
    assert!(fx.store.member(&MemberId::new("nobody")).unwrap().is_none());
    fx.member("opencode");
    assert_eq!(fx.store.members().unwrap().len(), 2);

    let dup = Member {
        id: a.clone(),
        name: "x".into(),
        kind: MemberKind::Human,
        surface: Surface::Terminal,
        level: Level::L0,
        live: false,
    };
    assert!(matches!(
        fx.store.add_member(&dup),
        Err(Error::Exists { .. })
    ));

    fx.store.set_member_live(&a, false).unwrap();
    assert!(!fx.store.member(&a).unwrap().unwrap().live);
}

// -------------------------------------------------------------------- tasks

#[test]
fn task_lifecycle_and_filter() {
    let mut fx = Fixture::new();
    let m = fx.member("claude-code");
    let t1 = fx.task("one");
    let t2 = fx.task("two");
    fx.store.set_owner(&t1, Some(&m)).unwrap();
    fx.store.set_task_state(&t1, TaskState::Working).unwrap();

    let working = fx.store.tasks(Some(TaskState::Working)).unwrap();
    assert_eq!(working.len(), 1);
    assert_eq!(working[0].id, t1);
    assert_eq!(working[0].owner.as_ref(), Some(&m));
    assert_eq!(fx.store.tasks(None).unwrap().len(), 2);
    assert_eq!(fx.store.tasks(Some(TaskState::Queued)).unwrap()[0].id, t2);

    let missing = fx
        .store
        .set_task_state(&TaskId::new("nope"), TaskState::Done);
    assert!(matches!(missing, Err(Error::NotFound { .. })));
    let bad_owner = fx.store.set_owner(&t2, Some(&MemberId::new("ghost")));
    assert!(matches!(bad_owner, Err(Error::NotFound { .. })));
}

#[test]
fn sub_task_requires_existing_parent() {
    let mut fx = Fixture::new();
    let parent = fx.task("parent");
    let child = fx
        .store
        .create_task(NewTask {
            goal: "child".into(),
            parent: Some(parent.clone()),
            ..NewTask::default()
        })
        .unwrap();
    assert_eq!(child.parent, Some(parent));
    let orphan = fx.store.create_task(NewTask {
        goal: "orphan".into(),
        parent: Some(TaskId::new("missing")),
        ..NewTask::default()
    });
    assert!(matches!(orphan, Err(Error::NotFound { .. })));
}

// ------------------------------------------------------------------ actions

#[test]
fn action_decision_and_completion() {
    let mut fx = Fixture::new();
    let m = fx.member("claude-code");
    let t = fx.task("t");
    let action = fx
        .store
        .propose_action(NewAction {
            task: t.clone(),
            member: m.clone(),
            resource: "file:///repo/a.js".into(),
            op: ActionOp::Edit,
            preview: Some("@@ -1 +1 @@".into()),
            reversible: true,
        })
        .unwrap();
    assert_eq!(action.state, ActionState::Proposed);

    let decided = fx
        .store
        .decide_action(&action.id, true, Some("ok"))
        .unwrap();
    assert_eq!(decided.state, ActionState::Approved);
    assert!(matches!(
        fx.store.decide_action(&action.id, false, None),
        Err(Error::InvalidState(_))
    ));

    let done = fx.store.complete_action(&action.id).unwrap();
    assert_eq!(done.state, ActionState::Done);
    assert!(matches!(
        fx.store.complete_action(&action.id),
        Err(Error::InvalidState(_))
    ));

    // The completed edit made the member a recent writer.
    let other = fx.member("opencode");
    let writers = fx
        .store
        .recent_writers("file:///repo/a.js", 10 * MIN, &other)
        .unwrap();
    assert_eq!(writers.len(), 1);
    assert_eq!(writers[0].0, m);

    let denied = fx
        .store
        .propose_action(NewAction {
            task: t,
            member: m,
            resource: "https://api.example.com/refund".into(),
            op: ActionOp::Send,
            preview: None,
            reversible: false,
        })
        .unwrap();
    let denied = fx
        .store
        .decide_action(&denied.id, false, Some("no"))
        .unwrap();
    assert_eq!(denied.state, ActionState::Denied);
}

// ------------------------------------------------------------- reservations

#[test]
fn write_write_same_file_conflicts() {
    let mut fx = Fixture::new();
    let a = fx.member("a");
    let b = fx.member("b");
    let uri = "file:///repo/a.js";
    fx.store.claim(&a, None, uri, Mode::Write, None).unwrap();
    let err = fx
        .store
        .claim(&b, None, uri, Mode::Write, None)
        .unwrap_err();
    let c = conflict(err);
    assert_eq!(c.holder, a);
    assert_eq!(c.pattern, uri);
}

#[test]
fn glob_versus_file_conflicts_both_ways() {
    let mut fx = Fixture::new();
    let a = fx.member("a");
    let b = fx.member("b");
    let c = fx.member("c");

    fx.store
        .claim(&a, None, "file:///repo/src/**", Mode::Write, None)
        .unwrap();
    let err = fx
        .store
        .claim(&b, None, "file:///repo/src/calc.js", Mode::Write, None)
        .unwrap_err();
    assert_eq!(conflict(err).holder, a);

    // Overlapping glob against glob.
    let err = fx
        .store
        .claim(&c, None, "file:///repo/src/*.js", Mode::Write, None)
        .unwrap_err();
    assert_eq!(conflict(err).pattern, "file:///repo/src/**");

    // File first, glob second.
    fx.store
        .claim(&b, None, "file:///repo/docs/readme.md", Mode::Write, None)
        .unwrap();
    let err = fx
        .store
        .claim(&c, None, "file:///repo/docs/**", Mode::Write, None)
        .unwrap_err();
    assert_eq!(conflict(err).holder, b);

    // Disjoint trees are fine, and `*` does not cross a slash.
    fx.store
        .claim(&c, None, "file:///repo/test/**", Mode::Write, None)
        .unwrap();
    fx.store
        .claim(&c, None, "file:///repo/*.toml", Mode::Write, None)
        .unwrap();
    fx.store
        .claim(&a, None, "file:///repo/sub/x.toml", Mode::Write, None)
        .unwrap();
}

#[test]
fn read_and_write_do_not_conflict() {
    let mut fx = Fixture::new();
    let a = fx.member("a");
    let b = fx.member("b");
    let c = fx.member("c");
    fx.store
        .claim(&a, None, "file:///repo/src/**", Mode::Write, None)
        .unwrap();
    fx.store
        .claim(&b, None, "file:///repo/src/a.js", Mode::Read, None)
        .unwrap();
    fx.store
        .claim(&c, None, "file:///repo/src/**", Mode::Read, None)
        .unwrap();
    // Reads held by others do not block a write claim either.
    let d = fx.member("d");
    fx.store
        .claim(&d, None, "file:///repo/lib/**", Mode::Read, None)
        .unwrap();
    fx.store
        .claim(&a, None, "file:///repo/lib/**", Mode::Write, None)
        .unwrap();
    assert_eq!(fx.store.reservations().unwrap().len(), 5);
}

#[test]
fn expired_reservations_are_ignored_and_cleaned() {
    let mut fx = Fixture::new();
    let a = fx.member("a");
    let b = fx.member("b");
    let uri = "file:///repo/a.js";
    fx.store
        .claim(&a, None, uri, Mode::Write, Some(5 * MIN))
        .unwrap();

    fx.advance(4 * MIN);
    assert_eq!(
        fx.store.check_write(&b, uri).unwrap(),
        WriteCheck::HeldBy(a.clone())
    );
    assert!(fx.store.claim(&b, None, uri, Mode::Write, None).is_err());

    fx.advance(MIN); // exactly at expiry counts as expired
    assert_eq!(fx.store.check_write(&b, uri).unwrap(), WriteCheck::Allowed);
    assert!(fx.store.reservations().unwrap().is_empty());
    fx.store.claim(&b, None, uri, Mode::Write, None).unwrap();

    let reservations = fx.store.reservations().unwrap();
    assert_eq!(reservations.len(), 1);
    assert_eq!(reservations[0].holder, b);

    // The lapse is on the record.
    let expired = fx
        .store
        .events(0)
        .unwrap()
        .into_iter()
        .filter(|e| matches!(e.kind, EventKind::ReservationReleased { expired: true, .. }))
        .count();
    assert_eq!(expired, 1);
}

#[test]
fn same_holder_reclaim_refreshes() {
    let mut fx = Fixture::new();
    let a = fx.member("a");
    let b = fx.member("b");
    let uri = "file:///repo/a.js";
    let first = fx
        .store
        .claim(&a, None, uri, Mode::Write, Some(5 * MIN))
        .unwrap();
    fx.advance(4 * MIN);
    let second = fx
        .store
        .claim(&a, None, uri, Mode::Write, Some(5 * MIN))
        .unwrap();
    assert!(second.expires_at > first.expires_at);
    assert_eq!(fx.store.reservations().unwrap().len(), 1);

    fx.advance(2 * MIN); // past the first expiry, inside the refreshed one
    assert_eq!(
        fx.store.check_write(&b, uri).unwrap(),
        WriteCheck::HeldBy(a.clone())
    );

    // Overlap with one's own wider reservation is fine too.
    fx.store
        .claim(&a, None, "file:///repo/**", Mode::Write, None)
        .unwrap();
}

#[test]
fn check_write_matches_globs_and_ignores_fragments() {
    let mut fx = Fixture::new();
    let a = fx.member("a");
    let b = fx.member("b");
    fx.store
        .claim(&a, None, "file:///repo/src/**", Mode::Write, None)
        .unwrap();
    fx.store
        .claim(&b, None, "file:///repo/docs/**", Mode::Read, None)
        .unwrap();

    assert_eq!(
        fx.store
            .check_write(&b, "file:///repo/src/deep/x.rs#L10-40")
            .unwrap(),
        WriteCheck::HeldBy(a.clone())
    );
    // The holder itself is never blocked by its own reservation.
    assert_eq!(
        fx.store.check_write(&a, "file:///repo/src/x.rs").unwrap(),
        WriteCheck::Allowed
    );
    assert_eq!(
        fx.store.check_write(&b, "file:///repo/other.rs").unwrap(),
        WriteCheck::Allowed
    );
    // Read reservations do not gate writes.
    assert_eq!(
        fx.store.check_write(&a, "file:///repo/docs/a.md").unwrap(),
        WriteCheck::Allowed
    );
}

#[test]
fn release_and_auto_flag() {
    let mut fx = Fixture::new();
    let a = fx.member("a");
    let b = fx.member("b");
    let t = fx.task("t");
    let auto = fx
        .store
        .claim_auto(&a, Some(&t), "file:///repo/a.js", Mode::Write, None)
        .unwrap();
    assert!(auto.auto);
    assert_eq!(auto.task, Some(t.clone()));
    assert!(
        !fx.store
            .claim(&a, None, "file:///repo/b.js", Mode::Write, None)
            .unwrap()
            .auto
    );

    assert!(fx.store.release(&a, "file:///repo/b.js").unwrap());
    assert!(!fx.store.release(&a, "file:///repo/b.js").unwrap());
    assert!(
        fx.store
            .claim(&b, None, "file:///repo/b.js", Mode::Write, None)
            .is_ok()
    );

    assert_eq!(fx.store.release_task(&t).unwrap(), 1);
    assert!(
        fx.store
            .claim(&b, None, "file:///repo/a.js", Mode::Write, None)
            .is_ok()
    );
}

#[test]
fn invalid_glob_is_rejected() {
    let mut fx = Fixture::new();
    let a = fx.member("a");
    let err = fx
        .store
        .claim(&a, None, "file:///repo/[", Mode::Write, None)
        .unwrap_err();
    assert!(matches!(err, ClaimError::Store(Error::InvalidGlob { .. })));
}

// -------------------------------------------------------------------- holds

fn yes_no() -> Vec<HoldOption> {
    vec![
        HoldOption {
            key: "y".into(),
            label: "allow".into(),
        },
        HoldOption {
            key: "n".into(),
            label: "deny".into(),
        },
    ]
}

fn new_hold(task: &TaskId, default: Option<&str>, timeout: Option<Duration>) -> NewHold {
    NewHold {
        kind: HoldKind::HeldEdit,
        task: task.clone(),
        question: "OpenCode wants to rewrite calc.js lines 10-40".into(),
        options: yes_no(),
        default_on_timeout: default.map(str::to_owned),
        timeout,
    }
}

#[test]
fn hold_lifecycle_open_answer() {
    let mut fx = Fixture::new();
    let t = fx.task("t");
    let hold = fx
        .store
        .open_hold(new_hold(&t, Some("n"), Some(5 * MIN)))
        .unwrap();
    assert_eq!(hold.timeout_at, Some(fx.now() + 5 * MIN));
    assert_eq!(fx.store.open_holds().unwrap().len(), 1);

    assert!(matches!(
        fx.store.answer_hold(&hold.id, "maybe"),
        Err(Error::InvalidAnswer { .. })
    ));
    let answered = fx.store.answer_hold(&hold.id, "y").unwrap();
    assert_eq!(answered.answer.as_deref(), Some("y"));
    assert_eq!(answered.answered_at, Some(fx.now()));
    assert!(fx.store.open_holds().unwrap().is_empty());
    assert!(matches!(
        fx.store.answer_hold(&hold.id, "n"),
        Err(Error::InvalidState(_))
    ));
    assert_eq!(
        fx.store.hold(&hold.id).unwrap().unwrap().answer.as_deref(),
        Some("y")
    );
}

#[test]
fn hold_default_is_validated_against_options() {
    let mut fx = Fixture::new();
    let t = fx.task("t");
    assert!(matches!(
        fx.store.open_hold(new_hold(&t, Some("zzz"), None)),
        Err(Error::InvalidAnswer { .. })
    ));
}

#[test]
fn hold_timeout_applies_default_and_leaves_others() {
    let mut fx = Fixture::new();
    let t = fx.task("t");
    let with_default = fx
        .store
        .open_hold(new_hold(&t, Some("n"), Some(5 * MIN)))
        .unwrap();
    let no_default = fx
        .store
        .open_hold(new_hold(&t, None, Some(5 * MIN)))
        .unwrap();
    let later = fx
        .store
        .open_hold(new_hold(&t, Some("y"), Some(30 * MIN)))
        .unwrap();
    let no_timeout = fx.store.open_hold(new_hold(&t, Some("y"), None)).unwrap();

    fx.advance(4 * MIN);
    assert!(fx.store.expire_holds(fx.now()).unwrap().is_empty());

    fx.advance(MIN);
    let expired = fx.store.expire_holds(fx.now()).unwrap();
    assert_eq!(expired.len(), 1);
    assert_eq!(expired[0].id, with_default.id);
    assert_eq!(expired[0].answer.as_deref(), Some("n"));

    let open: Vec<_> = fx
        .store
        .open_holds()
        .unwrap()
        .into_iter()
        .map(|h| h.id)
        .collect();
    assert_eq!(open, vec![no_default.id, later.id, no_timeout.id]);

    // Running it again is a no-op.
    assert!(fx.store.expire_holds(fx.now()).unwrap().is_empty());

    let defaulted = fx
        .store
        .events_for_task(&t)
        .unwrap()
        .into_iter()
        .filter(|e| {
            matches!(
                e.kind,
                EventKind::HoldAnswered {
                    defaulted: true,
                    ..
                }
            )
        })
        .count();
    assert_eq!(defaulted, 1);
}

// ------------------------------------------------------------ events and log

#[test]
fn event_log_is_ordered_and_filterable() {
    let mut fx = Fixture::new();
    let a = fx.member("a");
    let b = fx.member("b");
    let t1 = fx.task("one");
    let t2 = fx.task("two");
    fx.store.set_owner(&t1, Some(&a)).unwrap();
    fx.advance(Duration::from_secs(1));
    fx.store.set_task_state(&t1, TaskState::Working).unwrap();
    fx.store
        .claim(&a, Some(&t1), "file:///repo/x.js", Mode::Write, None)
        .unwrap();
    let denied = fx
        .store
        .claim(&b, Some(&t2), "file:///repo/x.js", Mode::Write, None);
    assert!(denied.is_err());
    fx.store.note(Some(&t2), Some(&b), "waiting").unwrap();
    let hold = fx.store.open_hold(new_hold(&t2, None, None)).unwrap();
    fx.store.answer_hold(&hold.id, "y").unwrap();
    fx.store
        .record_result(&TaskResult {
            task: t1.clone(),
            status: TaskState::Review,
            content: "done".into(),
            evidence: vec![],
            usage: None,
        })
        .unwrap();

    let all = fx.store.events(0).unwrap();
    let seqs: Vec<u64> = all.iter().map(|e| e.seq).collect();
    assert_eq!(seqs, (1..=all.len() as u64).collect::<Vec<_>>());
    assert!(all.windows(2).all(|w| w[0].at <= w[1].at));

    // Exactly the events one would expect, in order.
    let kinds: Vec<&str> = all.iter().map(|e| kind_name(&e.kind)).collect();
    assert_eq!(
        kinds,
        [
            "member_added",
            "member_added",
            "task_created",
            "task_created",
            "task_owner_changed",
            "task_state_changed",
            "reservation_taken",
            "reservation_denied",
            "note",
            "hold_opened",
            "hold_answered",
            "result_recorded",
        ]
    );

    // The denial belongs to the requester and the requester's task.
    let denial = all
        .iter()
        .find(|e| kind_name(&e.kind) == "reservation_denied")
        .unwrap();
    assert_eq!(denial.member.as_ref(), Some(&b));
    assert_eq!(denial.task.as_ref(), Some(&t2));
    assert!(matches!(&denial.kind, EventKind::ReservationDenied { holder, .. } if *holder == a));

    // since is exclusive.
    let tail = fx.store.events(all[9].seq).unwrap();
    assert_eq!(tail.len(), 2);

    // Per-task filter.
    let for_t1: Vec<&str> = fx
        .store
        .events_for_task(&t1)
        .unwrap()
        .iter()
        .map(|e| kind_name(&e.kind))
        .collect();
    assert_eq!(
        for_t1,
        [
            "task_created",
            "task_owner_changed",
            "task_state_changed",
            "reservation_taken",
            "result_recorded"
        ]
    );
    let for_t2 = fx.store.events_for_task(&t2).unwrap();
    assert_eq!(for_t2.len(), 5);
    assert!(for_t2.iter().all(|e| e.task.as_ref() == Some(&t2)));
}

fn kind_name(kind: &EventKind) -> &'static str {
    match kind {
        EventKind::MemberAdded { .. } => "member_added",
        EventKind::MemberLiveChanged { .. } => "member_live_changed",
        EventKind::TaskCreated { .. } => "task_created",
        EventKind::TaskStateChanged { .. } => "task_state_changed",
        EventKind::TaskOwnerChanged { .. } => "task_owner_changed",
        EventKind::ActionProposed { .. } => "action_proposed",
        EventKind::ActionDecided { .. } => "action_decided",
        EventKind::ActionDone { .. } => "action_done",
        EventKind::WriteRecorded { .. } => "write_recorded",
        EventKind::ReservationTaken { .. } => "reservation_taken",
        EventKind::ReservationReleased { .. } => "reservation_released",
        EventKind::ReservationDenied { .. } => "reservation_denied",
        EventKind::HoldOpened { .. } => "hold_opened",
        EventKind::HoldAnswered { .. } => "hold_answered",
        EventKind::ResultRecorded { .. } => "result_recorded",
        EventKind::Note { .. } => "note",
    }
}

#[test]
fn failed_mutation_leaves_no_event() {
    let mut fx = Fixture::new();
    let before = fx.store.events(0).unwrap().len();
    assert!(
        fx.store
            .set_task_state(&TaskId::new("nope"), TaskState::Done)
            .is_err()
    );
    assert!(fx.store.set_owner(&TaskId::new("nope"), None).is_err());
    assert_eq!(fx.store.events(0).unwrap().len(), before);
}

#[test]
fn events_serialise_to_json_and_back() {
    let mut fx = Fixture::new();
    let a = fx.member("a");
    fx.store.note(None, Some(&a), "hello").unwrap();
    for event in fx.store.events(0).unwrap() {
        let json = serde_json::to_string(&event).unwrap();
        let back: Event = serde_json::from_str(&json).unwrap();
        assert_eq!(back, event);
    }
}

// ----------------------------------------------------------- revert watch

#[test]
fn recent_writers_respects_window_and_exclusion() {
    let mut fx = Fixture::new();
    let a = fx.member("a");
    let b = fx.member("b");
    let c = fx.member("c");
    let t9 = fx.task("t9");
    let t12 = fx.task("t12");
    let uri = "file:///repo/calc.js";

    fx.store.record_write(&a, &t9, uri).unwrap();
    fx.advance(8 * MIN);
    fx.store
        .record_write(&b, &t12, "file:///repo/calc.js#L10-40")
        .unwrap();
    fx.store
        .record_write(&b, &t12, "file:///repo/other.js")
        .unwrap();

    // Both in the window, newest first.
    let w = fx.store.recent_writers(uri, 10 * MIN, &c).unwrap();
    assert_eq!(w.iter().map(|x| &x.0).collect::<Vec<_>>(), [&b, &a]);
    assert_eq!(w[0].1, t12);
    assert_eq!(w[0].2, fx.now());

    // Tighter window drops the older write.
    let w = fx.store.recent_writers(uri, 5 * MIN, &c).unwrap();
    assert_eq!(w.len(), 1);
    assert_eq!(w[0].0, b);

    // Exactly on the edge is inside the window.
    assert_eq!(fx.store.recent_writers(uri, 8 * MIN, &c).unwrap().len(), 2);

    // The asker never sees itself.
    let w = fx.store.recent_writers(uri, 10 * MIN, &b).unwrap();
    assert_eq!(w.len(), 1);
    assert_eq!(w[0].0, a);
    assert_eq!(w[0].1, t9);

    // Other files are unaffected, and time moving on empties the answer.
    assert!(
        fx.store
            .recent_writers("file:///repo/none.js", 10 * MIN, &c)
            .unwrap()
            .is_empty()
    );
    fx.advance(10 * MIN);
    assert!(
        fx.store
            .recent_writers(uri, 10 * MIN - Duration::from_secs(1), &c)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn repeated_writes_collapse_to_one_row_per_member_and_task() {
    let mut fx = Fixture::new();
    let a = fx.member("a");
    let c = fx.member("c");
    let t = fx.task("t");
    let uri = "file:///repo/calc.js";
    fx.store.record_write(&a, &t, uri).unwrap();
    fx.advance(MIN);
    fx.store.record_write(&a, &t, uri).unwrap();
    let w = fx.store.recent_writers(uri, 10 * MIN, &c).unwrap();
    assert_eq!(w.len(), 1);
    assert_eq!(w[0].2, fx.now());
}

#[test]
fn only_writing_ops_count_as_writes() {
    let mut fx = Fixture::new();
    let a = fx.member("a");
    let b = fx.member("b");
    let t = fx.task("t");
    for op in [ActionOp::Fetch, ActionOp::Execute] {
        let act = fx
            .store
            .propose_action(NewAction {
                task: t.clone(),
                member: a.clone(),
                resource: "file:///repo/calc.js".into(),
                op,
                preview: None,
                reversible: true,
            })
            .unwrap();
        fx.store.complete_action(&act.id).unwrap();
    }
    assert!(
        fx.store
            .recent_writers("file:///repo/calc.js", 10 * MIN, &b)
            .unwrap()
            .is_empty()
    );
}

// ------------------------------------------------------------- persistence

#[test]
fn state_survives_reopen() {
    let dir = std::env::temp_dir().join(format!("chariot-core-test-{}", uuid_like()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("chariot.db");

    let (a, task, hold_id, events_before): (MemberId, Task, _, _);
    {
        let mut store = Store::open(&path).unwrap();
        a = add_member(&mut store, "a");
        task = store
            .create_task(NewTask {
                goal: "persist me".into(),
                done: vec!["tests pass".into()],
                ..NewTask::default()
            })
            .unwrap();
        store.set_task_state(&task.id, TaskState::Working).unwrap();
        store
            .claim(&a, Some(&task.id), "file:///repo/**", Mode::Write, None)
            .unwrap();
        hold_id = store
            .open_hold(new_hold(&task.id, Some("n"), Some(MIN)))
            .unwrap()
            .id;
        store
            .record_write(&a, &task.id, "file:///repo/x.js")
            .unwrap();
        events_before = store.events(0).unwrap();
    }

    let mut store = Store::open(&path).unwrap(); // schema creation is idempotent
    let loaded = store.task(&task.id).unwrap().unwrap();
    assert_eq!(loaded.goal, "persist me");
    assert_eq!(loaded.state, TaskState::Working);
    assert_eq!(loaded.done, vec!["tests pass".to_owned()]);
    assert_eq!(store.members().unwrap().len(), 1);
    assert_eq!(store.reservations().unwrap().len(), 1);
    assert_eq!(store.open_holds().unwrap()[0].id, hold_id);
    assert_eq!(store.events(0).unwrap(), events_before);

    let b = add_member(&mut store, "b");
    assert_eq!(
        store.check_write(&b, "file:///repo/y.js").unwrap(),
        WriteCheck::HeldBy(a.clone())
    );
    assert_eq!(
        store
            .recent_writers("file:///repo/x.js", MIN, &b)
            .unwrap()
            .len(),
        1
    );

    // New events continue the sequence.
    store.note(None, None, "after reopen").unwrap();
    let last = store.events(0).unwrap().last().unwrap().seq;
    assert_eq!(last, events_before.last().unwrap().seq + 2); // add_member b, then the note

    drop(store);
    std::fs::remove_dir_all(&dir).unwrap();
}

fn uuid_like() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("{}-{nanos}", std::process::id())
}

#[test]
fn results_are_stored_per_task() {
    let mut fx = Fixture::new();
    let t = fx.task("t");
    let result = TaskResult {
        task: t.clone(),
        status: TaskState::Review,
        content: "added sub()".into(),
        evidence: vec![chariot_core::Evidence {
            kind: chariot_core::EvidenceKind::Test,
            detail: "14 passed".into(),
        }],
        usage: Some(chariot_core::Usage {
            tokens_in: 1000,
            tokens_out: 200,
            quota_left_pct: Some(61.5),
        }),
    };
    fx.store.record_result(&result).unwrap();
    assert_eq!(fx.store.results(&t).unwrap(), vec![result]);
}
