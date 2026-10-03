#!/usr/bin/env python3
"""Seeded-defect recall + negative controls for `graph query issues/impact`.

Copies source trees into the (gitignored) bench workspace, ingests a clean
baseline, injects known defects, re-ingests, and diffs findings with
--baseline: every seed must appear as a *new* finding, and negative controls
(lazy Python cycle, Rust `mod tests` dev-dep) must not.
"""
import json, os, shutil, subprocess, sys

BIN, WS, SRC = sys.argv[1], sys.argv[2], sys.argv[3]
OUT = os.path.join(WS, "seeded")


def run(*args):
    p = subprocess.run([BIN, *args], capture_output=True, text=True)
    return json.loads(p.stdout) if p.stdout.strip() else {}, p.returncode


def copy(name, src, ignore=(".git", "node_modules", "target", "dist", "build")):
    dst = os.path.join(OUT, name)
    shutil.rmtree(dst, ignore_errors=True)
    shutil.copytree(src, dst, ignore=shutil.ignore_patterns(*ignore), symlinks=False)
    return dst


def write(root, rel, text, append=False):
    path = os.path.join(root, rel)
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "a" if append else "w") as f:
        f.write(text)


def issues_diff(root, seed_fn):
    base, _ = run("graph", "ingest", root, "--workspace", WS, "--keep", "5")
    seed_fn(root)
    head, _ = run("graph", "ingest", root, "--workspace", WS, "--keep", "5")
    out, _ = run("graph", "query", "issues", "--graph", head["id"], "--baseline", base["id"],
                 "--workspace", WS, "--limit", "1000")
    new = [(r["detector"], r["subject"], r.get("evidence", {})) for r in out.get("results", []) if r.get("status") == "new"]
    return new, head["id"], out.get("baseline", {})


def check(label, new, expect, forbid=()):
    got = {(d, s) for d, s, _ in new}
    hits = [e for e in expect if any(d == e[0] and e[1] in s for d, s in got)]
    leaks = [f for f in forbid if any(d == f[0] and f[1] in s for d, s in got)]
    extra = sorted(g for g in got if not any(g[0] == e[0] and e[1] in g[1] for e in expect))
    print(json.dumps({"case": label, "recall": f"{len(hits)}/{len(expect)}", "missed": [e for e in expect if e not in hits],
                      "negativeControlLeaks": leaks, "otherNewFindings": extra[:15], "otherNewCount": len(extra)}))


# ── TypeScript monorepo (excalidraw) ────────────────────────────────────────
ts = copy("tsx", os.path.join(SRC, "tsx"))
def seed_ts(root):
    pkg = "packages/common/src"
    write(root, f"{pkg}/seedCycleA.ts", "import { b } from './seedCycleB';\nexport const a = (): number => b() + 1;\n")
    write(root, f"{pkg}/seedCycleB.ts", "import { a } from './seedCycleA';\nexport const b = (): number => a() - 1;\n")
    write(root, f"{pkg}/index.ts", "\nexport { a as seededCycleEntry } from './seedCycleA';\n", append=True)
    write(root, f"{pkg}/seedDead.ts", "export const seedDeadValue = 42;\n")
    write(root, f"{pkg}/seedHelpers.ts",
          "import missingThing from 'seeded-missing-pkg';\nexport const seedUsed = () => missingThing;\nexport function seedNeverImported() { return 1; }\n")
    write(root, f"{pkg}/index.ts", "export { seedUsed } from './seedHelpers';\n", append=True)
new, gid, _ = issues_diff(ts, seed_ts)
check("typescript-monorepo", new, [
    ("cycle", "seedCycle"),
    ("unreachable-file", "seedDead.ts"),
    ("undeclared-dependency", "seeded-missing-pkg"),
])
imp, _ = run("graph", "query", "impact", "packages/common/src/seedCycleB.ts", "--graph", gid, "--workspace", WS, "--limit", "500")
ids = [r["id"] for r in imp.get("results", [])]
print(json.dumps({"case": "typescript-impact", "seedCycleA affected": "packages/common/src/seedCycleA.ts" in ids,
                  "index affected": "packages/common/src/index.ts" in ids, "affectedFiles": imp.get("summary", {}).get("affectedFiles"),
                  "tests": imp.get("summary", {}).get("testCount")}))

# ── Python app: real cycle vs lazy (function-level) cycle ──────────────────
py = os.path.join(OUT, "pyapp")
shutil.rmtree(py, ignore_errors=True)
write(py, "manage.py", "from app import views\nif __name__ == '__main__':\n    views.run()\n")
write(py, "app/__init__.py", "")
write(py, "app/views.py", "from app import models\ndef run():\n    return models.load()\n")
write(py, "app/models.py", "def load():\n    return 1\n")
def seed_py(root):
    write(root, "app/eager_a.py", "from app import eager_b\nX = 1\n")
    write(root, "app/eager_b.py", "from app import eager_a\nY = 2\n")
    write(root, "app/lazy_a.py", "def f():\n    from app import lazy_b\n    return lazy_b.g()\n")
    write(root, "app/lazy_b.py", "def g():\n    from app import lazy_a\n    return lazy_a.f()\n")
    write(root, "app/views.py", "from app import eager_a, lazy_a\n", append=True)
new, _, _ = issues_diff(py, seed_py)
check("python-lazy-cycle-control", new, [("cycle", "app/eager_a.py")], forbid=[("cycle", "app/lazy_a.py")])

# ── Rust crate: dev-dep in production vs inside `mod tests` ────────────────
rs = os.path.join(OUT, "rscrate")
shutil.rmtree(rs, ignore_errors=True)
write(rs, "Cargo.toml", '[package]\nname = "seeded"\nversion = "0.1.0"\nedition = "2021"\n[dependencies]\nserde = "1"\n[dev-dependencies]\ntempfile = "3"\n')
write(rs, "src/lib.rs", "pub mod a;\npub mod b;\nuse serde::Serialize;\npub fn root() -> u8 { a::one() }\n")
write(rs, "src/a.rs", "pub fn one() -> u8 { 1 }\n")
write(rs, "src/b.rs", "pub fn two() -> u8 { 2 }\n")
def seed_rs(root):
    write(root, "src/a.rs", "use tempfile::tempdir;\npub fn leak() { let _ = tempdir(); }\n", append=True)
    write(root, "src/b.rs", "\n#[cfg(test)]\nmod tests {\n    use tempfile::tempdir;\n    #[test]\n    fn t() { let _ = tempdir(); }\n}\n", append=True)
new, _, _ = issues_diff(rs, seed_rs)
check("rust-dev-dep-control", new, [("dev-dependency-in-production", "tempfile")])
b_sites = [e for d, s, e in new if d == "dev-dependency-in-production"]
print(json.dumps({"case": "rust-test-module-control", "sites": b_sites[0]["sites"] if b_sites else [],
                  "leak": any(site["file"] == "src/b.rs" for e in b_sites for site in e["sites"])}))
