// Generic driver runner + conformance check for declarative GT/1 driver specs.
// Usage: node conform.mjs <spec.json> [<spec.json> ...]
import fs from "node:fs";
import path from "node:path";
import { spawn } from "node:child_process";

const HERE = path.dirname(new URL(import.meta.url).pathname);
const get = (o, p) => p.split(".").reduce((x, k) => (x == null ? x : x[k]), o);

export function run(spec, prompt, workspace, timeoutMs = 120000) {
  const sub = s => s.replaceAll("{prompt}", prompt).replaceAll("{workspace}", workspace);
  const argv = spec.start.map(sub);
  return new Promise(res => {
    const t0 = Date.now();
    const p = spawn(argv[0], argv.slice(1), { cwd: workspace, env: { ...process.env, ...(spec.env || {}) }, stdio: ["ignore", "pipe", "pipe"] });
    let out = "", err = "";
    p.stdout.on("data", d => (out += d)); p.stderr.on("data", d => (err += d));
    const tm = setTimeout(() => p.kill("SIGKILL"), timeoutMs);
    p.on("error", e => { clearTimeout(tm); res({ error: e.message, events: [], result: "", ms: Date.now() - t0 }); });
    p.on("close", code => {
      clearTimeout(tm);
      const events = [];
      let result = "";
      if (spec.output === "jsonl") {
        for (const line of out.split("\n")) {
          let obj; try { obj = JSON.parse(line); } catch { continue; }
          const rule = (spec.events || []).find(r => Object.entries(r.when || {}).every(([k, v]) => get(obj, k) === v));
          if (!rule || rule.emit === "ignore") continue;
          const text = rule.text ? get(obj, rule.text) : undefined;
          events.push({ emit: rule.emit, text: typeof text === "string" ? text : text == null ? undefined : JSON.stringify(text) });
        }
        const results = events.filter(e => e.emit === "result" && e.text);
        result = results.length ? results[results.length - 1].text : events.filter(e => e.emit === "message" && e.text).map(e => e.text).join("\n");
      } else if (spec.output === "json") {
        try { result = String(get(JSON.parse(out), spec.result?.text) ?? ""); } catch { result = ""; }
      } else result = out;
      res({ code, events, result: result || "", ms: Date.now() - t0, stderr: err.slice(-500) });
    });
  });
}

export async function conform(specFile) {
  const spec = JSON.parse(fs.readFileSync(specFile, "utf8"));
  const ws = path.join(HERE, "ws", spec.id + "-" + Date.now());
  fs.mkdirSync(ws, { recursive: true });
  const report = { id: spec.id, kind: spec.kind, output: spec.output };
  if (spec.kind === "model") {
    const r = await run(spec, "Reply with exactly the word pong.", ws);
    report.L0 = /pong/i.test(r.result);
    report.detail = { result: r.result.slice(0, 120), ms: r.ms, error: r.error };
  } else {
    const r = await run(spec, "Create a file named hello.txt containing exactly the word hi. Then reply with the single word done.", ws);
    const f = path.join(ws, "hello.txt");
    const fileOk = fs.existsSync(f) && fs.readFileSync(f, "utf8").trim() === "hi";
    report.L0 = fileOk && r.result.trim().length > 0 && r.ms < 120000;
    report.detail = { fileOk, result: r.result.slice(0, 120), ms: r.ms, error: r.error, stderr: r.stderr };
    report.L1 = spec.output === "jsonl" && r.events.some(e => e.emit === "message" || e.emit === "tool.call") && r.events.some(e => e.emit === "result");
    report.event_counts = r.events.reduce((a, e) => ((a[e.emit] = (a[e.emit] || 0) + 1), a), {});
  }
  return report;
}

if (process.argv.length > 2) {
  for (const f of process.argv.slice(2)) console.log(JSON.stringify(await conform(f)));
  process.exit(0);
}
