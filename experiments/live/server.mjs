// GT/1 shared tools over MCP, backed by the shared SQLite state.
import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import { z } from "zod";
import { open, norm, log, claim, release, holderOf } from "./gtdb.mjs";

const REPO = process.env.GT_REPO, ME = process.env.GT_MEMBER;
const db = open(process.env.GT_DB);
const reply = (tool, args, out) => { log(db, ME, tool, { args, out }); return { content: [{ type: "text", text: JSON.stringify(out) }] }; };
const server = new McpServer({ name: "gt", version: "0.2.0" });

server.registerTool("gt_claim", {
  description: "Reserve a file before changing it. Another agent works in this repo at the same time. Only change a file after a granted claim.",
  inputSchema: { resource: z.string(), reason: z.string().optional() },
}, async ({ resource, reason }) => reply("gt_claim", { resource }, claim(db, ME, norm(REPO, resource), { reason: reason || "" })));

server.registerTool("gt_release", {
  description: "Release a file as soon as you finish changing it.",
  inputSchema: { resource: z.string() },
}, async ({ resource }) => { release(db, ME, norm(REPO, resource)); return reply("gt_release", { resource }, { released: true }); });

server.registerTool("gt_wait", {
  description: "Wait until a file held by another agent is released (up to timeout_s, max 150). Then claim it, re-read it, and build on the other agent's changes.",
  inputSchema: { resource: z.string(), timeout_s: z.number().optional() },
}, async ({ resource, timeout_s = 120 }) => {
  const r = norm(REPO, resource), end = Date.now() + Math.min(timeout_s, 150) * 1000;
  while (Date.now() < end) {
    const c = holderOf(db, r);
    if (!c || c.holder === ME) return reply("gt_wait", { resource }, { released: true, resource: r, waited_ms: Date.now() - (end - Math.min(timeout_s, 150) * 1000) });
    await new Promise(s => setTimeout(s, 500));
  }
  const c = holderOf(db, r);
  return reply("gt_wait", { resource }, { released: false, resource: r, holder: c?.holder });
});

server.registerTool("gt_inbox_send", {
  description: "Send a message to the other agent.",
  inputSchema: { to: z.string(), text: z.string() },
}, async ({ to, text }) => {
  db.prepare("INSERT INTO inbox(t,sender,recipient,text) VALUES (?,?,?,?)").run(Date.now(), ME, to, text);
  return reply("gt_inbox_send", { to, text }, { delivered: true });
});

server.registerTool("gt_inbox_read", {
  description: "Read messages sent to you by the other agent.",
  inputSchema: {},
}, async () => {
  const msgs = db.prepare("SELECT sender, text FROM inbox WHERE recipient=? AND read=0").all(ME);
  db.prepare("UPDATE inbox SET read=1 WHERE recipient=?").run(ME);
  return reply("gt_inbox_read", {}, { messages: msgs });
});

server.registerTool("gt_evidence", {
  description: "Report what you tested and anything you could not do, before you finish.",
  inputSchema: { kind: z.enum(["test", "check", "blocked", "note"]), detail: z.string() },
}, async ({ kind, detail }) => reply("gt_evidence", { kind, detail }, { recorded: true }));

await server.connect(new StdioServerTransport());
