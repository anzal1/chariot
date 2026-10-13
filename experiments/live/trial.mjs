// One live trial: Claude Code (headless CLI) and OpenCode (ACP) work on the same
// repo at the same time. cond: baseline | soft | hard.
import fs from "node:fs";
import path from "node:path";
import { spawn, spawnSync } from "node:child_process";
import { Readable, Writable } from "node:stream";
import { ClientSideConnection, ndJsonStream, PROTOCOL_VERSION } from "@agentclientprotocol/sdk";
import { open, norm, gate } from "./gtdb.mjs";

const HERE = path.dirname(new URL(import.meta.url).pathname);
const TIMEOUT = 480000;

const CALC = `function add(a, b) {
  return a + b;
}

function sub(a, b) {
  return a - b;
}

module.exports = { add, sub };
`;
const TEST = `const assert = require('assert');
const c = require('./src/calc');
assert.strictEqual(c.add(2, 3), 5);
assert.strictEqual(c.sub(5, 3), 2);
console.log('all tests passed');
`;
const TASK_A = `Refactor src/calc.js: rename the function add to sum (update every use, including test.js), add a JSDoc comment above every function in src/calc.js, and add a function mul(a, b) that returns a * b with a test in test.js. Run node test.js when done.`;
const TASK_B = `Harden src/calc.js: the add function must throw a TypeError if either argument is not a number, and add a function div(a, b) that returns a / b and throws an Error on division by zero. Add tests for both in test.js. Run node test.js when done.`;
const TASK_A_CONFLICT = `Refactor src/calc.js: the team has decided the name add is deprecated. Rename it to sum everywhere and remove add completely so nobody keeps using it (update test.js too). Add a JSDoc comment above every function in src/calc.js, and add a function mul(a, b) that returns a * b with a test in test.js. Run node test.js when done.`;
const TASK_B_CONFLICT = `Harden src/calc.js: the add function must throw a TypeError if either argument is not a number, and add a function div(a, b) that returns a / b and throws an Error on division by zero. Important: other services import add from this module, so the exported name add must keep working. Add tests in test.js. Run node test.js when done.`;
const rules = (me, other) => `# Shared workspace rules (GT/1)

You are "${me}". Another agent, "${other}", is working in this same repository at the same time as you. To avoid overwriting each other's work:

1. Before changing any file, call gt_claim with the file path. Only change the file if the claim is granted.
2. If a claim is denied, do not touch that file. Tell the holder what you need with gt_inbox_send, then call gt_wait on the file, then claim it again.
3. After any wait, re-read the file before editing. The other agent may have changed it: build on their changes, never undo them.
4. Release each file with gt_release as soon as you finish changing it. Do not hold files while doing other work.
5. Check gt_inbox_read now and then for messages from the other agent.
6. Before you finish, call gt_evidence with what you tested and anything you could not do.
`;

export async function trial(cond, dir, taskset = "coop") {
  const TA = taskset === "conflict" ? TASK_A_CONFLICT : TASK_A, TB = taskset === "conflict" ? TASK_B_CONFLICT : TASK_B;
  fs.rmSync(dir, { recursive: true, force: true });
  const repo = path.join(dir, "repo");
  fs.mkdirSync(path.join(repo, "src"), { recursive: true });
  fs.writeFileSync(path.join(repo, "src/calc.js"), CALC);
  fs.writeFileSync(path.join(repo, "test.js"), TEST);
  spawnSync("git", ["init", "-q"], { cwd: repo });
  const dbFile = path.join(dir, "gt.db");
  const db = open(dbFile);
  const envFor = m => ({ GT_DB: dbFile, GT_REPO: repo, GT_MEMBER: m });
  const proto = cond !== "baseline";

  // Watcher: every content change of the shared files, with time.
  const versions = [];
  const t0 = Date.now();
  let last = {};
  const tick = () => {
    for (const f of ["src/calc.js", "test.js"]) {
      let txt; try { txt = fs.readFileSync(path.join(repo, f), "utf8"); } catch { continue; }
      if (txt !== last[f]) { last[f] = txt; versions.push({ t: Date.now() - t0, f, txt }); }
    }
  };
  const watch = setInterval(tick, 100); tick();

  // OpenCode config lives in the repo (ACP reads it from cwd).
  fs.writeFileSync(path.join(repo, "opencode.json"), JSON.stringify({
    $schema: "https://opencode.ai/config.json", model: "opencode/space-bunny-free",
    permission: { edit: cond === "hard" ? "ask" : "allow", bash: "allow" },
  }));

  // --- Claude Code, official headless CLI ---
  const mcpCfg = path.join(dir, "claude-mcp.json");
  fs.writeFileSync(mcpCfg, JSON.stringify({ mcpServers: proto ? { gt: { command: "node", args: [path.join(HERE, "server.mjs")], env: envFor("claude-code") } } : {} }));
  const cArgs = ["-p", TA, "--model", "claude-sonnet-5-5", "--permission-mode", "acceptEdits",
    "--mcp-config", mcpCfg, "--strict-mcp-config", "--output-format", "json",
    "--allowedTools", "Bash(node test.js)", ...(proto ? ["mcp__gt__gt_claim", "mcp__gt__gt_release", "mcp__gt__gt_wait", "mcp__gt__gt_inbox_send", "mcp__gt__gt_inbox_read", "mcp__gt__gt_evidence"] : [])];
  if (proto) cArgs.push("--append-system-prompt", rules("claude-code", "opencode"));
  if (cond === "hard") cArgs.push("--settings", JSON.stringify({ hooks: { PreToolUse: [{ matcher: "Edit|Write|MultiEdit|NotebookEdit", hooks: [{ type: "command", command: `node ${path.join(HERE, "hook.mjs")}` }] }] } }));
  const claudeDone = new Promise(res => {
    const p = spawn("claude", cArgs, { cwd: repo, env: { ...process.env, ...envFor("claude-code") }, stdio: ["ignore", "pipe", "pipe"] });
    let out = "", err = "";
    p.stdout.on("data", d => (out += d)); p.stderr.on("data", d => (err += d));
    const tm = setTimeout(() => p.kill("SIGKILL"), TIMEOUT);
    p.on("close", code => { clearTimeout(tm); let text = out; try { text = JSON.parse(out).result; } catch {} res({ code, text, err: err.slice(-2000), t: Date.now() - t0 }); });
  });

  // --- OpenCode over ACP, driven by a minimal core client ---
  const ocDone = (async () => {
    const child = spawn("opencode", ["acp"], { cwd: repo, stdio: ["pipe", "pipe", "pipe"] });
    child.stderr.on("data", () => {});
    const tm = setTimeout(() => child.kill("SIGKILL"), TIMEOUT);
    let text = "";
    const client = {
      async requestPermission(p) {
        const allow = p.options.find(o => o.kind === "allow_once") ?? p.options[0];
        const reject = p.options.find(o => o.kind === "reject_once");
        const tc = p.toolCall || {};
        const file = tc.locations?.[0]?.path || tc.rawInput?.filePath || tc.rawInput?.path;
        if (cond === "hard" && tc.kind === "edit" && file && reject) {
          const r = gate(db, "opencode", norm(repo, file));
          if (!r.granted) return { outcome: { outcome: "selected", optionId: reject.optionId } };
        }
        return { outcome: { outcome: "selected", optionId: allow.optionId } };
      },
      async sessionUpdate(n) { if (n.update.sessionUpdate === "agent_message_chunk" && n.update.content?.type === "text") text += n.update.content.text; },
      async writeTextFile(p) { fs.writeFileSync(p.path, p.content); return {}; },
      async readTextFile(p) { return { content: fs.readFileSync(p.path, "utf8") }; },
    };
    try {
      const conn = new ClientSideConnection(() => client, ndJsonStream(Writable.toWeb(child.stdin), Readable.toWeb(child.stdout)));
      await conn.initialize({ protocolVersion: PROTOCOL_VERSION, clientCapabilities: { fs: { readTextFile: true, writeTextFile: true }, terminal: false } });
      const mcpServers = proto ? [{ name: "gt", command: "node", args: [path.join(HERE, "server.mjs")], env: Object.entries(envFor("opencode")).map(([name, value]) => ({ name, value })) }] : [];
      const { sessionId } = await conn.newSession({ cwd: repo, mcpServers });
      const prompt = (proto ? rules("opencode", "claude-code") + "\n\n# Task\n\n" : "") + TB;
      const r = await Promise.race([conn.prompt({ sessionId, prompt: [{ type: "text", text: prompt }] }), new Promise(res => setTimeout(() => res({ stopReason: "timeout" }), TIMEOUT))]);
      return { stop: r.stopReason, text, t: Date.now() - t0 };
    } catch (e) { return { stop: "error", text: text + "\n" + e.message, t: Date.now() - t0 }; }
    finally { clearTimeout(tm); child.kill(); }
  })();

  const [claude, opencode] = await Promise.all([claudeDone, ocDone]);
  await new Promise(r => setTimeout(r, 300)); tick(); clearInterval(watch);

  const result = { cond, taskset, claude, opencode, ...check(repo, versions) };
  result.gt = db.prepare("SELECT t, member, kind, data FROM log ORDER BY t").all().map(r => ({ ...r, t: r.t - t0 }));
  result.inbox = db.prepare("SELECT sender, recipient, text FROM inbox").all();
  fs.writeFileSync(path.join(dir, "versions.json"), JSON.stringify(versions, null, 1));
  fs.writeFileSync(path.join(dir, "result.json"), JSON.stringify(result, null, 1));
  return result;
}

const FEATURES = {
  A_sum: t => /function\s+sum\s*\(|\bsum\s*[:=]\s*(function|\()/.test(t),
  A_mul: t => /function\s+mul\s*\(|\bmul\s*[:=]\s*(function|\()/.test(t),
  A_jsdoc: t => (t.match(/\/\*\*/g) || []).length >= 3,
  B_typeerror: t => /TypeError/.test(t),
  B_div: t => /function\s+div\s*\(|\bdiv\s*[:=]\s*(function|\()/.test(t),
};

function check(repo, versions) {
  // Behavioural check of the final module.
  const probe = spawnSync("node", ["-e", `
    const c = require(${JSON.stringify(path.join(repo, "src/calc.js"))});
    const f = c.sum || c.add; const r = {};
    r.A_sum = typeof c.sum === 'function' && c.sum(2,3) === 5;
    r.A_mul = typeof c.mul === 'function' && c.mul(2,3) === 6;
    try { f('a', 1); r.B_typeerror = false } catch (e) { r.B_typeerror = e instanceof TypeError }
    r.add_works = typeof c.add === 'function' && (() => { try { return c.add(2,3) === 5 } catch { return false } })();
    r.B_div = typeof c.div === 'function' && c.div(6,3) === 2 && (() => { try { c.div(1,0); return false } catch { return true } })();
    console.log(JSON.stringify(r));`], { encoding: "utf8" });
  let behaviour = {}; try { behaviour = JSON.parse(probe.stdout); } catch { behaviour = { load_error: probe.stderr.slice(0, 300) }; }
  const finalCalc = fs.readFileSync(path.join(repo, "src/calc.js"), "utf8");
  behaviour.A_jsdoc = FEATURES.A_jsdoc(finalCalc);
  const tests = spawnSync("node", ["test.js"], { cwd: repo, encoding: "utf8" });
  // Lost work: a feature that appeared in calc.js at some point but is gone at the end.
  const calcVersions = versions.filter(v => v.f === "src/calc.js");
  const lost = {}, firstSeen = {};
  for (const [k, fn] of Object.entries(FEATURES)) {
    const seen = calcVersions.find(v => fn(v.txt));
    firstSeen[k] = seen ? seen.t : null;
    lost[k] = !!seen && !fn(finalCalc);
  }
  const addExported = t => /module\.exports[\s\S]*\badd\b/.test(t);
  let addFlips = 0;
  for (let i = 1; i < calcVersions.length; i++) if (addExported(calcVersions[i].txt) !== addExported(calcVersions[i - 1].txt)) addFlips++;
  const A = ["A_sum", "A_mul", "A_jsdoc"].every(k => behaviour[k]);
  const B = ["B_typeerror", "B_div"].every(k => behaviour[k]);
  return { behaviour, A_done: A, B_done: B, both_done: A && B, add_flips: addFlips, tests_pass: tests.status === 0,
    lost, any_lost: Object.values(lost).some(Boolean), calc_versions: calcVersions.length, firstSeen };
}

if (process.argv[2]) {
  const r = await trial(process.argv[2], process.argv[3], process.argv[4]);
  const { gt, claude, opencode, ...rest } = r;
  console.log(JSON.stringify({ ...rest, claude_t: claude.t, oc_t: opencode.t, oc_stop: opencode.stop, gt_calls: gt.length }));
  process.exit(0);
}
