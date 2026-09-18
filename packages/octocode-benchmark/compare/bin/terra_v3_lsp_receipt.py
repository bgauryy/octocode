#!/usr/bin/env python3
"""Resolve, probe, fingerprint, and validate direct language-server commands."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import sys
from typing import Any

from terra_v3_lsp_client import run as run_lsp
from terra_v3_preflight import PreflightError, digest_record


LSP_ARMS = {"lsp-pyright", "lsp-typescript"}


def _sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def _resolve_executable(value: str) -> Path:
    candidate = Path(value)
    resolved = candidate.resolve() if candidate.is_absolute() else Path(shutil.which(value) or "").resolve()
    if not resolved.is_file():
        raise PreflightError(f"language-server executable not found: {value}")
    return resolved


def _package_root(executable: Path) -> Path:
    for candidate in (executable.parent, *executable.parents):
        manifest = candidate / "package.json"
        if manifest.is_file():
            return candidate.resolve()
    raise PreflightError(f"cannot locate language-server package.json above {executable}")


def _package_digest(root: Path) -> str:
    entries: list[dict[str, str]] = []
    for path in sorted(root.rglob("*"), key=lambda item: item.relative_to(root).as_posix()):
        relative = path.relative_to(root)
        if not path.is_file() or path.is_symlink() or any(part in {"node_modules", ".git", "__pycache__"} for part in relative.parts):
            continue
        entries.append({"path": relative.as_posix(), "sha256": _sha256(path)})
    return digest_record(entries)


def _config_state(config_paths: list[Path], initialization_options: object) -> tuple[list[dict[str, str]], str]:
    entries: list[dict[str, str]] = []
    for path in sorted((item.resolve() for item in config_paths), key=str):
        if not path.is_file():
            raise PreflightError(f"language-server config is missing: {path}")
        entries.append({"path": str(path), "sha256": _sha256(path)})
    return entries, digest_record({"files": entries, "initializationOptions": initialization_options})


def build_language_server_receipt(
    arm: str,
    server_argv: list[str],
    workspace_root: Path,
    *,
    config_paths: list[Path],
    initialization_options: object,
    probe: dict[str, Any],
) -> dict[str, object]:
    if arm not in LSP_ARMS:
        raise PreflightError(f"not a direct-LSP arm: {arm}")
    if not server_argv:
        raise PreflightError("language-server argv is empty")
    executable = _resolve_executable(server_argv[0])
    package_root = _package_root(executable)
    manifest = json.loads((package_root / "package.json").read_text(encoding="utf-8"))
    configs, config_fingerprint = _config_state(config_paths, initialization_options)
    initialize = probe.get("initialize", {})
    response = probe.get("response", {})
    shutdown = probe.get("shutdown", {})
    capabilities = initialize.get("result", {}).get("capabilities", {}) if isinstance(initialize, dict) else {}
    readiness = {
        "initializeSucceeded": isinstance(initialize, dict) and "error" not in initialize and isinstance(capabilities, dict),
        "probeSucceeded": isinstance(response, dict) and "error" not in response,
        "shutdownSucceeded": isinstance(shutdown, dict) and "error" not in shutdown,
    }
    receipt: dict[str, object] = {
        "version": 1,
        "arm": arm,
        "requestedCommand": list(server_argv),
        "resolvedCommand": [str(executable), *server_argv[1:]],
        "executablePath": str(executable),
        "executableDigest": _sha256(executable),
        "packageRoot": str(package_root),
        "packageName": manifest.get("name"),
        "packageVersion": manifest.get("version"),
        "packageManifestDigest": _sha256(package_root / "package.json"),
        "packageDigest": _package_digest(package_root),
        "workspaceRoot": str(workspace_root.resolve()),
        "configFiles": configs,
        "initializationOptions": initialization_options,
        "configFingerprint": config_fingerprint,
        "capabilities": capabilities,
        "capabilitiesDigest": digest_record(capabilities),
        "readiness": readiness,
        "probeReceipt": {
            "initializeDigest": digest_record(initialize),
            "probeDigest": digest_record(response),
            "shutdownDigest": digest_record(shutdown),
        },
    }
    receipt["receiptDigest"] = digest_record(receipt)
    return receipt


def validate_language_server_receipt(
    receipt: dict[str, object], arm: str, workspace_root: Path
) -> list[str]:
    errors: list[str] = []
    if receipt.get("receiptDigest") != digest_record({key: value for key, value in receipt.items() if key != "receiptDigest"}):
        errors.append("language-server receipt digest mismatch")
    if receipt.get("arm") != arm:
        errors.append("language-server arm mismatch")
    if Path(str(receipt.get("workspaceRoot", ""))).resolve() != workspace_root.resolve():
        errors.append("language-server workspace root mismatch")
    command = receipt.get("resolvedCommand")
    if not isinstance(command, list) or not command or not all(isinstance(value, str) for value in command):
        return errors + ["language-server resolved command is missing"]
    executable = Path(command[0]).resolve()
    if not executable.is_file() or _sha256(executable) != receipt.get("executableDigest"):
        errors.append("language-server executable digest mismatch")
    package_root = Path(str(receipt.get("packageRoot", ""))).resolve()
    manifest = package_root / "package.json"
    if not manifest.is_file() or _sha256(manifest) != receipt.get("packageManifestDigest"):
        errors.append("language-server package manifest digest mismatch")
    if not package_root.is_dir() or _package_digest(package_root) != receipt.get("packageDigest"):
        errors.append("language-server package digest mismatch")
    config_paths = [Path(str(item.get("path", ""))) for item in receipt.get("configFiles", []) if isinstance(item, dict)]
    try:
        configs, fingerprint = _config_state(config_paths, receipt.get("initializationOptions"))
        if configs != receipt.get("configFiles") or fingerprint != receipt.get("configFingerprint"):
            errors.append("language-server config fingerprint mismatch")
    except PreflightError as exc:
        errors.append(str(exc))
    capabilities = receipt.get("capabilities")
    if not isinstance(capabilities, dict) or not capabilities or digest_record(capabilities) != receipt.get("capabilitiesDigest"):
        errors.append("language-server capabilities fingerprint mismatch")
    readiness = receipt.get("readiness")
    if not isinstance(readiness, dict) or any(readiness.get(key) is not True for key in ("initializeSucceeded", "probeSucceeded", "shutdownSucceeded")):
        errors.append("language-server readiness probe did not complete successfully")
    return errors


def validate_lsp_replay(receipt: dict[str, object], argv: list[str], workspace_root: Path) -> list[str]:
    errors = validate_language_server_receipt(receipt, str(receipt.get("arm")), workspace_root)
    if argv != receipt.get("resolvedCommand"):
        errors.append("direct-LSP command must exactly replay the resolved receipt argv")
    return errors


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--arm", choices=sorted(LSP_ARMS), required=True)
    parser.add_argument("--workspace-root", type=Path, required=True)
    parser.add_argument("--config", type=Path, action="append", default=[])
    parser.add_argument("--initialization-options-json", default="{}")
    parser.add_argument("--probe-method", default="workspace/symbol")
    parser.add_argument("--probe-params-json", default='{"query":""}')
    parser.add_argument("--timeout-seconds", type=float, default=30.0)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--server", nargs=argparse.REMAINDER, required=True)
    args = parser.parse_args()
    server = args.server[1:] if args.server[:1] == ["--"] else args.server
    try:
        resolved = [str(_resolve_executable(server[0])), *server[1:]]
        probe = run_lsp(
            resolved, args.workspace_root, args.probe_method,
            json.loads(args.probe_params_json), args.timeout_seconds,
        )
        receipt = build_language_server_receipt(
            args.arm, server, args.workspace_root, config_paths=args.config,
            initialization_options=json.loads(args.initialization_options_json), probe=probe,
        )
        errors = validate_language_server_receipt(receipt, args.arm, args.workspace_root)
        if errors:
            raise PreflightError("; ".join(errors))
        payload = json.dumps(receipt, indent=2, sort_keys=True) + "\n"
        if args.output.exists() and args.output.read_text(encoding="utf-8") != payload:
            raise PreflightError(f"refusing to overwrite different language-server receipt: {args.output}")
        args.output.parent.mkdir(parents=True, exist_ok=True)
        if not args.output.exists():
            args.output.write_text(payload, encoding="utf-8")
        print(json.dumps({"receiptDigest": receipt["receiptDigest"], "resolvedCommand": receipt["resolvedCommand"]}))
        return 0
    except (OSError, IndexError, ValueError, json.JSONDecodeError, PreflightError) as exc:
        print(json.dumps({"valid": False, "errors": [str(exc)]}), file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
