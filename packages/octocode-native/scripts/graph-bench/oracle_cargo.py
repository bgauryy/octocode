#!/usr/bin/env python3
"""Ground-truth oracle for Rust crate edges.

`cargo metadata` gives each workspace member's path dependencies on other
members. The graph gives crate edges as imports between files of different
cargo components (FCMP section). An edge the graph reports that no manifest
declares is a false positive. A declared dependency the graph never sees
lowers recall; that can also mean the manifest declares an unused crate.

    python3 oracle_cargo.py <octocode-bin> <workspace> <cargo-root>
"""
import json, os, re, subprocess, sys

from graphbin import EDGE_IMPORTS, NODE_FILE, NODE_PACKAGE, components, decode

BIN, WS, ROOT = sys.argv[1], os.path.abspath(sys.argv[2]), os.path.realpath(sys.argv[3])
EXAMPLES = 5


def crate(name):
    return name.replace("-", "_")


def patched_members(manifest):
    """Crate names that the root `[patch.*]` tables point at a local path.

    A line-level reader: Python 3.9 has no tomllib, and patch entries are
    one-line inline tables in practice.
    """
    out, in_patch = set(), False
    for line in open(manifest, encoding="utf-8"):
        line = line.strip()
        if line.startswith("["):
            in_patch = line.startswith("[patch.")
        elif in_patch:
            m = re.match(r'^"?([A-Za-z0-9_-]+)"?\s*=\s*\{.*\bpath\s*=', line)
            if m:
                out.add(crate(m.group(1)))
    return out


def cargo_edges():
    cmd = ["cargo", "metadata", "--format-version", "1", "--offline"]
    p = subprocess.run(cmd, capture_output=True, text=True, cwd=ROOT)
    if p.returncode != 0:
        # Offline resolution needs every registry crate cached; declared
        # dependencies alone do not.
        p = subprocess.run(cmd + ["--no-deps"], capture_output=True, text=True, cwd=ROOT)
    if p.returncode != 0:
        sys.exit(f"cargo metadata failed: {p.stderr[-500:]}")
    meta = json.loads(p.stdout)
    members = set(meta["workspace_members"])
    pkgs = [pkg for pkg in meta["packages"] if pkg["id"] in members]
    names = {crate(pkg["name"]) for pkg in pkgs}
    # `[patch.crates-io] foo = { path = "foo" }` in the workspace root makes a
    # registry-style `foo = "1"` resolve to the member (tokio does this).
    patched = patched_members(os.path.join(meta["workspace_root"], "Cargo.toml"))
    edges = {}  # (from, to) -> set of kinds
    for pkg in pkgs:
        for dep in pkg["dependencies"]:
            internal = dep.get("path") or crate(dep["name"]) in patched
            if internal and crate(dep["name"]) in names:
                kind = dep.get("kind") or "normal"
                edges.setdefault((crate(pkg["name"]), crate(dep["name"])), set()).add(kind)
    return names, edges


def graph_edges(names):
    p = subprocess.run([BIN, "graph", "ingest", ROOT, "--workspace", WS, "--keep", "1"],
                       capture_output=True, text=True)
    if p.returncode != 0:
        sys.exit(f"graph ingest failed: {p.stdout[-500:]}{p.stderr[-500:]}")
    gid = json.loads(p.stdout)["id"]
    path = os.path.join(WS, ".octocode", "graph", gid, "graph.bin")
    nodes, edges = decode(path)
    comp = {node: name for node, (_, name, meta) in components(path).items()
            if meta.split(";")[0] == "cargo"}
    found, witness, via_package = {}, {}, 0
    for e in edges:
        if e["kind"] != EDGE_IMPORTS or nodes[e["src"]]["kind"] != NODE_FILE:
            continue
        src = comp.get(e["src"])
        dst_node = nodes[e["dst"]]
        if dst_node["kind"] == NODE_FILE:
            dst = comp.get(e["dst"])
        elif dst_node["kind"] == NODE_PACKAGE and dst_node["key"][4:] in names:
            # A member crate left as an external package node.
            dst = dst_node["key"][4:]
            via_package += 1
        else:
            continue
        if src and dst and src != dst:
            found[(src, dst)] = found.get((src, dst), 0) + 1
            witness.setdefault((src, dst), f'{nodes[e["src"]]["key"]}:{e["line"]} -> {dst_node["key"]}')
    return gid, found, witness, via_package, comp


def main():
    names, declared = cargo_edges()
    gid, found, witness, via_package, comp = graph_edges(names)
    # Crates outside the workspace (excluded fuzz crates, vendored examples)
    # have no manifest in `cargo metadata`; they are reported, not scored.
    outside = sorted(e for e in found if e[0] not in names or e[1] not in names)
    ours = {e for e in found if e[0] in names and e[1] in names}
    everything = set(declared)
    production = {e for e, kinds in declared.items() if kinds & {"normal", "build"}}
    tp_all, tp_prod = ours & everything, ours & production
    fp = sorted(ours - everything)
    fn = sorted(production - ours)
    report = {
        "cargoRoot": ROOT,
        "graph": gid,
        "members": sorted(names),
        "graphCrateEdges": len(ours),
        "graphEdgesViaPackageNode": via_package,
        "declaredEdges": len(everything),
        "declaredProductionEdges": len(production),
        # Any declared kind (normal, build, dev) justifies an edge: the graph
        # links test files too.
        "precision": round(len(tp_all) / len(ours), 4) if ours else None,
        # Every normal/build dependency should show up as an import.
        "recall": round(len(tp_prod) / len(production), 4) if production else None,
        "recallAllKinds": round(len(tp_all) / len(everything), 4) if everything else None,
        "falsePositives": [{"from": a, "to": b, "imports": found[(a, b)], "witness": witness[(a, b)]}
                           for a, b in fp[:EXAMPLES]],
        "falseNegatives": [{"from": a, "to": b, "kinds": sorted(declared[(a, b)])} for a, b in fn[:EXAMPLES]],
        "falsePositiveCount": len(fp),
        "falseNegativeCount": len(fn),
        "unusedDevEdges": sorted(f"{a}->{b}" for a, b in everything - production - ours),
        "outsideWorkspace": [{"from": a, "to": b, "witness": witness[(a, b)]} for a, b in outside[:EXAMPLES]],
    }
    print(json.dumps(report, indent=2))


main()
