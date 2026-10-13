// Minimal "governor" ACP client. Spawns a harness over ACP, offers to own fs,
// logs every effect it can see, and compares against what actually hit disk.
import { spawn } from "node:child_process";
import { Readable, Writable } from "node:stream";
import fs from "node:fs";
import path from "node:path";
import { ClientSideConnection, ndJsonStream, PROTOCOL_VERSION } from "@agentclientprotocol/sdk";

const [, , who, cwd, ...promptParts] = process.argv;
const prompt = promptParts.join(" ");
const cmds = {
  claude: [new URL("node_modules/.bin/claude-agent-acp", import.meta.url).pathname, []],
  opencode: ["opencode", ["acp"]],
};
const [bin, args] = cmds[who];
const log = [];
const t0 = Date.now();
const rec = (kind, data) => { log.push({ t: Date.now() - t0, kind, ...data }); };

const snap = () => Object.fromEntries(
  fs.readdirSync(cwd).filter(f => !f.startsWith(".") && f !== "opencode.json")
    .map(f => [f, fs.readFileSync(path.join(cwd, f), "utf8")]));
const before = snap();
const viaClient = new Set();

const child = spawn(bin, args, { cwd, stdio: ["pipe", "pipe", "inherit"], env: { ...process.env } });
const stream = ndJsonStream(Writable.toWeb(child.stdin), Readable.toWeb(child.stdout));

const client = {
  async requestPermission(p) {
    rec("permission", { title: p.toolCall?.title, kind: p.toolCall?.kind, options: p.options.map(o => o.kind) });
    const allow = p.options.find(o => o.kind === "allow_once") ?? p.options[0];
    return { outcome: { outcome: "selected", optionId: allow.optionId } };
  },
  async sessionUpdate(n) {
    const u = n.update;
    if (u.sessionUpdate === "tool_call" || u.sessionUpdate === "tool_call_update") {
      const diffs = (u.content ?? []).filter(c => c.type === "diff").map(c => c.path);
      rec(u.sessionUpdate, { id: u.toolCallId, title: u.title, kind: u.kind, status: u.status, diffs: diffs.length ? diffs : undefined });
    }
  },
  async writeTextFile(p) {
    rec("fs_write", { path: p.path, bytes: p.content.length });
    viaClient.add(path.basename(p.path));
    fs.writeFileSync(p.path, p.content);
    return {};
  },
  async readTextFile(p) {
    rec("fs_read", { path: p.path });
    return { content: fs.existsSync(p.path) ? fs.readFileSync(p.path, "utf8") : "" };
  },
};

const conn = new ClientSideConnection(() => client, stream);
const init = await conn.initialize({
  protocolVersion: PROTOCOL_VERSION,
  clientCapabilities: { fs: { readTextFile: true, writeTextFile: true }, terminal: false },
});
rec("init", { agent: init.agentInfo?.name, version: init.agentInfo?.version, caps: Object.keys(init.agentCapabilities ?? {}) });
const { sessionId } = await conn.newSession({ cwd, mcpServers: [] });
const res = await conn.prompt({ sessionId, prompt: [{ type: "text", text: prompt }] });
rec("done", { stopReason: res.stopReason });

const after = snap();
const changed = Object.keys(after).filter(f => after[f] !== before[f]);
const bypassed = changed.filter(f => !viaClient.has(f));
console.log(JSON.stringify({ who, log, changed, routedThroughGovernor: [...viaClient], bypassedGovernor: bypassed }, null, 1));
child.kill();
process.exit(0);
