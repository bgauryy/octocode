"""Execute exact reviewed method/guard excerpts, not an installed package or API."""
import ast
import asyncio
import json
import pathlib
import textwrap
from types import SimpleNamespace
from collections.abc import Sequence

root = pathlib.Path(__file__).parent
packet = json.loads((root / "source.json").read_text())
regions = [r["data"]["files"][0]["content"] for r in packet["results"]]
methods = textwrap.dedent(regions[0][regions[0].index('    def list_keys('):])
# Source range may contain a trailing next declaration; select only these AST nodes.
tree = ast.parse(methods)
selected = [n for n in tree.body if isinstance(n, (ast.FunctionDef, ast.AsyncFunctionDef)) and n.name in ("list_keys", "alist_keys")]
assert {n.name for n in selected} == {"list_keys", "alist_keys"}
source = "\n".join(ast.unparse(n) for n in selected)
guards = textwrap.dedent(regions[1])
assert "if self.n < 1:" in guards and "if self.n > 1 and self.streaming:" in guards
rows = []
for fixed in (False, True):
    namespace = {"Sequence": Sequence}
    code = source.replace("if limit:", "if limit is not None:") if fixed else source
    exec(compile(code, "<reviewed-list-methods>", "exec"), namespace)
    manager_type = type("ExtractedManager", (), {k: namespace[k] for k in ("list_keys", "alist_keys")})
    manager = manager_type()
    manager.records = {"a": {"group_id": "x", "updated_at": 1}, "b": {"group_id": "y", "updated_at": 2}, "c": {"group_id": "x", "updated_at": 3}}
    cases = [({}, ["a", "b", "c"]), ({"limit": 0}, []), ({"limit": 1}, ["a"]), ({"limit": 9}, ["a", "b", "c"]), ({"group_ids": ["x"], "limit": 1}, ["a"]), ({"before": 3, "after": 1}, ["b"]), ({"group_ids": ["missing"], "limit": 0}, [])]
    failures = []
    for kwargs, expected in cases:
        for mode in ("sync", "async"):
            got = manager.list_keys(**kwargs) if mode == "sync" else asyncio.run(manager.alist_keys(**kwargs))
            if got != expected:
                failures.append({"mode": mode, "args": kwargs, "actual": got, "expected": expected})
    rows.append({"case": "L40592", "fixed": fixed, "checks": 14, "failures": failures})
    assert len(failures) == (0 if fixed else 2)
for fixed in (False, True):
    guard = guards.replace("if self.n > 1 and self.streaming:", "if self.n > 1:") if fixed else guards
    ns = {}
    exec("def guard(self):\n" + textwrap.indent(guard, "    "), ns)
    failures = []
    for n in (-1, 0, 1, 2, 4):
        for streaming in (False, True):
            try:
                ns["guard"](SimpleNamespace(n=n, streaming=streaming))
                rejects = False
            except ValueError:
                rejects = True
            if rejects != (n != 1):
                failures.append({"n": n, "streaming": streaming, "rejects": rejects})
    rows.append({"case": "L40590", "fixed": fixed, "checks": 10, "failures": failures})
    assert len(failures) == (0 if fixed else 2)
(root / "results.json").write_text(json.dumps({"fidelity": "Exact source method/guard extraction; not full LangChain/Pydantic/provider runtime", "results": rows}, indent=2) + "\n")
print(json.dumps(rows, indent=2))
