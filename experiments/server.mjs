// Minimal GT/1 shared-tools MCP server for the in-context teaching test.
// State and call log live in files named by env vars, one per run.
import fs from "node:fs";
import path from "node:path";
import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import { z } from "zod";

const REPO = process.env.GT_REPO;
const STATE = process.env.GT_STATE;
const LOG = process.env.GT_LOG;
const ME = process.env.GT_MEMBER || "you";

const load = () => JSON.parse(fs.readFileSync(STATE, "utf8"));
const save = s => fs.writeFileSync(STATE, JSON.stringify(s, null, 1));
const log = (tool, args, out) =>
  fs.appendFileSync(LOG, JSON.stringify({ t: Date.now(), tool, args, out }) + "\n");
const norm = r => {
  let p = String(r).replace(/^file:\/\//, "");
  if (path.isAbsolute(p)) p = path.relative(fs.realpathSync(REPO), fs.existsSync(p) ? fs.realpathSync(p) : p);
  return path.normalize(p).replace(/^(\.\/)+/, "");
};
const reply = (tool, args, out) => { log(tool, args, out); return { content: [{ type: "text", text: JSON.stringify(out) }] }; };

const server = new McpServer({ name: "gt", version: "0.1.0" });

server.registerTool("gt_claim", {
  description: "Reserve a resource before you change it. This workspace is shared with other agents working at the same time. You must hold a granted write claim on a file before you create, edit or delete it. If a claim is denied, do not change that file: message the holder with gt_inbox_send and continue with the rest of your work.",
  inputSchema: { resource: z.string().describe("File path relative to the repo, e.g. src/app.js"), mode: z.enum(["read", "write"]).default("write"), reason: z.string().optional() },
}, async ({ resource, mode = "write", reason }) => {
  const s = load(); const r = norm(resource);
  const held = s.claims.find(c => c.resource === r && c.holder !== ME && c.mode === "write");
  if (held && mode === "write") return reply("gt_claim", { resource, mode, reason }, { granted: false, resource: r, holder: held.holder, note: held.reason, until: held.until });
  s.claims = s.claims.filter(c => !(c.resource === r && c.holder === ME));
  s.claims.push({ resource: r, holder: ME, mode, reason: reason || "", until: "end of task" });
  save(s);
  return reply("gt_claim", { resource, mode, reason }, { granted: true, resource: r, holder: ME });
});

server.registerTool("gt_release", {
  description: "Release a claim you hold once you are done changing the resource.",
  inputSchema: { resource: z.string() },
}, async ({ resource }) => {
  const s = load(); const r = norm(resource);
  s.claims = s.claims.filter(c => !(c.resource === r && c.holder === ME)); save(s);
  return reply("gt_release", { resource }, { released: true, resource: r });
});

server.registerTool("gt_inbox_send", {
  description: "Send a message to another agent in this workspace, for example the holder of a resource you need.",
  inputSchema: { to: z.string(), text: z.string() },
}, async ({ to, text }) => reply("gt_inbox_send", { to, text }, { delivered: true }));

server.registerTool("gt_evidence", {
  description: "Report evidence for your work before you finish: tests you ran, what passed, what you could not do and why.",
  inputSchema: { kind: z.enum(["test", "check", "blocked", "note"]), detail: z.string() },
}, async ({ kind, detail }) => reply("gt_evidence", { kind, detail }, { recorded: true }));

await server.connect(new StdioServerTransport());
