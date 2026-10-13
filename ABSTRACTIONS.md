# God Tier: the abstraction on top

Status: design draft, 2026-10-12. Companion to ARCHITECTURE.md. That document is the engine. This one is the steering wheel: what a user touches, what a developer extends, and what stays under the hood.

The promise, in one line: **keep using your AI tools exactly as you do now; they stop wiping each other's work, hand over when one runs out, and tell you the truth about what got done.**

---

## 1. What the user sees

Four nouns, four verbs. Nothing else is ever required.

| Noun | Meaning | What it hides |
|---|---|---|
| **Member** | Anything that can do work: Claude Code, OpenCode, a local model, a script, you | Manifest, driver, tier, level, certification, scores |
| **Task** | A goal plus what "done" means | Task contract, resources, budget, handoff pack, review settings |
| **Hold** | Something waiting on you, always answerable with one key | Reservations, action proposals, quota events, verifier verdicts, policy engine |
| **Rule** | One line in `gt.yaml` that changes behaviour | Policy engine, dispatcher weights, pacer |

| Verb | Command | What it does |
|---|---|---|
| **add** | `gt add <thing>` | Onboards a member: discover, write driver, certify, enrol. Two minutes, no questions unless it fails |
| **run** | `gt run "<goal>"` | Creates a task, suggests a member, starts it. Optional: existing sessions become tasks by themselves |
| **ok / no** | `gt ok 12`, `gt no 12` | Answers a hold. Also a key in the board |
| **why** | `gt why 12` | The full record for one task: who, what, when, what was held and why |

`gt board` is a view, not a verb: tasks, members, quota bars, open holds.

### The metaphor: keep it internal

The founder's framing (god hands off, apostles work, everyone prays in their own language) is the right one-sentence explanation of the architecture and it should stay in the README and the project story. It should not be the product vocabulary, for three reasons.

1. "God" implies the core thinks. The design's strongest property is the opposite: the core has no model and never ages. "Team" says that; "god" contradicts it.
2. Vocabulary is read at 2 a.m. in an error message. "Prayer 12 rejected by apostle opencode" costs a second of translation every time. "Task 12 held" costs nothing.
3. Some users will not bring religious framing into a work tool, and a tool that needs adoption by every harness vendor should not carry that friction.

So: codename and story use the metaphor. Surface uses member, task, hold, rule.

---

## 2. The first five minutes

```
$ gt init
Found on this machine:
  Claude Code (Max)        speaks ACP     -> certified L2
  OpenCode (GLM)           speaks ACP     -> certified L2
  Ollama: qwen3, gemma4    local models   -> certified L1 (reference harness)
Wrote a marked rules block to CLAUDE.md and AGENTS.md (28 lines, remove with `gt leave`).
Intake check and done-check will run on Claude Code with Haiku (tested: 14/14 and 17/17). Change with `gt rule`.
Ready. Keep working as usual, or try: gt run "add sub() to calc.js with tests"
```

Nothing else to learn. The user opens Claude Code and OpenCode on the same repo the way they always did. Because the rules block is in the instruction channel (the mechanism that passed 18/18) and the shared tools are injected at session start, both sessions now claim files before editing, message each other when blocked, and report evidence. The user notices only that the second session says "calc.js is held by Claude Code for task 9, I did the rest and left a note" instead of overwriting it.

The first `gt run` shows the one piece of routing the user ever sees:

```
$ gt run "add sub() to calc.js with tests"
Task 12. Suggest: Claude Code (code.write, 61% quota left). Enter to start, or type a member.
```

Suggest mode is the permanent default. Auto mode exists but is earned per task kind and is turned on by a rule, never by itself.

---

## 3. When it needs you

Five moments. Every hold states the default that applies if you ignore it, so nothing stalls silently.

**1. Contradiction at intake** (the judge member, 14/14 in tests)

```
HOLD 14  Two tasks disagree on src/server.ts
  Task 12 (Claude Code): keep Express, add rate limiting
  Task 14 (OpenCode):    replace Express with Fastify
  [1] keep 12, cancel 14   [2] keep 14, cancel 12   [3] both, I will explain
  Ignored: task 14 waits.
```

**2. Quota failover**

```
HOLD 15  Claude Code hit its limit. Back at 17:40.
  Task 12 is about 60% done (3 files, tests not yet run).
  [y] continue on OpenCode   [w] wait for 17:40   [s] stop here
  Ignored: waits. Change the default with rule `on limit: continue`.
```

**3. A held edit** (reservation plus revert detection; the holder is asked first, you only when it objects or does not answer)

```
HOLD 16  OpenCode wants to rewrite calc.js lines 10-40.
  Claude Code wrote those lines 8 minutes ago for task 9 and has not replied in 2 min.
  [y] allow   [n] deny   [d] show diff
  Ignored: denied after 5 min, OpenCode continues with the rest.
```

**4. A failed done-check** (the independent verifier, 17/17 in tests)

```
HOLD 17  Task 12 reported done by Claude Code. Check failed.
  "tests pass": 2 of 14 fail after OpenCode changed utils.js (task 14) at 16:02.
  [r] send back to Claude Code   [o] send to OpenCode   [a] accept anyway   [v] show
  Ignored: stays open on the board. Nothing is marked done.
```

**5. An irreversible action** (cannot be turned off)

```
HOLD 18  OpenCode wants to POST https://api.stripe.com/v1/refunds
  [y] allow once   [n] deny
  Ignored: denied.
```

Holds arrive as one line on the board and one desktop notification. They are never delivered inside a member's own chat: a hold is a decision for you, and a member's chat is where the member, not you, is reading.

### What stays invisible unless you ask

Levels, GT/1 message names, resource URIs, reservation modes and TTLs, handoff pack fields, driver tiers, conformance results, scoreboard numbers, the guard, the sandbox backend, the event log. All of it is one `gt why` away, none of it is on the board.

---

## 4. One file: `gt.yaml`

Everything in it is optional. Delete the file and `gt` still works with defaults.

```yaml
# gt.yaml  (project or ~/.gt/gt.yaml)

members:
  claude-code:  default
  opencode:     when claude-code is out        # failover order, top to bottom
  ollama/qwen3: for [commit-message, summarise]

checks by: claude-code (haiku)   # intake check and done-check; local models untested for this

on limit: ask            # ask | continue | wait

ask me before:
  - edits under db/migrations/**
  - any shell command containing "rm -rf"
  # mail, payments, public posts and https POST are always asked. Cannot be removed.

never ask me about:
  - edits under test/**

review:
  by: a different model than the writer
  required for: edits under src/**

route: suggest           # suggest | auto (auto only where the scoreboard has earned it)
```

Rules are structured on disk and plain in the hand. `gt rule "always ask me before touching migrations"` sends the sentence to the judge member, which writes the structured line above and shows it back for one-key confirmation. Natural language is an input method, never the stored form, so the file is always inspectable and no model sits in the config loop at runtime.

---

## 5. Extending it

Five extension points. Each has a smallest form that fits on a screen.

| Extension | Smallest form | Who writes it |
|---|---|---|
| **Member** | A script: task JSON on stdin, result JSON on stdout | A user, in a minute |
| **Driver** | A YAML spec: how to start a thing and read its output | The onboarding agent, usually. Humans only to repair |
| **Skill** | A markdown file: a task kind, its done criteria, its preferred member | A developer defining team conventions |
| **Rule** | One line in `gt.yaml` | A user |
| **Up-link** | `gt serve --mcp` exposes the whole team as `team.run` and `team.review` | Nobody; it is a flag |

**Member (L0, one method)**

```sh
#!/bin/sh
# summarise.sh: gt add ./summarise.sh
goal=$(jq -r .task.goal)
ollama run qwen3 "$goal" | jq -Rs '{status:"done",content:[{type:"text",text:.}]}'
```

**Driver (headless tier)** (illustrative: Codex event names not yet verified)

```yaml
driver: codex-exec
start: ["codex", "exec", "--json", "{prompt}"]
events:
  - match: {type: "agent_message"}  emit: message
  - match: {type: "token_count"}    emit: usage
result: {match: {type: "task_complete"}}
quota: {signals: ["rate_limit"]}
```

**Skill**

```markdown
---
skill: commit-message
prefer: ollama/qwen3
done: ["one line under 72 chars", "imperative mood", "no trailing period"]
---
Write a conventional commit message from the staged diff. Do not edit files.
```

The done list is what the verifier checks literally. The prefer field is what the dispatcher weighs. The body is appended to the member's instructions for that task.

**Progressive disclosure**

| Step | You touch | You get |
|---|---|---|
| 0 | `gt init` | Safety, failover, done-check, with defaults |
| 1 | `gt.yaml` | Your own asks, failover order, local-model routing |
| 2 | Skills, `gt add` | Named task kinds, new members |
| 3 | Driver specs | Repair or hand-write how a member is driven |
| 4 | Native SDK (30 lines) | A member at L2 or above: proposes actions, accepts injected context |
| 5 | Code driver escape hatch | Anything, sandboxed, no credentials |

Nobody needs step N+1 to benefit from step N.

---

## 6. Nothing proven is hidden

Every mechanism the tests showed to be necessary has a place in the surface. The table is the contract between this document and ARCHITECTURE.md.

| Proven mechanism | Test result | Where it lives in the abstraction | Can the user turn it off |
|---|---|---|---|
| Rules in the instruction channel | 18/18 vs 5/18 | The marked block `gt init` writes to CLAUDE.md and AGENTS.md | Only by `gt leave` |
| Reservations and revert hold | Wiped 2/6 without, 0/12 with (revert detection itself is designed, not yet tested) | Hold type 3. "Held by X for task N" in the member's own words | No. Timeouts are tunable |
| Intake contradiction check | 14/14 blind | Hold type 1. Runs on a cheap member named in `members:` | No. The judge member is swappable |
| Final acceptance check | 17/17 false "done" caught | Hold type 4. The board shows the verdict, never the member's summary | No. "Accept anyway" is per task |
| Quota failover with handoff | Designed, 27% of days affected | Hold type 2, `on limit:` rule | Default is ask |
| Hard gate (L2) as backstop | Added no value over rules when rules were present; members without rules violated almost always, so the gate stays as a backstop (untested on rule-less members) | Invisible. Matters most for members that did not receive the rules block | No |
| Stop when blocked, do not poll | One agent polled to timeout | A line in the rules block plus `gt.wait` in the shared tools | No |

---

## 7. Stable and swappable

| Stable contract (changing it breaks members or users) | Swappable (expected to churn) |
|---|---|
| The task shape: goal, done list, resources touched | Every driver and every member |
| The result shape: status, content, evidence, actions | The judge and verifier models |
| Resource URI scheme and the three reservation modes | Dispatcher scoring and the scoreboard |
| Level definitions L0 to L4 | Which level each harness reaches this month |
| The five hold types and their one-key answers | Board rendering: terminal, web page, editor panel |
| `gt.yaml` top-level keys and the four verbs | Transport bridges (ACP, MCP, A2A, HTTP) |
| The rules block format and marker | The rules block wording |
| L0 stdin and stdout contract | Sandbox backend, quota signal per harness |

Rule of thumb: nouns are stable, implementations are swappable. A user reshapes behaviour only through rules and skills. A developer reshapes capability only through drivers and members. Neither ever edits the core.

---

## 8. Every surface is a member (v0 scope)

Founder decision, 2026-10-12: onboarding of every kind of surface comes first, and the abstraction sits on top of that. A harness of harnesses that only takes CLIs is just another CLI manager. So v0 onboards all surface kinds, and the governance a member gets depends on how much control its surface allows, never on whether it is allowed in.

Two facts make this work:

1. **Rules do not need an instruction file.** In the live trials the rules reached Claude Code through its system prompt flag and OpenCode through the ACP prompt, not through `CLAUDE.md`. Any surface that accepts text can receive the rules as a prefix of the task the core sends it.
2. **Members that cannot use the shared tools are governed from outside.** The core takes the reservations on their behalf, gives them a sandbox copy of the workspace, and merges their output only after the done-check. They can never touch shared state directly, so they cannot wipe anyone's work.

| Surface kind | Examples | How it is onboarded | How rules reach it | How it is governed |
|---|---|---|---|---|
| Protocol speaker | OpenCode (ACP), Codex (ACP adapter, `codex mcp-server`), A2A agents, any OpenAI-compatible endpoint | Bridge, no per-member work | Session prompt, injected MCP tools | Gated in the shared workspace |
| Headless CLI | `claude -p`, `opencode run`, `codex exec`, Gemini or Antigravity CLI, Qwen Code, Crush, Goose, Aider `--message` | Onboarding agent writes a driver spec from `--help` (3/3 first try in tests) | System prompt flag or prompt prefix, plus instruction file if it reads one | Gated through hooks or permissions where they exist, otherwise sandbox plus merge |
| Someone's favourite model CLI | `llm`, `aichat`, `mods`, `fabric`, `ollama run`, a vendor's own CLI | Same as headless CLI | Prompt prefix | Sandbox plus merge (most cannot edit files; they answer) |
| SDK or library | Claude Agent SDK, OpenAI Agents SDK, Vercel AI SDK, LangGraph, a personal Python agent | Onboarding agent reads the README and writes a small code shim: task in, result out, optional event callback | System prompt parameter | Level depends on the shim: sandbox by default, gated if the SDK exposes a tool-approval hook |
| Bare model or API | Ollama, OpenRouter, Z.ai API, Gemini API, any API key | Runs inside the reference harness, which speaks GT/1 natively | Native | Fully gated, because the reference harness is ours |
| Terminal-only app | A TUI with no headless mode | Virtual terminal driver | Typed in as the first message | Sandbox plus merge |
| Website or desktop app | z.ai chat, Gemini in search, ZCode desktop, ChatGPT app | See below | Prompt prefix | Sandbox plus merge |

**Websites and desktop apps.** Agents can drive browsers, so a website is a first-class member: a browser-capable member (any harness with a browser tool, or a computer-use engine over CDP) operates the site on the core's behalf, in the user's own visible browser profile. The core offers three doors per website member, in this order:

1. **Free API twin first.** Most free-tier website models have a free or near-free API twin: GLM Flash models on the Z.ai API (1 concurrent request) and OpenRouter, Gemini Flash models on the AI Studio free tier, OpenCode Zen free models, local models. Same model, structured output, no UI breakage, no account risk. `gt add z.ai` proposes this first.
2. **CDP driving.** For sites with no API twin, or when the user prefers the website. Opt-in per site, with a one-time notice: most consumer terms restrict automated access (Z.ai's terms ban bots and automated means aimed at unauthorized access; Google suspends accounts flagged for "automated activity" and losing a Google account means losing Gmail). Guardrails are fixed: human-like pace, the user's own logged-in session only, no CAPTCHA solving or bot-detection evasion (a challenge becomes a hold for the user), never across multiple accounts to multiply free quota.
3. **Assisted.** The core composes the full prompt and opens the site with it ready; the user presses send and brings the answer back with one key. Nothing automated.

Whatever the door, the answer goes through the sandbox, the merge and the done-check, because a website member cannot use the shared tools.

**What v0 builds:** the core, all bridges above, the onboarding agent for CLI, SDK and terminal surfaces, the reference harness, assisted mode for websites, rules prefix delivery, sandbox plus merge, intake check, done-check, quota failover, reservations, suggest-mode dispatch, the board, `gt.yaml`.

**Deferred, not cut:** the scoreboard and calibration exam, dispatcher auto mode, L4 rewind, the `council` member kind, the memory service beyond handoff packs, up-links (ACP, MCP, A2A out), automated GUI driving.

---

## 9. Five usability risks

| Risk | Design answer |
|---|---|
| **Holds become noise and get dismissed on reflex** | Only five hold types exist and no extension can add a sixth. Each states its default on timeout. The board shows holds per hour; above three, `gt` suggests the rule that would silence the most frequent one |
| **"It picked the wrong member" kills trust in routing** | Suggest mode forever by default. One-line reason with the suggestion. Enter accepts, a name overrides, and the override is recorded as a score. Auto mode needs a rule and an earned record |
| **The rules block pollutes the user's own CLAUDE.md** | One fenced block with markers, under 40 lines, regenerated in place, never touches a byte outside the markers, `gt leave` removes it. The user's own instructions always come first in the file |
| **Verifier false alarms (2 of 19 in tests) teach users to hit "accept anyway"** | The verdict quotes the exact criterion and the evidence. "Accept anyway" asks one more key: "was the check wrong? [y/n]", and a yes refines the skill's done list. The verifier can never edit code, so a false alarm costs a key, not work |
| **It is slower (42 s vs 27 s) and the user cannot see why** | The board shows what each member is waiting on right now (holding, claimed by, checking). Latency is spent only at task boundaries. `gt why` explains every wait in one line each |

---

## 10. Attacking this design

The weakest claim is "keep working as usual". It holds fully only for surfaces that accept injected tools. Everything else is a member (section 8), but in a sandbox with merge, so the user feels a difference: their answers arrive as a merge, not as live edits, and websites need one key in assisted mode. The promise is therefore "everything joins, and what cannot be steered live is kept safe in its own copy".

The judge is a model, and the core claims to have none. The free local model missed 1 in 14 contradictions. A missed contradiction looks exactly like the original incident. The mitigation is that the done-check catches the damage afterwards, but afterwards is after the wipe. So v0 defaults the judge to the strongest cheap member available (Haiku in tests: 14/14 blind, 3/3 on the subtle live pair, against 1/3 for the free model) and says so on `gt init`.

"Hold" is one word for four different mechanisms. That is the point, but it also means a user who hits hold type 3 and hold type 4 on the same task in a minute may not understand that one is about a file and the other about a claim. The wording above tries to make each self-explaining. It needs a usability test, not an argument.

`gt.yaml` will grow a grammar. "edits under", "shell command containing", "a different model than the writer" are already three mini languages. The guard against this is to cap the vocabulary at what the five hold types need and refuse rules the parser does not understand, with `gt rule` as the escape that writes the structured form for you.

And the solo case is thin. A user with one harness and no parallel sessions gets failover and the done-check and nothing else. The tests were all about two agents on one repo. If most people do not work that way, the install is not worth it to them, and the founder's own data (two or more sessions for half of active minutes, 35 files edited back and forth) may not generalise. That is the first thing to measure once a dozen people have run `gt init`.
