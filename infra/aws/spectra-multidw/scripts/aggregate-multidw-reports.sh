#!/usr/bin/env bash
# Aggregate multidw shard reports into a summary JSON for scoreboard fill.
# Usage: ./scripts/aggregate-multidw-reports.sh [reports_dir]
set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../.." && pwd)"
DIR="${1:-$REPO/profiling/spectra-bench/reports}"
OUT="${DIR}/multidw-aggregate-summary.json"

python3 - <<PY
import json, glob, os, re
from collections import defaultdict

report_dir = "${DIR}"
repo_root = "${REPO}"
pat = re.compile(r"multidw-(bm-sw[56])-(\w+)-n(\d+)-shard(\d+)-(.+)\.json")
cells = defaultdict(list)
for path in sorted(glob.glob(os.path.join(report_dir, "multidw-*.json"))):
    base = os.path.basename(path)
    if base.startswith("multidw-aggregate"):
        continue
    m = pat.match(base)
    if not m:
        continue
    exp, storage, n, shard, hw = m.groups()
    with open(path) as f:
        doc = json.load(f)
    if isinstance(doc, list):
        continue
    durable = doc.get("durable_counter_ops_per_sec")
    if durable is None:
        durable = doc.get("durable_event_ops_per_sec")
    # Prefer repo-relative paths so aggregates never embed machine home dirs.
    rel_path = os.path.relpath(path, repo_root) if path.startswith(repo_root + os.sep) else base
    cells[(exp, storage, int(n), hw)].append({
        "shard": int(shard),
        "durable_ops_per_sec": durable,
        "binding_tier": doc.get("binding_tier"),
        "visibility_confirmed": doc.get("visibility_confirmed"),
        "path": rel_path.replace(os.sep, "/"),
    })

rows = []
for (exp, storage, n, hw), shards in sorted(cells.items()):
    rates = [s["durable_ops_per_sec"] for s in shards if s["durable_ops_per_sec"] is not None]
    agg = sum(rates) if rates else None
    tiers = {s["binding_tier"] for s in shards}
    rows.append({
        "experiment": exp,
        "storage": storage,
        "n": n,
        "hardware": hw,
        "shard_count": len(shards),
        "durable_ops_per_sec_aggregate": agg,
        "shards": shards,
        "binding_tiers": sorted(t for t in tiers if t),
    })

# efficiency vs n=1
by_key = {(r["experiment"], r["storage"], r["hardware"], r["n"]): r for r in rows}
for r in rows:
    if r["n"] != 2:
        continue
    n1 = by_key.get((r["experiment"], r["storage"], r["hardware"], 1))
    if not n1 or not n1["durable_ops_per_sec_aggregate"] or not r["durable_ops_per_sec_aggregate"]:
        r["efficiency"] = None
        continue
    r["efficiency"] = r["durable_ops_per_sec_aggregate"] / (2.0 * n1["durable_ops_per_sec_aggregate"])

out = {"rows": rows}
with open("${OUT}", "w") as f:
    json.dump(out, f, indent=2)
    f.write("\n")
print("Wrote ${OUT}")
for r in rows:
    print(f"{r['experiment']} {r['storage']} n={r['n']}: aggregate={r.get('durable_ops_per_sec_aggregate')} efficiency={r.get('efficiency')} tiers={r.get('binding_tiers')}")
PY
