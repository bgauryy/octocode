#!/usr/bin/env python3
"""Timing + quality benchmark for `octocode graph ingest|query` across repos.

Quality is measured without trusting the graph: sampled edges are checked
against the source line they cite, detector findings are cross-checked with
`git grep`, and ingest determinism is checked by re-ingesting.
"""
import hashlib, json, os, random, re, subprocess, sys, time

from graphbin import decode

BIN = sys.argv[1]
WS = sys.argv[2]
REPOS = sys.argv[3:]
random.seed(7)


def run(args, cwd=None):
    t = time.perf_counter()
    p = subprocess.run([BIN, *args], capture_output=True, text=True, cwd=cwd)
    ms = (time.perf_counter() - t) * 1000
    try:
        out = json.loads(p.stdout) if p.stdout.strip() else {}
    except json.JSONDecodeError:
        out = {"raw": p.stdout[:300]}
    return out, ms, p.returncode


def source_line(root, rel, line, span=0):
    try:
        with open(os.path.join(root, rel), encoding="utf-8", errors="replace") as f:
            lines = f.read().split("\n")
    except OSError:
        return None
    # Facts use 1-based lines; tolerate an off-by-one window for multi-line
    # statements.
    lo, hi = max(0, line - 2), min(len(lines), line + span + 1)
    return "\n".join(lines[lo:hi])


def stem(path):
    base = path.rsplit("/", 1)[-1]
    s = base.split(".")[0]
    if s in ("index", "mod", "__init__", "lib", "main"):
        parts = path.split("/")
        return parts[-2] if len(parts) > 1 else s
    return s


def edge_precision(root, nodes, edges):
    imports = [e for e in edges if e["kind"] == 1 and nodes[e["dst"]]["kind"] == 0 and e["line"] != 0xFFFFFFFF]
    calls = [e for e in edges if e["kind"] == 2 and e["conf"] == 0 and e["line"] != 0xFFFFFFFF]
    res = {}
    for label, pool, want in (("imports", imports, lambda e: stem(nodes[e["dst"]]["key"])),
                              ("callsHigh", calls, lambda e: nodes[e["dst"]]["name"])):
        sample = random.sample(pool, min(100, len(pool)))
        ok = 0
        bad = []
        for e in sample:
            src_file = nodes[e["src"]]["key"] if nodes[e["src"]]["kind"] == 0 else nodes[nodes[e["src"]]["file"]]["key"]
            text = source_line(root, src_file, e["line"], span=6 if label == "imports" else 1)
            token = want(e)
            # Imports name a module path, not always the file stem: accept
            # any of the target's last three path segments (Go package dirs,
            # index/mod files, workspace aliases like `@scope/common`).
            tokens = [token] if label != "imports" else [
                seg.split(".")[0] for seg in nodes[e["dst"]]["key"].split("/")[-3:]
            ] + [token]
            if text is not None and any(t and t in text for t in tokens):
                ok += 1
            elif len(bad) < 3:
                bad.append(f"{src_file}:{e['line']} -> {token}")
        res[label] = {"sampled": len(sample), "valid": ok, "precision": round(ok / len(sample), 3) if sample else None, "misses": bad}
    return res


def git_grep(root, pattern, word=True):
    args = ["git", "-C", root, "grep", "-l", "-I", "-F"] + (["-w"] if word else []) + ["-e", pattern]
    p = subprocess.run(args, capture_output=True, text=True)
    return [l for l in p.stdout.splitlines() if l]


def finding_proxies(root, issues):
    rows = issues.get("results", [])
    out = {}
    unreachable = [r for r in rows if r["detector"] == "unreachable-file"][:40]
    suspects = 0
    for r in unreachable:
        s = stem(r["subject"])
        hits = [h for h in git_grep(root, s) if h != r["subject"] and not h.endswith((".md", ".json", ".txt"))]
        refd = False
        for h in hits[:30]:
            try:
                txt = open(os.path.join(root, h), encoding="utf-8", errors="replace").read()
            except OSError:
                continue
            if re.search(r"(import|require|from|include|mod |use )[^\n]*\b" + re.escape(s) + r"\b", txt):
                refd = True
                break
        suspects += refd
    if unreachable:
        out["unreachable"] = {"checked": len(unreachable), "textuallyReferenced": suspects,
                              "precisionLowerBound": round(1 - suspects / len(unreachable), 3)}
    unused = [r for r in rows if r["detector"] == "unused-export"][:30]
    checked = sus = 0
    for r in unused:
        for name in r["evidence"]["exports"][:4]:
            if len(name) < 4:
                continue
            checked += 1
            hits = [h for h in git_grep(root, name) if h != r["subject"] and not re.search(r"(test|spec|__tests__|\.md$|\.json$)", h)]
            sus += bool(hits)
    if checked:
        out["unusedExport"] = {"checked": checked, "nameAppearsElsewhere": sus,
                               "precisionLowerBound": round(1 - sus / checked, 3)}
    cycles = [r for r in rows if r["detector"] == "cycle"][:20]
    hops = valid = 0
    for r in cycles:
        for hop in r["evidence"]["witness"]:
            if "line" not in hop:
                continue
            hops += 1
            text = source_line(root, hop["from"], hop["line"], span=6)
            segs = [seg.split(".")[0] for seg in hop["to"].split("/")[-3:]] + [stem(hop["to"])]
            valid += bool(text and any(t and t in text for t in segs))
    if hops:
        out["cycleWitness"] = {"hops": hops, "verified": valid, "precision": round(valid / hops, 3)}
    return out


def bench(repo):
    root = os.path.abspath(repo)
    name = os.path.basename(root)
    rec = {"repo": name}
    ing, ms, code = run(["graph", "ingest", root, "--workspace", WS, "--keep", "1"])
    if code != 0:
        rec["error"] = ing
        return rec
    files = ing["scan"]["filesScanned"]
    rec.update(files=files, truncated=ing["scan"]["truncated"], ingestMs=round(ms), buildMs=ing["buildMs"],
               filesPerSec=round(files / (ms / 1000)), bytes=ing["bytes"], nodes=ing["counts"]["nodes"],
               edges=ing["counts"]["edgesByKind"], callsLinked=f'{ing["calls"]["linked"]}/{ing["calls"]["sites"]}',
               callInternalRecall=ing.get("callInternalRecall"))
    gid = ing["id"]
    if files <= 5000:
        again, _, _ = run(["graph", "ingest", root, "--workspace", WS, "--keep", "2"])
        rec["deterministic"] = again.get("sha256") == ing["sha256"]
    q = lambda *a: run(["graph", "query", *a, "--graph", gid, "--workspace", WS])
    _, startup, _ = run(["--version"])
    rec["startupMs"] = round(startup)
    stats, ms, _ = q("stats")
    times = {"stats": round(ms)}
    top = (stats.get("mostImportedFiles") or [{}])[0].get("id")
    sym = (stats.get("mostCalledSymbols") or [{}])[0].get("id")
    if top:
        for op, extra in (("dependents", ["--depth", "3"]), ("impact", []), ("deps", ["--depth", "2"])):
            _, ms, _ = q(op, top, *extra)
            times[op] = round(ms)
    if sym:
        _, ms, _ = q("callers", sym)
        times["callers"] = round(ms)
        _, ms, _ = q("find", sym.rsplit("#", 1)[-1].split(".")[-1][:4])
        times["find"] = round(ms)
    for op in ("cycles", "hubs"):
        _, ms, _ = q(op)
        times[op] = round(ms)
    issues, ms, _ = q("issues", "--limit", "1000")
    times["issues"] = round(ms)
    rec["queryMs"] = times
    rec["findings"] = issues.get("summary", {}).get("byDetector")
    rec["hiddenBelowTier"] = issues.get("summary", {}).get("hiddenBelowTier")
    rec["health"] = issues.get("summary", {}).get("health")
    rec["notes"] = [n["skipped"] for n in issues.get("summary", {}).get("notes", [])]
    graph_bin = os.path.join(WS, ".octocode", "graph", gid, "graph.bin")
    nodes, edges = decode(graph_bin)
    rec["edgeAccuracy"] = edge_precision(root, nodes, edges)
    rec["detectorProxies"] = finding_proxies(root, issues)
    return rec


results = []
for repo in REPOS:
    r = bench(repo)
    print(json.dumps(r), flush=True)
    results.append(r)
