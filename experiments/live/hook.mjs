// Claude Code PreToolUse hook: the core's hard gate for file writes.
import { open, norm, gate } from "./gtdb.mjs";

let raw = "";
for await (const chunk of process.stdin) raw += chunk;
try {
  const ev = JSON.parse(raw);
  const file = ev.tool_input?.file_path || ev.tool_input?.notebook_path;
  if (!file) process.exit(0);
  const db = open(process.env.GT_DB);
  const r = gate(db, process.env.GT_MEMBER, norm(process.env.GT_REPO, file));
  if (!r.granted) {
    console.log(JSON.stringify({ hookSpecificOutput: { hookEventName: "PreToolUse", permissionDecision: "deny",
      permissionDecisionReason: `Blocked by workspace core: ${r.resource} is held by ${r.holder} (${r.note}). Message them with gt_inbox_send and use gt_wait, then re-read the file before editing.` } }));
  }
} catch { /* fail open: never break the harness */ }
process.exit(0);
