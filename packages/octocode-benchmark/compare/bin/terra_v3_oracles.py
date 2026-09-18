#!/usr/bin/env python3
"""Materialize and seal deterministic public oracles for the Terra benchmark.

Repository source is treated as inert data.  The materializer runs only pinned search
specialists, Git plumbing, and its own byte readers; it never executes corpus code.
Unavailable specialists produce typed gaps and ``--require-complete`` fails closed.
"""

from __future__ import annotations

import argparse
import difflib
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
from typing import Any, Iterable


SUITE_VERSION = 4
NORMALIZATION = {
    "version": "terra-public-oracle-v1",
    "paths": "repository-relative-posix",
    "lines": "one-based",
    "columns": "zero-based-unicode-scalar",
    "ordering": "canonical-json-lexicographic",
    "patch": "unified-diff-a-b-prefix-lf",
}
REPOSITORIES = {
    "langchain-ai/langchain": "67ee6cb63dd9ae7f3a4dfedc3095652bce15a125",
    "vercel/next.js": "d155ba9ebfffe4742efefda8d68c2e0e8e490924",
}
SUPPORTED_ORACLES = {
    "python-regex", "ast-grep-json", "ast-grep-preview", "direct-lsp",
    "verified-source-facts",
}
HEX64 = re.compile(r"^[0-9a-f]{64}$")
EXCLUDED_PARTS = {".git", "node_modules", "dist", "build", "out", "target", ".next"}


def _canonical_bytes(value: object) -> bytes:
    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=False).encode("utf-8")


def digest_record(value: object) -> str:
    return hashlib.sha256(_canonical_bytes(value)).hexdigest()


def seal_record(value: dict[str, object]) -> dict[str, object]:
    sealed = dict(value)
    sealed["receiptDigest"] = digest_record(value)
    return sealed


def _gap(case_id: str, oracle: str, code: str, detail: str) -> dict[str, object]:
    return {"caseId": case_id, "status": "gap", "gap": {"code": code, "oracle": oracle, "detail": detail}}


def validate_suite_materializers(document: object) -> list[str]:
    if not isinstance(document, dict):
        return ["suite must be an object"]
    errors: list[str] = []
    if document.get("version") != SUITE_VERSION:
        errors.append(f"public suite version must be {SUITE_VERSION}")
    if document.get("split") != "public":
        errors.append("suite split must be public")
    if document.get("normalizationVersion") != NORMALIZATION["version"]:
        errors.append("suite normalizationVersion is not supported")
    cases = document.get("cases")
    if not isinstance(cases, list):
        return errors + ["cases must be a list"]
    expected_ids = [f"p{i:02}" for i in range(1, 21)]
    actual_ids: list[str] = []
    for index, case in enumerate(cases):
        if not isinstance(case, dict):
            errors.append(f"case {index} must be an object")
            continue
        case_id = str(case.get("id", ""))
        actual_ids.append(case_id[:3])
        anchor = case.get("anchor")
        if not isinstance(anchor, dict):
            errors.append(f"{case_id or index} lacks anchor")
            continue
        oracle = anchor.get("oracle")
        if oracle not in SUPPORTED_ORACLES:
            errors.append(f"{case_id} has unsupported oracle {oracle!r}")
        if anchor.get("kind") not in {"exact-result-set", "fact-set", "patch-hash"}:
            errors.append(f"{case_id} has unsupported anchor kind")
        if oracle == "python-regex" and not isinstance(anchor.get("query"), str):
            errors.append(f"{case_id} python-regex lacks query")
        if oracle == "ast-grep-json" and not isinstance(anchor.get("patterns"), list):
            errors.append(f"{case_id} ast-grep-json lacks patterns")
        if oracle in {"ast-grep-json", "ast-grep-preview"} and not (
            isinstance(anchor.get("language"), str)
            or isinstance(anchor.get("languages"), list) and anchor.get("languages")
        ):
            errors.append(f"{case_id} ast-grep anchor lacks language(s)")
        if oracle == "ast-grep-preview" and not all(isinstance(anchor.get(key), str) for key in ("pattern", "rewrite")):
            errors.append(f"{case_id} ast-grep-preview is incomplete")
        if oracle == "direct-lsp" and not isinstance(anchor.get("request"), dict):
            errors.append(f"{case_id} direct-lsp lacks request")
        if oracle == "verified-source-facts" and not isinstance(anchor.get("facts"), list):
            errors.append(f"{case_id} verified-source-facts lacks facts")
    if len(cases) != 20 or actual_ids != expected_ids:
        errors.append("cases must be ordered p01 through p20 exactly")
    return errors


def _iter_files(root: Path, roots: Iterable[str], extensions: set[str]) -> Iterable[Path]:
    for relative_root in roots:
        start = (root / relative_root).resolve()
        try:
            start.relative_to(root.resolve())
        except ValueError:
            continue
        if start.is_file():
            candidates = [start]
        elif start.is_dir():
            discovered: list[Path] = []
            for current, dirs, files in os.walk(start, topdown=True, followlinks=False):
                dirs[:] = sorted(name for name in dirs if name not in EXCLUDED_PARTS)
                discovered.extend(Path(current) / name for name in sorted(files))
            candidates = discovered
        else:
            continue
        for path in candidates:
            if path.is_file() and not path.is_symlink() and (not extensions or path.suffix in extensions):
                yield path


def _python_regex(case: dict[str, object], roots: dict[str, Path]) -> list[dict[str, object]]:
    anchor = case["anchor"]
    assert isinstance(anchor, dict)
    expression = re.compile(str(anchor["query"]), re.MULTILINE)
    extensions = set(anchor.get("includeExtensions", []))
    matches: list[dict[str, object]] = []
    for repo_name in case["repositories"]:
        repo = roots[str(repo_name)].resolve()
        for path in _iter_files(repo, anchor.get("roots", ["."]), extensions):
            text = path.read_text(encoding="utf-8", errors="replace")
            for line_number, line in enumerate(text.splitlines(), 1):
                for match in expression.finditer(line):
                    matches.append({
                        "repository": repo_name, "path": path.relative_to(repo).as_posix(),
                        "line": line_number, "column": match.start(), "match": match.group(0),
                    })
    return sorted(matches, key=lambda item: _canonical_bytes(item))


def _tool_path(name: str, tools: dict[str, str]) -> str | None:
    return tools.get(name) or shutil.which(name)


def _run_ast_grep(
    case: dict[str, object], roots: dict[str, Path], tools: dict[str, str], *, rewrite: bool,
) -> tuple[list[dict[str, object]], list[dict[str, object]]]:
    anchor = case["anchor"]
    assert isinstance(anchor, dict)
    executable = _tool_path("ast-grep", tools) or _tool_path("sg", tools)
    if not executable:
        raise FileNotFoundError("ast-grep")
    patterns = [anchor["pattern"]] if rewrite else anchor["patterns"]
    languages = anchor.get("languages", [anchor.get("language")])
    results: list[dict[str, object]] = []
    raw_matches: list[dict[str, object]] = []
    for repo_name in case["repositories"]:
        repo = roots[str(repo_name)].resolve()
        for relative_root in anchor.get("roots", ["."]):
            search_root = (repo / relative_root).resolve()
            for language in languages:
                for pattern in patterns:
                    argv = [executable, "run", "--pattern", str(pattern), "--lang", str(language), "--json=stream"]
                    if rewrite:
                        argv.extend(["--rewrite", str(anchor["rewrite"])])
                    argv.append(str(search_root))
                    process = subprocess.run(argv, text=True, capture_output=True, check=False)
                    if process.returncode not in {0, 1}:
                        raise RuntimeError(process.stderr.strip() or f"ast-grep exited {process.returncode}")
                    for row in process.stdout.splitlines():
                        if not row.strip():
                            continue
                        item = json.loads(row)
                        path = Path(item["file"]).resolve()
                        normalized = {
                            "repository": repo_name, "path": path.relative_to(repo).as_posix(),
                            "line": int(item["range"]["start"]["line"]) + 1,
                            "column": int(item["range"]["start"]["column"]),
                            "endLine": int(item["range"]["end"]["line"]) + 1,
                            "endColumn": int(item["range"]["end"]["column"]),
                            "textDigest": hashlib.sha256(item["text"].encode("utf-8")).hexdigest(),
                        }
                        meta = item.get("metaVariables", {}).get("single", {})
                        captures = {key: value.get("text") for key, value in sorted(meta.items()) if isinstance(value, dict)}
                        if captures:
                            normalized["captures"] = captures
                        results.append(normalized)
                        if rewrite:
                            raw_matches.append({**item, "repository": repo_name, "relativePath": normalized["path"]})
    unique = {_canonical_bytes(item): item for item in results}
    raw_unique: dict[tuple[object, ...], dict[str, object]] = {}
    for item in raw_matches:
        offsets = item.get("replacementOffsets", item["range"]["byteOffset"])
        key = (
            item["repository"], item["relativePath"], offsets["start"], offsets["end"],
            item.get("replacement"),
        )
        raw_unique[key] = item
    return [unique[key] for key in sorted(unique)], [raw_unique[key] for key in sorted(raw_unique)]


def _preview_patch(raw_matches: list[dict[str, object]], roots: dict[str, Path]) -> str:
    by_file: dict[tuple[str, str], list[dict[str, object]]] = {}
    for match in raw_matches:
        by_file.setdefault((str(match["repository"]), str(match["relativePath"])), []).append(match)
    chunks: list[str] = []
    for (repo_name, relative), matches in sorted(by_file.items()):
        path = roots[repo_name] / relative
        original = path.read_text(encoding="utf-8")
        encoded = original.encode("utf-8")
        edits: list[tuple[int, int, bytes]] = []
        for match in matches:
            offsets = match.get("replacementOffsets", match["range"]["byteOffset"])
            edits.append((int(offsets["start"]), int(offsets["end"]), str(match["replacement"]).encode("utf-8")))
        for start, end, replacement in sorted(edits, reverse=True):
            encoded = encoded[:start] + replacement + encoded[end:]
        updated = encoded.decode("utf-8")
        chunks.extend(difflib.unified_diff(
            original.splitlines(keepends=True), updated.splitlines(keepends=True),
            fromfile=f"a/{repo_name}/{relative}", tofile=f"b/{repo_name}/{relative}", lineterm="\n",
        ))
    return "".join(chunks)


def _verified_facts(case: dict[str, object], roots: dict[str, Path]) -> tuple[list[dict[str, object]] | None, str | None]:
    anchor = case["anchor"]
    assert isinstance(anchor, dict)
    output: list[dict[str, object]] = []
    for fact in anchor["facts"]:
        repo_name = str(fact["repository"])
        path = roots[repo_name] / str(fact["path"])
        if not path.is_file() or path.is_symlink():
            return None, f"missing evidence file {repo_name}:{fact['path']}"
        lines = path.read_text(encoding="utf-8", errors="replace").splitlines()
        line_number = int(fact["line"])
        if line_number < 1 or line_number > len(lines) or str(fact["contains"]) not in lines[line_number - 1]:
            return None, f"evidence mismatch at {repo_name}:{fact['path']}:{line_number}"
        output.append({
            "id": fact["id"], "claim": fact["claim"], "repository": repo_name,
            "path": fact["path"], "line": line_number,
            "lineDigest": hashlib.sha256(lines[line_number - 1].encode("utf-8")).hexdigest(),
        })
    return sorted(output, key=lambda item: _canonical_bytes(item)), None


def _normalize_lsp_locations(value: object, repo_name: str, repo: Path) -> list[dict[str, object]]:
    items = value if isinstance(value, list) else [value]
    normalized: list[dict[str, object]] = []
    for item in items:
        if not isinstance(item, dict):
            continue
        uri = item.get("uri") or item.get("targetUri")
        location_range = item.get("range") or item.get("targetSelectionRange")
        if not isinstance(uri, str) or not uri.startswith("file:") or not isinstance(location_range, dict):
            continue
        from urllib.parse import unquote, urlparse

        path = Path(unquote(urlparse(uri).path)).resolve()
        try:
            relative = path.relative_to(repo).as_posix()
        except ValueError:
            relative = uri
        start = location_range.get("start", {})
        end = location_range.get("end", {})
        normalized.append({
            "repository": repo_name, "path": relative,
            "line": int(start.get("line", 0)) + 1, "column": int(start.get("character", 0)),
            "endLine": int(end.get("line", 0)) + 1, "endColumn": int(end.get("character", 0)),
        })
    return sorted(normalized, key=lambda item: _canonical_bytes(item))


def _direct_lsp(case: dict[str, object], roots: dict[str, Path], tools: dict[str, str]) -> list[dict[str, object]]:
    from terra_v3_lsp_client import run

    anchor = case["anchor"]
    assert isinstance(anchor, dict)
    request = anchor["request"]
    assert isinstance(request, dict)
    repo_name = str(case["repositories"][0])
    repo = roots[repo_name].resolve()
    server = str(anchor["server"])
    executable = _tool_path(server, tools)
    if not executable:
        raise FileNotFoundError(server)
    path = (repo / str(request["path"])).resolve()
    params: dict[str, object] = {
        "textDocument": {"uri": path.as_uri()},
        "position": {"line": int(request["line"]), "character": int(request["character"])},
    }
    if request.get("method") == "textDocument/references":
        params["context"] = {"includeDeclaration": bool(request.get("includeDeclaration", True))}
    response = run([executable, "--stdio"], repo, str(request["method"]), params, 60.0)["response"]
    if not isinstance(response, dict) or "error" in response:
        raise RuntimeError(f"direct LSP request failed: {response}")
    return _normalize_lsp_locations(response.get("result"), repo_name, repo)


def materialize_case(
    case: dict[str, object], roots: dict[str, Path], *, tools: dict[str, str] | None = None,
) -> dict[str, object]:
    tools = tools or {}
    case_id = str(case.get("id", ""))
    anchor = case.get("anchor", {})
    oracle = str(anchor.get("oracle", "")) if isinstance(anchor, dict) else ""
    try:
        if oracle == "python-regex":
            answer: object = _python_regex(case, roots)
        elif oracle == "ast-grep-json":
            answer, _ = _run_ast_grep(case, roots, tools, rewrite=False)
        elif oracle == "ast-grep-preview":
            matches, raw = _run_ast_grep(case, roots, tools, rewrite=True)
            patch = _preview_patch(raw, roots)
            if not matches or not patch:
                return _gap(case_id, oracle, "empty-oracle", "rewrite produced no matches or patch")
            answer = {"matches": matches, "patch": patch, "patchHash": hashlib.sha256(patch.encode("utf-8")).hexdigest()}
        elif oracle == "verified-source-facts":
            answer, drift = _verified_facts(case, roots)
            if drift:
                return _gap(case_id, oracle, "evidence-drift", drift)
        elif oracle == "direct-lsp":
            answer = _direct_lsp(case, roots, tools)
        else:
            return _gap(case_id, oracle, "unsupported-oracle", f"unsupported oracle: {oracle}")
    except FileNotFoundError as exc:
        return _gap(case_id, oracle, "tool-unavailable", f"required specialist is unavailable: {exc}")
    except (OSError, ValueError, KeyError, json.JSONDecodeError, RuntimeError) as exc:
        return _gap(case_id, oracle, "materialization-error", str(exc))
    assert answer is not None
    count = len(answer) if isinstance(answer, list) else len(answer.get("matches", [])) if isinstance(answer, dict) else 0
    if count == 0:
        return _gap(case_id, oracle, "empty-oracle", "deterministic oracle produced zero results")
    return seal_record({
        "caseId": case_id, "status": "materialized", "kind": anchor.get("kind"),
        "oracle": oracle, "resultCount": count, "answer": answer, "answerDigest": digest_record(answer),
    })


def materialize_public_oracles(
    suite: dict[str, object], roots: dict[str, Path], *, commits: dict[str, str] | None = None,
    corpus_receipt_digest: str, tools: dict[str, str] | None = None,
) -> dict[str, object]:
    errors = validate_suite_materializers(suite)
    if errors:
        raise ValueError("; ".join(errors))
    commits = commits or REPOSITORIES
    cases = [materialize_case(case, roots, tools=tools) for case in suite["cases"]]
    tool_receipts: list[dict[str, object]] = []
    requested_tools = {"ast-grep"}
    requested_tools.update(
        str(case["anchor"]["server"])
        for case in suite["cases"]
        if case["anchor"].get("oracle") == "direct-lsp"
    )
    for name in sorted(requested_tools):
        executable = _tool_path(name, tools or {})
        if executable:
            version = subprocess.run([executable, "--version"], text=True, capture_output=True, check=False).stdout.strip()
            tool_receipts.append(seal_record({
                "tool": name, "argv0": str(Path(executable).resolve()), "versionOutput": version,
                "executableDigest": hashlib.sha256(Path(executable).read_bytes()).hexdigest(),
            }))
    normalization = seal_record(dict(NORMALIZATION))
    unsigned: dict[str, object] = {
        "version": SUITE_VERSION, "status": "complete" if all(item["status"] == "materialized" for item in cases) else "incomplete",
        "suiteDigest": digest_record(suite), "corpusReceiptDigest": corpus_receipt_digest,
        "repositories": [{"repo": name, "commit": commits[name]} for name in sorted(commits)],
        "normalizationReceipt": {**normalization, "digest": normalization["receiptDigest"]},
        "toolReceipts": tool_receipts, "cases": cases,
    }
    return seal_record(unsigned)


def validate_oracle_receipt(receipt: object, suite: dict[str, object]) -> list[str]:
    if not isinstance(receipt, dict):
        return ["oracle receipt must be an object"]
    errors: list[str] = []
    unsigned = {key: value for key, value in receipt.items() if key != "receiptDigest"}
    if receipt.get("receiptDigest") != digest_record(unsigned):
        errors.append("oracle receipt digest does not bind its contents")
    if receipt.get("suiteDigest") != digest_record(suite):
        errors.append("oracle receipt suite digest mismatch")
    if not HEX64.fullmatch(str(receipt.get("corpusReceiptDigest", ""))):
        errors.append("oracle receipt lacks corpus receipt digest")
    cases = receipt.get("cases")
    if not isinstance(cases, list) or len(cases) != len(suite.get("cases", [])):
        errors.append("oracle receipt case set mismatch")
    else:
        for item in cases:
            if item.get("status") == "materialized":
                case_unsigned = {key: value for key, value in item.items() if key != "receiptDigest"}
                if item.get("receiptDigest") != digest_record(case_unsigned):
                    errors.append(f"{item.get('caseId')} receipt digest mismatch")
                if item.get("answerDigest") != digest_record(item.get("answer")):
                    errors.append(f"{item.get('caseId')} answer digest mismatch")
    return errors


def _git_head(path: Path) -> str:
    process = subprocess.run(["git", "-c", "core.hooksPath=/dev/null", "rev-parse", "HEAD^{commit}"], cwd=path, text=True, capture_output=True, check=False)
    if process.returncode != 0:
        raise ValueError(f"cannot resolve Git HEAD for {path}: {process.stderr.strip()}")
    return process.stdout.strip()


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--suite", type=Path, required=True)
    parser.add_argument("--langchain", type=Path, required=True)
    parser.add_argument("--nextjs", type=Path, required=True)
    parser.add_argument("--corpus-lock", type=Path)
    parser.add_argument("--corpus-receipt-digest")
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--require-complete", action="store_true")
    args = parser.parse_args()
    suite = json.loads(args.suite.read_text(encoding="utf-8"))
    roots = {"langchain-ai/langchain": args.langchain.resolve(), "vercel/next.js": args.nextjs.resolve()}
    commits = {name: _git_head(path) for name, path in roots.items()}
    drift = [f"{name}: expected {REPOSITORIES[name]}, got {commit}" for name, commit in commits.items() if commit != REPOSITORIES[name]]
    if drift:
        print(json.dumps({"status": "invalid-corpus", "errors": drift}, sort_keys=True), file=sys.stderr)
        return 2
    if args.corpus_lock:
        corpus_digest = hashlib.sha256(args.corpus_lock.read_bytes()).hexdigest()
    elif args.corpus_receipt_digest:
        corpus_digest = args.corpus_receipt_digest
    else:
        parser.error("one of --corpus-lock or --corpus-receipt-digest is required")
    receipt = materialize_public_oracles(suite, roots, commits=commits, corpus_receipt_digest=corpus_digest)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(receipt, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(json.dumps({"status": receipt["status"], "output": str(args.output), "receiptDigest": receipt["receiptDigest"]}, sort_keys=True))
    return 1 if args.require_complete and receipt["status"] != "complete" else 0


if __name__ == "__main__":
    raise SystemExit(main())
