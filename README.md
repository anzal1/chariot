# god-tier

A harness of harnesses: one local layer that any harness, SDK, model CLI, API or website can join. It hands out tasks, keeps members from wiping each other's work, fails over when a subscription runs out, and checks what was really done. It has no model of its own and never holds anyone's keys.

Status: design, tested, not built yet.

| File | What it is |
|---|---|
| [ARCHITECTURE.md](ARCHITECTURE.md) | The engine: core, protocol (GT/1), drivers, onboarding, every test result |
| [ABSTRACTIONS.md](ABSTRACTIONS.md) | The user and developer layer on top: member, task, hold, rule; `gt.yaml`; extension points |
| [diagrams/architecture.html](diagrams/architecture.html) | The architecture as diagrams |
| [experiments/](experiments/) | The rigs behind every number in the docs |

## What the experiments showed (2026-10-12)

| Test | Result |
|---|---|
| Rules taught in the prompt channel vs only in tool descriptions | 18/18 followed vs 5/18 |
| Claude Code and OpenCode live on one repo | whole work wiped 2/6 without the protocol, 0/12 with |
| Intake contradiction check, 36 blind task pairs | 14/14 caught, 1/14 false flag (Haiku 5.5) |
| Final acceptance check | agents said done 36/36, really done 19/36; caught 17/17 |
| Onboarding from `--help` only | 3/3 targets passed conformance first try |

## Running the experiments

```sh
cd experiments && npm install
node run.mjs                 # in-context rules matrix
node live/batch.mjs          # live two-agent trials
node intake/detect.mjs       # intake contradiction check
node live/accept.mjs         # acceptance check over live/runs
node conform/conform.mjs onboard/specs/*.json
```

They drive real Claude Code, OpenCode and Ollama on this machine and use real quota.
