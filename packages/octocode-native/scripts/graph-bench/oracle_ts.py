#!/usr/bin/env python3
"""Ground-truth oracle for TypeScript import edges.

Compares the graph's file->file `imports` edges with the module resolutions
`tsc --traceResolution` reports for the same project. Both sides are limited
to importers in the tsc program and targets inside the project directory
(no node_modules, no .d.ts).

    python3 oracle_ts.py <octocode-bin> <workspace> <ts-project-dir> [tsconfig]

tsc runs only from a local install: <project>/node_modules/.bin/tsc or the
nearest ancestor's. It never downloads one.
"""
import json, os, re, subprocess, sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from graphbin import decode, file_imports  # noqa: E402

BIN, WS, PROJECT = sys.argv[1], os.path.abspath(sys.argv[2]), os.path.realpath(sys.argv[3])
TSCONFIG = os.path.join(PROJECT, sys.argv[4] if len(sys.argv) > 4 else "tsconfig.json")
EXAMPLES = 5

RESOLVING = re.compile(r"^======== Resolving module '(.+)' from '(.+)'\. ========$")
RESOLVED = re.compile(r"^======== Module name '(.+)' was successfully resolved to '(.+?)'(?: with Package ID '.*')?\. ========$")


def local_tsc(start):
    d = start
    while True:
        cand = os.path.join(d, "node_modules", ".bin", "tsc")
        if os.path.exists(cand):
            return cand
        parent = os.path.dirname(d)
        if parent == d:
            return None
        d = parent


def inside(path):
    """Project-relative path, or None for files the oracle does not score."""
    real = os.path.realpath(path)
    if not real.startswith(PROJECT + os.sep) or "/node_modules/" in real or real.endswith(".d.ts"):
        return None
    return os.path.relpath(real, PROJECT)


def tsc_edges(tsc):
    p = subprocess.run([tsc, "--traceResolution", "--noEmit", "--listFiles", "-p", TSCONFIG],
                       capture_output=True, text=True, cwd=PROJECT)
    edges, specs, program = set(), {}, set()
    pending = {}
    for line in p.stdout.splitlines():
        m = RESOLVING.match(line)
        if m:
            pending[m.group(1)] = m.group(2)
            continue
        m = RESOLVED.match(line)
        if m:
            importer = pending.pop(m.group(1), None)
            src, dst = importer and inside(importer), inside(m.group(2))
            if src and dst and src != dst:
                edges.add((src, dst))
                specs.setdefault((src, dst), m.group(1))
            continue
        if line.startswith("/") and not line.startswith("======"):
            rel = inside(line.strip())
            if rel:
                program.add(rel)
    if not program:
        sys.exit(f"tsc produced no program files (exit {p.returncode}): {p.stdout[-500:]}{p.stderr[-500:]}")
    return edges, specs, program


def graph_edges():
    p = subprocess.run([BIN, "graph", "ingest", PROJECT, "--workspace", WS, "--keep", "1"],
                       capture_output=True, text=True)
    if p.returncode != 0:
        sys.exit(f"graph ingest failed: {p.stdout[-500:]}{p.stderr[-500:]}")
    gid = json.loads(p.stdout)["id"]
    nodes, edges = decode(os.path.join(WS, ".octocode", "graph", gid, "graph.bin"))
    # Keys are relative to the ingest root; normalize through realpath like tsc.
    out, lines = set(), {}
    for src, dst in file_imports(nodes, edges):
        a, b = inside(os.path.join(PROJECT, src)), inside(os.path.join(PROJECT, dst))
        if a and b and a != b:
            out.add((a, b))
    for e in edges:
        if e["kind"] == 1 and nodes[e["src"]]["kind"] == 0 and nodes[e["dst"]]["kind"] == 0:
            lines.setdefault((nodes[e["src"]]["key"], nodes[e["dst"]]["key"]), (e["line"], e["detail"]))
    return out, lines, gid


def main():
    tsc = local_tsc(PROJECT)
    if not tsc:
        sys.exit("no local typescript (node_modules/.bin/tsc) in the project or its ancestors; skipping")
    oracle, specs, program = tsc_edges(tsc)
    ours_all, lines, gid = graph_edges()
    ours = {e for e in ours_all if e[0] in program}
    tp = ours & oracle
    fp = sorted(ours - oracle)
    fn = sorted(oracle - ours)
    report = {
        "project": PROJECT,
        "tsc": tsc,
        "graph": gid,
        "programFiles": len(program),
        "oracleEdges": len(oracle),
        "graphEdges": len(ours),
        "graphEdgesOutsideProgram": len(ours_all - ours),
        "truePositive": len(tp),
        "precision": round(len(tp) / len(ours), 4) if ours else None,
        "recall": round(len(tp) / len(oracle), 4) if oracle else None,
        "falsePositives": [
            {"from": a, "to": b, "line": lines.get((a, b), (None,))[0], "detail": lines.get((a, b), (None, None))[1]}
            for a, b in fp[:EXAMPLES]
        ],
        "falseNegatives": [{"from": a, "to": b, "specifier": specs.get((a, b))} for a, b in fn[:EXAMPLES]],
        "falsePositiveCount": len(fp),
        "falseNegativeCount": len(fn),
    }
    print(json.dumps(report, indent=2))


main()
