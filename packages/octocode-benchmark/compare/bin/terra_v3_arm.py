#!/usr/bin/env python3
"""Validate a native benchmark arm, freeze its executable receipt, then instrument it."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import sys

from terra_v3_contracts import load_and_validate_contracts, validate_arm_argv, validate_sourcegraph_receipt
from terra_v3_lsp_receipt import validate_language_server_receipt, validate_lsp_replay
from terra_v3_preflight import _catalog, _sha256, digest_record, validate_workspace_receipt, verify_corpus_bytes


LSP_ARMS = {"lsp-pyright", "lsp-typescript"}


def verify_locked_corpus(corpus_lock: dict[str, object]) -> list[str]:
    paths = {
        str(item.get("repo")): Path(str(item.get("sourcePath")))
        for item in corpus_lock.get("repositories", []) if isinstance(item, dict) and item.get("sourcePath")
    }
    return verify_corpus_bytes(corpus_lock, None, repo_paths=paths)


def _directory_digest(root: Path) -> str:
    digest = hashlib.sha256()
    for path in sorted((item for item in root.rglob("*") if item.is_file()), key=lambda item: item.relative_to(root).as_posix()):
        relative = path.relative_to(root).as_posix().encode()
        content = path.read_bytes()
        digest.update(len(relative).to_bytes(8, "big")); digest.update(relative)
        digest.update(len(content).to_bytes(8, "big")); digest.update(content)
    return digest.hexdigest()


def build_measured_argv(
    arm: str, argv: list[str], workspace: Path, *, lsp_root: Path | None,
    lsp_method: str | None, lsp_params_json: str | None,
) -> list[str]:
    if arm not in LSP_ARMS:
        return argv
    if not lsp_root or not lsp_method or lsp_params_json is None:
        raise RuntimeError("direct-LSP arms require --lsp-root, --lsp-method, and --lsp-params-json")
    json.loads(lsp_params_json)
    client = Path(__file__).resolve().parent / "terra_v3_lsp_client.py"
    return [
        sys.executable, str(client), "--root", str(lsp_root.resolve()),
        "--method", lsp_method, "--params-json", lsp_params_json,
        "--server", *argv,
    ]


def _resolve_executable(argv: list[str]) -> Path:
    candidate = Path(argv[0])
    resolved = candidate.resolve() if candidate.is_absolute() else Path(shutil.which(argv[0]) or "")
    if not resolved.is_file():
        raise RuntimeError(f"executable not found: {argv[0]}")
    return resolved.resolve()


def _receipt(arm: str, argv: list[str], version_command: list[str]) -> dict[str, object]:
    executable = _resolve_executable(argv)
    version = subprocess.run(version_command, text=True, capture_output=True, check=False)
    if version.returncode != 0:
        raise RuntimeError(f"version command failed for {arm}: {version.stderr.strip()}")
    receipt = {
        "arm": arm, "argv0": str(executable),
        "executableDigest": hashlib.sha256(executable.read_bytes()).hexdigest(),
        "versionCommand": version_command,
        "versionOutput": (version.stdout + version.stderr).strip(),
    }
    receipt["receiptDigest"] = digest_record(receipt)
    return receipt


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--arm", required=True)
    parser.add_argument("--workspace", type=Path, required=True)
    parser.add_argument("--workspace-receipt", type=Path, required=True)
    parser.add_argument("--corpus-lock", type=Path, required=True)
    parser.add_argument("--sourcegraph-receipt", type=Path)
    parser.add_argument("--language-server-receipt", type=Path)
    parser.add_argument("--contracts", type=Path, required=True)
    parser.add_argument("--fixture-manifest", type=Path, required=True)
    parser.add_argument("--log", type=Path, required=True)
    parser.add_argument("--artifact-dir", type=Path, required=True)
    parser.add_argument("--label", required=True)
    parser.add_argument("--cache-cohort", choices=("cold", "warm-process", "warm-index"), required=True)
    parser.add_argument("--logical-call-id", required=True)
    parser.add_argument("--attempt-index", type=int, required=True)
    parser.add_argument("--empty-classification", choices=("expected-absence", "scope-empty", "provider-incomplete", "query-miss"))
    parser.add_argument("--cgroup-parent", type=Path)
    parser.add_argument("--lsp-root", type=Path)
    parser.add_argument("--lsp-method")
    parser.add_argument("--lsp-params-json")
    parser.add_argument("--dry-run", action="store_true")
    parser.add_argument("command", nargs=argparse.REMAINDER)
    args = parser.parse_args()
    argv = args.command[1:] if args.command[:1] == ["--"] else args.command
    try:
        bundle, errors = load_and_validate_contracts(args.contracts)
        language_server_receipt = None
        if args.arm in LSP_ARMS:
            if not args.language_server_receipt:
                errors.append("direct-LSP arm requires --language-server-receipt")
            elif not args.lsp_root:
                errors.append("direct-LSP arm requires --lsp-root")
            else:
                language_server_receipt = json.loads(args.language_server_receipt.read_text(encoding="utf-8"))
                errors.extend(validate_language_server_receipt(language_server_receipt, args.arm, args.lsp_root))
                errors.extend(validate_lsp_replay(language_server_receipt, argv, args.lsp_root))
        else:
            errors.extend(validate_arm_argv(args.arm, argv, args.workspace))
        workspace_receipt = json.loads(args.workspace_receipt.read_text(encoding="utf-8"))
        catalog_bytes = _catalog(args.workspace) if args.arm == "octocode" else None
        errors.extend(validate_workspace_receipt(
            workspace_receipt, args.workspace, catalog_bytes=catalog_bytes,
            require_canonical_core=True,
        ))
        corpus_lock = json.loads(args.corpus_lock.read_text(encoding="utf-8"))
        errors.extend(verify_locked_corpus(corpus_lock))
        fixture_digest = _sha256(args.fixture_manifest.read_bytes())
        if fixture_digest != corpus_lock.get("fixtureManifestDigest"):
            errors.append("fixture manifest digest differs from corpus lock")
        contracts_digest = _directory_digest(args.contracts)
        measured_argv = build_measured_argv(
            args.arm, argv, args.workspace, lsp_root=args.lsp_root,
            lsp_method=args.lsp_method, lsp_params_json=args.lsp_params_json,
        )
        if sys.platform.startswith("linux") and not args.cgroup_parent:
            errors.append("strict Linux measurement requires a delegated --cgroup-parent")
        if args.arm == "sourcegraph":
            if not args.sourcegraph_receipt:
                errors.append("Sourcegraph arm requires --sourcegraph-receipt")
            else:
                sourcegraph_receipt = json.loads(args.sourcegraph_receipt.read_text(encoding="utf-8"))
                errors.extend(validate_sourcegraph_receipt(sourcegraph_receipt, corpus_lock))
        if errors:
            raise RuntimeError("; ".join(errors))
        arm_contract = bundle["arms"][args.arm]
        if args.arm in LSP_ARMS:
            tool_receipt = language_server_receipt
        else:
            version_command = [value.replace("{workspace}", str(args.workspace.resolve())) for value in arm_contract["versionCommand"]]
            version_command[0] = argv[0]
            tool_receipt = _receipt(args.arm, argv, version_command)
        result = {
            "arm": args.arm, "argv": measured_argv, "toolReceipt": tool_receipt,
            "workspaceReceiptDigest": workspace_receipt["receiptDigest"],
            "corpusLockDigest": digest_record(corpus_lock),
            "fixtureManifestDigest": fixture_digest, "contractsDigest": contracts_digest,
        }
        if args.dry_run:
            print(json.dumps(result, indent=2, sort_keys=True)); return 0
        instrument = Path(__file__).resolve().parent / "instrument_command.py"
        command = [
            sys.executable, str(instrument), "--log", str(args.log), "--artifact-dir", str(args.artifact_dir),
            "--label", args.label, "--cache-cohort", args.cache_cohort,
            "--corpus-digest", result["corpusLockDigest"],
            "--workspace-receipt-digest", result["workspaceReceiptDigest"],
            "--fixture-manifest-digest", fixture_digest,
            "--contracts-digest", contracts_digest,
            "--tool-receipt-digest", tool_receipt["receiptDigest"],
            "--logical-call-id", args.logical_call_id, "--attempt-index", str(args.attempt_index),
        ]
        if args.empty_classification:
            command.extend(["--empty-classification", args.empty_classification])
        if args.cgroup_parent:
            command.extend(["--cgroup-parent", str(args.cgroup_parent)])
        command.extend(["--strict-v3", "--", *measured_argv])
        return subprocess.run(command, check=False).returncode
    except (OSError, KeyError, ValueError, RuntimeError, json.JSONDecodeError) as exc:
        print(json.dumps({"valid": False, "errors": [str(exc)]}), file=sys.stderr); return 1


if __name__ == "__main__":
    raise SystemExit(main())
