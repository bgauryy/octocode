#!/usr/bin/env python3
"""Workspace-only and two-repository identity gate for benchmark generation v3.

This module deliberately uses only Git plumbing and byte reads. It never runs code from
the benchmark repositories. Campaign orchestration may call ``prepare``; unit tests use
the pure receipt builders and negative controls.
"""

from __future__ import annotations

import argparse
from collections import defaultdict
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import sys
import time
from typing import Iterable


LOCK_VERSION = 3
SHA1_LENGTH = 40
SHA256_LENGTH = 64
EXCLUSIONS = (".git", "node_modules", "dist", "build", "out", "target", ".next")
CONTROL_EXCLUSIONS = (".octocode-clone-meta.json",)
REPOSITORIES = (
    {"repo": "langchain-ai/langchain", "ref": "master", "directory": "langchain", "url": "https://github.com/langchain-ai/langchain.git"},
    {"repo": "vercel/next.js", "ref": "canary", "directory": "nextjs", "url": "https://github.com/vercel/next.js.git"},
)
FROZEN_COMMITS = {
    "langchain-ai/langchain": "67ee6cb63dd9ae7f3a4dfedc3095652bce15a125",
    "vercel/next.js": "d155ba9ebfffe4742efefda8d68c2e0e8e490924",
}
DEFAULT_PUBLIC_ORACLES = Path(__file__).resolve().parent.parent / "terra-v3/suite/public-oracles.json"
BUILD_COMMANDS = (
    ("yarn", "workspace", "@octocodeai/octocode-engine", "build:dev"),
    ("yarn", "workspace", "@octocodeai/octocode-tools-core", "build"),
    ("yarn", "workspace", "octocode", "build:dev"),
)
SOURCE_ROOTS = (
    "packages/octocode-engine/src",
    "packages/octocode-tools-core/src",
    "packages/octocode-config/src",
    "packages/octocode/src",
)
SOURCE_FILES = (
    "packages/octocode-engine/Cargo.toml",
    "packages/octocode-engine/build.rs",
    "packages/octocode-engine/package.json",
    "packages/octocode-tools-core/package.json",
    "packages/octocode-config/package.json",
    "packages/octocode/package.json",
    "yarn.lock",
)


def _canonical_core_root(workspace: Path) -> Path:
    return workspace.parent / "octocode-mcp-host/packages/octocode-core"


def _build_commands(workspace: Path) -> list[tuple[tuple[str, ...], Path]]:
    commands: list[tuple[tuple[str, ...], Path]] = []
    core = _canonical_core_root(workspace)
    if core.is_dir():
        commands.append((("yarn", "build"), core))
    commands.extend((command, workspace) for command in BUILD_COMMANDS)
    return commands
LANGUAGES = {
    ".py": "Python", ".pyi": "Python", ".js": "JavaScript", ".jsx": "JavaScript",
    ".mjs": "JavaScript", ".cjs": "JavaScript", ".ts": "TypeScript", ".tsx": "TypeScript",
    ".mts": "TypeScript", ".cts": "TypeScript", ".rs": "Rust", ".go": "Go",
    ".java": "Java", ".kt": "Kotlin", ".rb": "Ruby", ".md": "Markdown",
}


class PreflightError(RuntimeError):
    """A hard benchmark validity failure."""


def _sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def _canonical_bytes(value: object) -> bytes:
    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=False).encode("utf-8")


def digest_record(value: object) -> str:
    return _sha256(_canonical_bytes(value))


def _iter_content_entries(root: Path, exclusions: Iterable[str] = EXCLUSIONS):
    root = root.resolve()
    excluded = set(exclusions)
    for current, dirs, files in os.walk(root, topdown=True, followlinks=False):
        dirs[:] = sorted(name for name in dirs if name not in excluded)
        for name in sorted(files):
            if name in excluded or name in CONTROL_EXCLUSIONS:
                continue
            path = Path(current) / name
            relative = path.relative_to(root).as_posix()
            if path.is_symlink():
                yield relative, b"symlink\0" + os.readlink(path).encode("utf-8", errors="surrogateescape")
            elif path.is_file():
                yield relative, path.read_bytes()


def hash_content_tree(root: Path, exclusions: Iterable[str] = EXCLUSIONS) -> dict[str, object]:
    """Hash path and exact bytes in deterministic order, plus language inventory."""
    digest = hashlib.sha256()
    file_count = 0
    source_bytes = 0
    languages: defaultdict[str, dict[str, int]] = defaultdict(lambda: {"files": 0, "bytes": 0})
    for relative, content in _iter_content_entries(Path(root), exclusions):
        encoded_path = relative.encode("utf-8", errors="surrogateescape")
        digest.update(len(encoded_path).to_bytes(8, "big"))
        digest.update(encoded_path)
        digest.update(len(content).to_bytes(8, "big"))
        digest.update(content)
        file_count += 1
        source_bytes += len(content)
        language = LANGUAGES.get(Path(relative).suffix.lower(), "Other")
        languages[language]["files"] += 1
        languages[language]["bytes"] += len(content)
    return {
        "contentDigest": digest.hexdigest(),
        "fileCount": file_count,
        "sourceBytes": source_bytes,
        "languages": dict(sorted(languages.items())),
    }


def _is_hex(value: object, length: int) -> bool:
    return isinstance(value, str) and len(value) == length and all(c in "0123456789abcdef" for c in value)


def validate_corpus_lock(lock: dict[str, object]) -> list[str]:
    errors: list[str] = []
    if lock.get("version") != LOCK_VERSION:
        errors.append(f"version must be {LOCK_VERSION}")
    repositories = lock.get("repositories")
    if not isinstance(repositories, list):
        return errors + ["repositories must be a list"]
    expected = {(item["repo"], item["ref"]) for item in REPOSITORIES}
    actual = {(item.get("repo"), item.get("ref")) for item in repositories if isinstance(item, dict)}
    if len(repositories) != 2 or actual != expected:
        errors.append("repository set must be exactly langchain-ai/langchain@master and vercel/next.js@canary")
    for index, item in enumerate(repositories):
        if not isinstance(item, dict):
            errors.append(f"repositories[{index}] must be an object")
            continue
        for key in ("commit", "tree"):
            if not _is_hex(item.get(key), SHA1_LENGTH):
                errors.append(f"repositories[{index}].{key} must be a lowercase 40-character git oid")
        if not _is_hex(item.get("contentDigest"), SHA256_LENGTH):
            errors.append(f"repositories[{index}].contentDigest must be sha256")
        if not isinstance(item.get("fileCount"), int) or int(item.get("fileCount", -1)) < 1:
            errors.append(f"repositories[{index}].fileCount must be positive")
        if not isinstance(item.get("sourceBytes"), int) or int(item.get("sourceBytes", -1)) < 1:
            errors.append(f"repositories[{index}].sourceBytes must be positive")
        if not isinstance(item.get("languages"), dict) or not item.get("languages"):
            errors.append(f"repositories[{index}].languages must be non-empty")
    if list(lock.get("exclusions", [])) != list(EXCLUSIONS):
        errors.append("exclusions differ from the frozen v3 list")
    if list(lock.get("controlExclusions", [])) != list(CONTROL_EXCLUSIONS):
        errors.append("clone-control exclusions differ from the frozen v3 list")
    if not _is_hex(lock.get("fixtureManifestDigest"), SHA256_LENGTH):
        errors.append("fixtureManifestDigest must be sha256")
    return errors


def _git(cwd: Path, *args: str) -> str:
    result = subprocess.run(
        ["git", "-c", "core.hooksPath=/dev/null", *args], cwd=cwd,
        text=True, capture_output=True, check=False,
        env={**os.environ, "GIT_TERMINAL_PROMPT": "0"},
    )
    if result.returncode != 0:
        raise PreflightError(f"git {' '.join(args)} failed: {result.stderr.strip()}")
    return result.stdout.strip()


def frozen_oracle_commits(public_oracles: Path = DEFAULT_PUBLIC_ORACLES) -> dict[str, str]:
    """Return the v4 corpus SHAs and reject a checked-in oracle receipt that drifts."""
    expected = dict(FROZEN_COMMITS)
    if not public_oracles.is_file():
        return expected
    try:
        receipt = json.loads(public_oracles.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        raise PreflightError(f"cannot read public oracle receipt {public_oracles}: {exc}") from exc
    repositories = receipt.get("repositories")
    if not isinstance(repositories, list):
        raise PreflightError(f"public oracle receipt has no repository pins: {public_oracles}")
    actual: dict[str, str] = {}
    for item in repositories:
        if not isinstance(item, dict):
            raise PreflightError(f"public oracle receipt has an invalid repository entry: {public_oracles}")
        repo = item.get("repo")
        commit = item.get("commit")
        if not isinstance(repo, str) or not _is_hex(commit, SHA1_LENGTH):
            raise PreflightError(f"public oracle receipt has an invalid repository pin: {public_oracles}")
        if repo in actual:
            raise PreflightError(f"public oracle receipt repeats repository {repo}: {public_oracles}")
        actual[repo] = commit
    if actual != expected:
        raise PreflightError(
            "public oracle repository pins differ from the frozen Terra v4 corpus: "
            f"expected {expected}, got {actual}"
        )
    return expected


def prepare_repositories(
    corpus_root: Path,
    *,
    expected_commits: dict[str, str] | None = None,
) -> None:
    expected_commits = dict(expected_commits or frozen_oracle_commits())
    expected_repositories = {spec["repo"] for spec in REPOSITORIES}
    if set(expected_commits) != expected_repositories:
        raise PreflightError("prepared repository pins must match the exact two-repository corpus")
    corpus_root.mkdir(parents=True, exist_ok=True)
    for spec in REPOSITORIES:
        destination = corpus_root / spec["directory"]
        if destination.exists():
            raise PreflightError(f"refusing to reuse corpus path: {destination}")
        subprocess.run(
            ["git", "-c", "core.hooksPath=/dev/null", "clone", "--no-checkout", "--filter=blob:none", spec["url"], str(destination)],
            check=True, env={**os.environ, "GIT_TERMINAL_PROMPT": "0"},
        )
        expected = expected_commits[spec["repo"]]
        if not _is_hex(expected, SHA1_LENGTH):
            raise PreflightError(f"invalid frozen commit for {spec['repo']}: {expected}")
        _git(destination, "cat-file", "-e", f"{expected}^{{commit}}")
        _git(destination, "checkout", "--detach", expected)
        resolved = _git(destination, "rev-parse", "HEAD^{commit}")
        if resolved != expected:
            raise PreflightError(f"{spec['repo']} checkout {resolved} differs from frozen {expected}")


def _repository_path(corpus_root: Path | None, spec: dict[str, str], repo_paths: dict[str, Path] | None) -> Path:
    if repo_paths and spec["repo"] in repo_paths:
        return repo_paths[spec["repo"]].resolve()
    if corpus_root is None:
        raise PreflightError(f"no path supplied for {spec['repo']}")
    return (corpus_root / spec["directory"]).resolve()


def build_corpus_lock(
    corpus_root: Path | None,
    fixture_manifest_digest: str,
    *,
    repo_paths: dict[str, Path] | None = None,
    expected_commits: dict[str, str] | None = None,
) -> dict[str, object]:
    repositories: list[dict[str, object]] = []
    for spec in REPOSITORIES:
        root = _repository_path(corpus_root, spec, repo_paths)
        if not root.is_dir():
            raise PreflightError(f"missing clone: {root}")
        commit = _git(root, "rev-parse", "HEAD^{commit}")
        tree = _git(root, "rev-parse", "HEAD^{tree}")
        if expected_commits:
            expected = expected_commits.get(spec["repo"])
            if commit != expected:
                raise PreflightError(f"{spec['repo']} HEAD {commit} differs from requested {expected}")
        else:
            branch_commit = _git(root, "rev-parse", f"origin/{spec['ref']}^{{commit}}")
            if commit != branch_commit:
                raise PreflightError(f"{spec['repo']} is not detached at frozen {spec['ref']} commit")
        dirty = [
            line for line in _git(root, "status", "--porcelain", "--untracked-files=all").splitlines()
            if line not in {f"?? {name}" for name in CONTROL_EXCLUSIONS}
        ]
        if dirty:
            raise PreflightError(f"corpus clone is dirty: {root}")
        stats = hash_content_tree(root)
        repositories.append({
            "repo": spec["repo"], "ref": spec["ref"], "commit": commit, "tree": tree,
            "sourcePath": str(root),
            **stats,
        })
    lock = {
        "version": LOCK_VERSION,
        "repositories": repositories,
        "exclusions": list(EXCLUSIONS),
        "controlExclusions": list(CONTROL_EXCLUSIONS),
        "fixtureManifestDigest": fixture_manifest_digest,
    }
    errors = validate_corpus_lock(lock)
    if errors:
        raise PreflightError("; ".join(errors))
    return lock


def verify_corpus_bytes(
    lock: dict[str, object], corpus_root: Path | None, *, repo_paths: dict[str, Path] | None = None
) -> list[str]:
    errors = validate_corpus_lock(lock)
    by_repo = {item["repo"]: item for item in lock.get("repositories", []) if isinstance(item, dict)}
    for spec in REPOSITORIES:
        item = by_repo.get(spec["repo"])
        root = _repository_path(corpus_root, spec, repo_paths)
        if not item or not root.is_dir():
            errors.append(f"missing locked clone for {spec['repo']}")
            continue
        try:
            if _git(root, "rev-parse", "HEAD^{commit}") != item.get("commit"):
                errors.append(f"commit drift for {spec['repo']}")
            if _git(root, "rev-parse", "HEAD^{tree}") != item.get("tree"):
                errors.append(f"tree drift for {spec['repo']}")
            if hash_content_tree(root)["contentDigest"] != item.get("contentDigest"):
                errors.append(f"content digest drift for {spec['repo']}")
            dirty = [
                line for line in _git(root, "status", "--porcelain", "--untracked-files=all").splitlines()
                if line not in {f"?? {name}" for name in CONTROL_EXCLUSIONS}
            ]
            if dirty:
                errors.append(f"corpus clone is dirty: {root}")
        except PreflightError as exc:
            errors.append(str(exc))
    return errors


def _workspace_sources(workspace: Path) -> list[Path]:
    result: list[Path] = []
    for relative in SOURCE_ROOTS:
        root = workspace / relative
        if root.is_dir():
            result.extend(path for path in root.rglob("*") if path.is_file() and not path.is_symlink())
    result.extend(workspace / relative for relative in SOURCE_FILES if (workspace / relative).is_file())
    return sorted(set(result), key=lambda item: item.relative_to(workspace).as_posix())


def _digest_paths(workspace: Path, paths: Iterable[Path]) -> str:
    digest = hashlib.sha256()
    for path in paths:
        relative = path.resolve().relative_to(workspace.resolve()).as_posix().encode("utf-8")
        content = path.read_bytes()
        digest.update(len(relative).to_bytes(8, "big")); digest.update(relative)
        digest.update(len(content).to_bytes(8, "big")); digest.update(content)
    return digest.hexdigest()


def _external_source_receipts(workspace: Path) -> list[dict[str, object]]:
    root = _canonical_core_root(workspace)
    if not root.is_dir():
        return []
    paths = sorted(
        (path for relative in ("src",) for path in (root / relative).rglob("*") if path.is_file() and not path.is_symlink()),
        key=lambda path: path.relative_to(root).as_posix(),
    )
    for name in ("package.json", "yarn.lock"):
        path = root / name
        if path.is_file():
            paths.append(path)
    paths = sorted(set(paths), key=lambda path: path.relative_to(root).as_posix())
    return [{
        "root": str(root.resolve()), "sourceStateDigest": _digest_paths(root, paths),
        "sourceFileCount": len(paths),
        "sourceMaxMtimeNs": max((path.stat().st_mtime_ns for path in paths), default=0),
    }]


def _native_artifacts(workspace: Path) -> list[Path]:
    loader = workspace / "packages/octocode-engine/index.cjs"
    if loader.is_file():
        probe = subprocess.run(
            [
                "node", "-e",
                "require(process.argv[1]); process.stdout.write(JSON.stringify(Object.keys(require.cache).filter(p=>p.endsWith('.node'))))",
                str(loader),
            ],
            cwd=workspace, text=True, capture_output=True, check=False,
        )
        if probe.returncode != 0:
            raise PreflightError(f"workspace native addon did not load: {probe.stderr.strip()}")
        try:
            loaded = [Path(value).resolve() for value in json.loads(probe.stdout)]
        except (json.JSONDecodeError, TypeError) as exc:
            raise PreflightError("workspace native addon probe returned invalid JSON") from exc
        inside = [path for path in loaded if path.is_file() and path.is_relative_to(workspace)]
        if inside:
            return sorted(set(inside))
        raise PreflightError("loaded native addon does not resolve inside the workspace")
    roots = (workspace / "packages/octocode-engine", workspace / "packages/octocode/runtime")
    return sorted({path for root in roots if root.is_dir() for path in root.rglob("*.node") if path.is_file()})


def build_workspace_receipt(workspace: Path, *, catalog_bytes: bytes) -> dict[str, object]:
    workspace = workspace.resolve()
    sources = _workspace_sources(workspace)
    cli = workspace / "packages/octocode/out/octocode.js"
    natives = _native_artifacts(workspace)
    dependency_lock = workspace / "yarn.lock"
    missing = [str(path) for path in (cli, dependency_lock) if not path.is_file()]
    if not sources:
        missing.append("benchmark-relevant workspace sources")
    if not natives:
        missing.append("workspace native .node artifact")
    if missing:
        raise PreflightError("missing workspace inputs: " + ", ".join(missing))
    source_digest = _digest_paths(workspace, sources)
    external_sources = _external_source_receipts(workspace)
    native_entries = [
        {"path": path.relative_to(workspace).as_posix(), "sha256": _sha256(path.read_bytes())}
        for path in natives
    ]
    receipt: dict[str, object] = {
        "version": LOCK_VERSION,
        "workspaceRoot": str(workspace),
        "gitHead": _git(workspace, "rev-parse", "HEAD^{commit}") if (workspace / ".git").exists() else None,
        "sourceStateDigest": source_digest,
        "sourceFileCount": len(sources),
        "sourceMaxMtimeNs": max(path.stat().st_mtime_ns for path in sources),
        "externalSources": external_sources,
        "externalSourceDigest": digest_record(external_sources),
        "dependencyLockDigest": _sha256(dependency_lock.read_bytes()),
        "nativeArtifacts": native_entries,
        "nativeArtifactDigest": digest_record(native_entries),
        "cliArtifactPath": cli.relative_to(workspace).as_posix(),
        "cliArtifactDigest": _sha256(cli.read_bytes()),
        "catalogDigest": _sha256(catalog_bytes),
        "nodeExecutable": shutil.which("node"),
        "nodeVersion": subprocess.run(["node", "--version"], text=True, capture_output=True, check=True).stdout.strip(),
        "platform": {"system": platform.system(), "release": platform.release(), "machine": platform.machine()},
        "buildCommands": [
            {"cwd": str(cwd.resolve()), "argv": list(command)}
            for command, cwd in _build_commands(workspace)
        ],
        "createdAtUnixNs": time.time_ns(),
    }
    receipt["receiptDigest"] = digest_record({key: value for key, value in receipt.items() if key not in {"createdAtUnixNs", "receiptDigest"}})
    return receipt


def validate_workspace_receipt(
    receipt: dict[str, object], workspace: Path, *, catalog_bytes: bytes | None = None
) -> list[str]:
    errors: list[str] = []
    workspace = workspace.resolve()
    if Path(str(receipt.get("workspaceRoot", ""))).resolve() != workspace:
        errors.append("workspace root mismatch")
    expected_receipt_digest = digest_record({
        key: value for key, value in receipt.items() if key not in {"createdAtUnixNs", "receiptDigest"}
    })
    if receipt.get("receiptDigest") != expected_receipt_digest:
        errors.append("workspace receipt digest mismatch")
    sources = _workspace_sources(workspace)
    if not sources or _digest_paths(workspace, sources) != receipt.get("sourceStateDigest"):
        errors.append("source state digest mismatch")
    external_sources = _external_source_receipts(workspace)
    if digest_record(external_sources) != receipt.get("externalSourceDigest"):
        errors.append("external source state digest mismatch")
    cli = workspace / str(receipt.get("cliArtifactPath", "packages/octocode/out/octocode.js"))
    if not cli.is_file() or _sha256(cli.read_bytes()) != receipt.get("cliArtifactDigest"):
        errors.append("cli artifact digest mismatch")
    dependency_lock = workspace / "yarn.lock"
    if not dependency_lock.is_file() or _sha256(dependency_lock.read_bytes()) != receipt.get("dependencyLockDigest"):
        errors.append("dependency lock digest mismatch")
    if catalog_bytes is not None and _sha256(catalog_bytes) != receipt.get("catalogDigest"):
        errors.append("live tool catalog digest mismatch")
    native_entries = receipt.get("nativeArtifacts", [])
    if not isinstance(native_entries, list) or not native_entries:
        errors.append("native artifact receipt missing")
    else:
        current_entries = []
        for item in native_entries:
            path = workspace / str(item.get("path", ""))
            if not path.is_file():
                errors.append(f"native artifact missing: {path}")
                continue
            current_entries.append({"path": item["path"], "sha256": _sha256(path.read_bytes())})
        if current_entries and digest_record(current_entries) != receipt.get("nativeArtifactDigest"):
            errors.append("native artifact digest mismatch")
    if sources:
        external_mtimes = [int(item.get("sourceMaxMtimeNs", 0)) for item in external_sources]
        newest_source = max([path.stat().st_mtime_ns for path in sources] + external_mtimes)
        native_paths = [workspace / str(item["path"]) for item in native_entries if isinstance(item, dict)]
        engine_sources = [
            path for path in sources
            if path.relative_to(workspace).as_posix().startswith("packages/octocode-engine/")
        ]
        native_stale = bool(engine_sources) and any(
            path.is_file() and path.stat().st_mtime_ns < max(source.stat().st_mtime_ns for source in engine_sources)
            for path in native_paths
        )
        cli_stale = cli.is_file() and (
            cli.stat().st_mtime_ns < newest_source
            or any(path.is_file() and cli.stat().st_mtime_ns < path.stat().st_mtime_ns for path in native_paths)
        )
        if native_stale:
            errors.append("stale native artifact predates engine source")
        if cli_stale:
            errors.append("stale CLI artifact predates benchmark-relevant source or native artifact")
    return errors


def validate_octocode_argv(argv: list[str], workspace: Path) -> list[str]:
    errors: list[str] = []
    if len(argv) < 2:
        return ["Octocode invocation must be node plus the workspace CLI"]
    executable = Path(argv[0]).name.lower()
    if executable in {"npx", "octocode", "npm", "yarn", "pnpm", "bun", "bunx"}:
        errors.append("published/global/package-runner Octocode invocation is forbidden")
        return errors
    if "node" not in executable:
        errors.append("Octocode invocation must start with the Node executable")
    expected = (workspace.resolve() / "packages/octocode/out/octocode.js").resolve()
    try:
        actual = Path(argv[1]).resolve(strict=True)
    except OSError:
        actual = Path(argv[1]).resolve()
    if actual != expected:
        errors.append(f"Octocode script must resolve to workspace artifact {expected}")
    return errors


def _write_frozen(path: Path, value: dict[str, object]) -> None:
    payload = json.dumps(value, indent=2, sort_keys=True, ensure_ascii=False) + "\n"
    if path.exists():
        if path.read_text(encoding="utf-8") != payload:
            raise PreflightError(f"refusing to overwrite frozen receipt: {path}")
        return
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(payload, encoding="utf-8")


def _catalog(workspace: Path) -> bytes:
    cli = workspace / "packages/octocode/out/octocode.js"
    calls = (
        ["node", str(cli), "tools", "--json"],
        ["node", str(cli), "tools", "localSearch", "astSearch", "localFetch", "lspSearch", "--scheme", "--json", "--compact"],
    )
    output = bytearray()
    for call in calls:
        errors = validate_octocode_argv(call, workspace)
        if errors:
            raise PreflightError("; ".join(errors))
        result = subprocess.run(call, cwd=workspace, capture_output=True, check=False)
        if result.returncode != 0:
            raise PreflightError(f"workspace catalog command failed: {result.stderr.decode(errors='replace')}")
        output.extend(result.stdout)
    return bytes(output)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    prepare = sub.add_parser("prepare", help="build workspace, clone the two repositories, and freeze receipts")
    prepare.add_argument("--workspace", type=Path, required=True)
    prepare.add_argument("--corpus-root", type=Path, required=True)
    prepare.add_argument("--output", type=Path, required=True)
    prepare.add_argument("--fixture-manifest", type=Path, required=True)
    prepare.add_argument(
        "--public-oracles",
        type=Path,
        default=DEFAULT_PUBLIC_ORACLES,
        help="checked-in v4 public oracle receipt whose repository pins must match the frozen corpus",
    )
    existing = sub.add_parser("lock-existing", help="freeze already cloned exact repository paths without executing repository code")
    existing.add_argument("--langchain-path", type=Path, required=True)
    existing.add_argument("--nextjs-path", type=Path, required=True)
    existing.add_argument("--langchain-sha", required=True)
    existing.add_argument("--nextjs-sha", required=True)
    existing.add_argument("--fixture-manifest", type=Path, required=True)
    existing.add_argument("--output", type=Path, required=True)
    verify = sub.add_parser("verify", help="verify existing workspace and corpus receipts")
    verify.add_argument("--workspace", type=Path, required=True)
    verify.add_argument("--corpus-root", type=Path, required=True)
    verify.add_argument("--workspace-receipt", type=Path, required=True)
    verify.add_argument("--corpus-lock", type=Path, required=True)
    args = parser.parse_args()
    try:
        if args.command == "prepare":
            workspace = args.workspace.resolve()
            for command, cwd in _build_commands(workspace):
                subprocess.run(command, cwd=cwd, check=True)
            catalog = _catalog(workspace)
            workspace_receipt = build_workspace_receipt(workspace, catalog_bytes=catalog)
            expected_commits = frozen_oracle_commits(args.public_oracles)
            prepare_repositories(args.corpus_root, expected_commits=expected_commits)
            fixture_digest = _sha256(args.fixture_manifest.read_bytes())
            corpus_lock = build_corpus_lock(
                args.corpus_root,
                fixture_digest,
                expected_commits=expected_commits,
            )
            _write_frozen(args.output / "WORKSPACE.receipt.json", workspace_receipt)
            _write_frozen(args.output / "CORPUS.lock.json", corpus_lock)
            print(json.dumps({"workspaceReceiptDigest": workspace_receipt["receiptDigest"], "corpusLockDigest": digest_record(corpus_lock)}))
        elif args.command == "lock-existing":
            paths = {
                "langchain-ai/langchain": args.langchain_path,
                "vercel/next.js": args.nextjs_path,
            }
            expected = {
                "langchain-ai/langchain": args.langchain_sha,
                "vercel/next.js": args.nextjs_sha,
            }
            fixture_digest = _sha256(args.fixture_manifest.read_bytes())
            corpus_lock = build_corpus_lock(None, fixture_digest, repo_paths=paths, expected_commits=expected)
            _write_frozen(args.output, corpus_lock)
            print(json.dumps({"corpusLockDigest": digest_record(corpus_lock), "commits": {item["repo"]: item["commit"] for item in corpus_lock["repositories"]}}))
        else:
            workspace_receipt = json.loads(args.workspace_receipt.read_text(encoding="utf-8"))
            corpus_lock = json.loads(args.corpus_lock.read_text(encoding="utf-8"))
            errors = validate_workspace_receipt(workspace_receipt, args.workspace, catalog_bytes=_catalog(args.workspace))
            errors.extend(verify_corpus_bytes(corpus_lock, args.corpus_root))
            print(json.dumps({"valid": not errors, "errors": errors}, indent=2))
            return 1 if errors else 0
    except (OSError, subprocess.SubprocessError, PreflightError, json.JSONDecodeError) as exc:
        print(json.dumps({"valid": False, "errors": [str(exc)]}, indent=2), file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
