# Chariot: the product

Status: direction agreed 2026-10-13. ARCHITECTURE.md is the engine, ABSTRACTIONS.md the vocabulary, this file is what people use and why they switch.

## The pitch

**Keep every agent you love. Chariot makes them one team.** Your agents stop wiping each other's work, "done" means verified done, a subscription that runs out hands over instead of stalling, and anything can join: a CLI, an SDK agent, a local model, a website.

**Love T3 Code? Bring it over. We manage it too.** Chariot does not ask anyone to give up their favourite harness or control surface. T3 Code, OpenCode, Claude Code, Codex, Cursor, Antigravity, Goose, a personal SDK agent: each joins as a member. Chariot is the layer underneath that none of them carry.

## What makes it different

Checked against T3 Code, Zed, Cursor, Conductor, Superset, Warp, the Codex app, Antigravity and Claude's desktop app (2026-10-13). Chat, threads, devices and multi-vendor support are crowded. These are not:

| Capability | Status elsewhere | Chariot |
|---|---|---|
| Agents on the same files without clobbering | Nobody | Reservations, held edits, revert detection |
| Contradicting tasks caught before work starts | Nobody | Intake check (14/14 in tests) |
| "Done" verified against the final state | Nobody fully | Acceptance check (caught 17/17 false "done") |
| Failover across subscriptions and harnesses | T3 waits for reset; Claudexor rotates accounts | Hands the task to another member with context |
| Onboarding any CLI, SDK or website | ACP only, or hand-written adapters | Onboarding agent writes the driver (3/3 first try) |
| An autonomous god model with guardrails | Delegation without guardrails | Decides by default; holds and `chariot why` keep it honest |
| Native GPU UI | Mostly Electron | GPUI |

## The app

**Home is the team view.** Not a chat list: the god model, what it decided and why, every member and what it is doing, holds that need you, quota per subscription, verified results.

| Part | What it is |
|---|---|
| Team view | God model decisions, members, live status, holds, quota, verified "done" |
| Threads and chat | One thread per task, carrying its reservations, contradiction check and verification |
| Editor | Built in and good (gpui-kit editor), with one key to open the file in Zed or VS Code |
| Review | Diffs per task and per member, with who wrote what and the verifier's verdict |
| Terminal | Per member, sandboxed or live |
| Devices | Other machines and phone control, later (T3 already has this, so it is not the lead) |
| Docs | Built-in docs for every concept, written for users first |

## Widgets and the things harnesses do not carry forward

The core runs as a daemon, so these work even when the app window is closed.

| Feature | What it does |
|---|---|
| Island bar | A notch-style island showing what the team is doing, the next hold, and quota at a glance. Answer a hold from the island with one key. Builds on what hum's notch island taught us |
| Keep awake | Holds a power assertion while any member is working, so the laptop does not sleep mid-task; releases the moment the team is idle |
| Menu bar | Quota per subscription with reset countdowns, team status, pause all |
| Global hotkey | Answer the current hold from anywhere |
| Actionable notifications | Allow, deny or hand off straight from the notification |
| Survives everything | Sleep, lid close, network loss, app quit and crashes: work is queued, resumed and never silently dropped |
| Battery aware | Slows the pace on battery, full speed on power |
| One ledger | Usage and cost across every subscription and API key in one place |
| Replay | Every task's timeline, step by step, from the event log |

## Build order (v0)

1. `chariot-core`: types, store, event log, reservations, holds
2. `chariot-drivers`: ACP bridge (from the spike), driver-spec runner
3. `chariotd`: the daemon, local socket, keep-awake
4. `chariot-app`: the team view, then threads and chat, then review and editor
5. Island bar and menu bar
6. Intake check, acceptance check and the god model
7. Quota meter and failover
8. Onboarding agent and conformance, then T3 Code as a member
