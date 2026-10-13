import { trial } from "./trial.mjs";
import fs from "node:fs"; import path from "node:path";
const HERE = path.dirname(new URL(import.meta.url).pathname);
const REPS = 3, CONC = 3, jobs = [];
for (const taskset of ["coop", "conflict"]) for (const cond of ["baseline", "soft", "hard"]) for (let r = 0; r < REPS; r++) jobs.push({ taskset, cond, r });
let i = 0;
async function worker() { while (i < jobs.length) { const j = jobs[i++];
  const res = await trial(j.cond, path.join(HERE, "runs", `${j.taskset}_${j.cond}_${j.r}`), j.taskset);
  const { gt, claude, opencode, ...rest } = res;
  const row = { ...j, ...rest, claude_t: claude.t, claude_code: claude.code, oc_t: opencode.t, oc_stop: opencode.stop, gt_calls: gt.length };
  fs.appendFileSync(path.join(HERE, "results.jsonl"), JSON.stringify(row) + "\n"); console.log(JSON.stringify(row)); } }
await Promise.all(Array.from({ length: CONC }, worker)); process.exit(0);
