// Intake contradiction detector test: a cheap member judges task pairs before dispatch.
import fs from "node:fs";
import path from "node:path";
import { spawn } from "node:child_process";

const HERE = path.dirname(new URL(import.meta.url).pathname);
const data = JSON.parse(fs.readFileSync(path.join(HERE, "dataset.json"), "utf8")).items;
const JUDGES = (process.env.JUDGES || "haiku,free").split(",");
const REPS = Number(process.env.REPS || 1);

const PROMPT = it => `Two tasks are about to be given to two different AI agents that will work on the same repository at the same time. Decide whether the tasks CONTRADICT: they cannot both be fully satisfied, because doing one necessarily violates the other (explicitly or implicitly). Touching the same files or functions is NOT a contradiction if both can be satisfied together.

Repository: ${it.repo}

Task A: ${it.task_a}

Task B: ${it.task_b}

Reply with ONLY one line of JSON: {"verdict":"contradict" or "compatible","reason":"one sentence","human_decision":"the question a human must answer, or empty"}`;

function judge(kind, prompt, cwd) {
  let cmd, args;
  if (kind === "haiku") { cmd = "claude"; args = ["-p", prompt, "--model", "claude-haiku-5-5", "--strict-mcp-config", "--mcp-config", '{"mcpServers":{}}', "--output-format", "json", "--tools", ""]; }
  else if (kind === "sonnet") { cmd = "claude"; args = ["-p", prompt, "--model", "claude-sonnet-5-5", "--strict-mcp-config", "--mcp-config", '{"mcpServers":{}}', "--output-format", "json", "--tools", ""]; }
  else { cmd = "opencode"; args = ["run", prompt, "-m", "opencode/space-bunny-free", "--dir", cwd]; }
  return new Promise(res => {
    const t0 = Date.now();
    const p = spawn(cmd, args, { cwd, stdio: ["ignore", "pipe", "pipe"] });
    let out = ""; p.stdout.on("data", d => (out += d)); p.stderr.on("data", () => {});
    const tm = setTimeout(() => p.kill("SIGKILL"), 120000);
    p.on("close", () => { clearTimeout(tm); let text = out; try { text = JSON.parse(out).result ?? out; } catch {} res({ text: String(text), ms: Date.now() - t0 }); });
  });
}

const cwd = path.join(HERE, "work"); fs.mkdirSync(cwd, { recursive: true });
const jobs = [];
for (const it of data) for (const j of JUDGES) for (let r = 0; r < REPS; r++) jobs.push({ it, j, r });
fs.rmSync(path.join(HERE, "detect.jsonl"), { force: true });
let i = 0;
async function worker() {
  while (i < jobs.length) {
    const { it, j, r } = jobs[i++];
    const { text, ms } = await judge(j, PROMPT(it), cwd);
    const m = text.match(/"verdict"\s*:\s*"(contradict|compatible)"/);
    const row = { id: it.id, label: it.label, judge: j, r, verdict: m ? m[1] : null, ms, reason: (text.match(/"reason"\s*:\s*"([^"]*)"/) || [])[1] || null };
    fs.appendFileSync(path.join(HERE, "detect.jsonl"), JSON.stringify(row) + "\n");
  }
}
await Promise.all(Array.from({ length: 4 }, worker));
const rows = fs.readFileSync(path.join(HERE, "detect.jsonl"), "utf8").trim().split("\n").map(JSON.parse);
for (const j of JUDGES) {
  const s = rows.filter(r => r.judge === j);
  const by = l => s.filter(r => r.label === l);
  const flag = rs => rs.filter(r => r.verdict === "contradict").length + "/" + rs.length;
  const ms = s.map(r => r.ms).sort((a, b) => a - b);
  console.log(j, "contradict caught", flag(by("contradict")), "| compatible_overlap falsely flagged", flag(by("compatible_overlap")), "| disjoint falsely flagged", flag(by("disjoint")), "| unparsed", s.filter(r => !r.verdict).length, "| median s", Math.round(ms[ms.length >> 1] / 1000));
}
process.exit(0);
