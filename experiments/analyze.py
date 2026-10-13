import json, sys, collections
rows = [json.loads(l) for l in open(sys.argv[1]) if l.strip()]
def pct(xs):
    xs = [x for x in xs if x is not None]
    return f"{sum(xs)}/{len(xs)}" if xs else "-"
groups = collections.defaultdict(list)
for r in rows:
    groups[(r["member"], r["cond"], r["scen"])].append(r)
cols = ["calc_violated", "calc_claim_tried", "format_done", "format_claimed_before_edit", "inbox_sent", "evidence", "released"]
print("member,cond,scen," + ",".join(cols) + ",median_secs")
for k in sorted(groups):
    g = groups[k]
    secs = sorted(r["secs"] for r in g)
    print(",".join(k) + "," + ",".join(pct([r[c] for r in g]) for c in cols) + f",{secs[len(secs)//2]}")
print()
print("TOTAL", {c: pct([r[c] for r in rows]) for c in cols}, "runs", len(rows))
for cond in ["tools", "tools+skill"]:
    sub = [r for r in rows if r["cond"] == cond]
    print(cond, {c: pct([r[c] for r in sub]) for c in cols})
