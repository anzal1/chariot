import json, sys, collections, os
here = os.path.dirname(os.path.abspath(__file__))
rows = [json.loads(l) for l in open(os.path.join(here, "results.jsonl")) if l.strip()]
g = collections.defaultdict(list)
for r in rows:
    g[(r["taskset"], r["cond"])].append(r)
def frac(xs): return f"{sum(1 for x in xs if x)}/{len(xs)}"
print("taskset,cond,both_done,A_done,B_done,add_works,any_lost,add_flips(sum),tests_pass,msgs,median_s")
for k in sorted(g):
    rs = g[k]
    secs = sorted(max(r["claude_t"], r["oc_t"]) // 1000 for r in rs)
    print(",".join(k), frac([r["both_done"] for r in rs]), frac([r["A_done"] for r in rs]), frac([r["B_done"] for r in rs]),
          frac([r["behaviour"].get("add_works") for r in rs]), frac([r["any_lost"] for r in rs]),
          sum(r["add_flips"] for r in rs), frac([r["tests_pass"] for r in rs]),
          sum(len(r.get("inbox", [])) for r in rs), secs[len(secs) // 2], sep=",")
with open(os.path.join(here, "reports.txt"), "w") as f:
    for r in rows:
        if r["taskset"] != "conflict": continue
        res = json.load(open(os.path.join(here, "runs", f"conflict_{r['cond']}_{r['r']}", "result.json")))
        f.write(f"===== conflict {r['cond']} {r['r']} | A_sum={r['behaviour'].get('A_sum')} add_works={r['behaviour'].get('add_works')} flips={r['add_flips']}\n")
        f.write("--- claude:\n" + (res["claude"]["text"] or "")[-900:] + "\n--- opencode:\n" + (res["opencode"]["text"] or "")[-900:] + "\n")
        for m in res.get("inbox", []): f.write(f"[inbox {m['sender']}->{m['recipient']}] {m['text'][:300]}\n")
print("reports written")
