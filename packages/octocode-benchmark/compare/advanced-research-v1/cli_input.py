"""Canonical CLI flag parsing for the advanced-research benchmark observer.

The bridge is bundled once into the campaign artifact directory before its
fingerprint is sealed. Calls only execute that parser bundle; they cannot reach
the CLI command dispatcher or any Octocode tool implementation.
"""

from __future__ import annotations

import json
import shutil
import subprocess
from pathlib import Path
from typing import Any, Sequence


class CliInputError(ValueError):
    """The canonical CLI parser rejected the supplied flag tail."""

    def __init__(self, message: str, details: Any = None) -> None:
        super().__init__(message)
        self.details = details


_HERE = Path(__file__).resolve().parent
_REPOSITORY_ROOT = _HERE.parents[3]
_BRIDGE_SOURCE = _HERE / "cli_input_bridge.ts"
_ESBUILD = _REPOSITORY_ROOT / "node_modules" / "esbuild" / "bin" / "esbuild"


def _node() -> str:
    node = shutil.which("node")
    if node is None:
        raise RuntimeError("Node.js is required to prepare the CLI flag bridge.")
    return node


def prepare_bridge(output_dir: Path | str) -> Path:
    """Bundle the canonical parser beneath ``output_dir`` and return its path."""
    destination = Path(output_dir).resolve()
    destination.mkdir(parents=True, exist_ok=True)
    bridge = destination / "cli-input-bridge.cjs"
    if bridge.exists():
        raise RuntimeError(f"Refusing to overwrite existing CLI flag bridge: {bridge}")
    if not _ESBUILD.is_file():
        raise RuntimeError(f"esbuild is unavailable at {_ESBUILD}.")
    try:
        completed = subprocess.run(
            [
                _node(),
                str(_ESBUILD),
                str(_BRIDGE_SOURCE),
                "--bundle",
                "--platform=node",
                "--format=cjs",
                "--target=node24",
                f"--outfile={bridge}",
                "--log-level=warning",
            ],
            cwd=_REPOSITORY_ROOT,
            capture_output=True,
            text=True,
            check=False,
            timeout=60,
        )
    except subprocess.TimeoutExpired as error:
        raise TimeoutError("Timed out while bundling the CLI flag bridge.") from error
    if completed.returncode != 0 or not bridge.is_file():
        detail = (completed.stderr or completed.stdout).strip()
        raise RuntimeError(f"Could not bundle the CLI flag bridge: {detail}")
    return bridge


def parse_flag_query(
    bridge: Path | str, tool: str, tail: Sequence[str]
) -> dict[str, Any]:
    """Parse one CLI flag tail with the bundled canonical parser.

    This deliberately returns an unvalidated query. The caller owns tool/runtime
    flag restrictions and must apply its scope policy to the parsed query.
    Parsing does not provide a pre-execution veto for the observed command.
    """
    bridge_path = Path(bridge)
    if not bridge_path.is_file():
        raise RuntimeError(f"CLI flag bridge is unavailable: {bridge_path}")
    try:
        completed = subprocess.run(
            [_node(), str(bridge_path), tool, *tail],
            cwd=_REPOSITORY_ROOT,
            capture_output=True,
            text=True,
            check=False,
            timeout=10,
        )
    except subprocess.TimeoutExpired as error:
        raise TimeoutError("Timed out while parsing CLI field flags.") from error
    try:
        result = json.loads(completed.stdout)
    except json.JSONDecodeError as error:
        raise RuntimeError(
            f"CLI flag bridge returned invalid JSON: {completed.stderr.strip()}"
        ) from error
    if not isinstance(result, dict) or result.get("ok") is not True:
        error = result.get("error") if isinstance(result, dict) else None
        message = error.get("message") if isinstance(error, dict) else None
        if completed.returncode == 2 and message:
            raise CliInputError(str(message), error.get("details"))
        raise RuntimeError(
            f"CLI flag bridge failed unexpectedly: {message or completed.stderr.strip()}"
        )
    if completed.returncode != 0:
        raise RuntimeError(
            f"CLI flag bridge exited {completed.returncode}: {completed.stderr.strip()}"
        )
    query = result.get("query")
    if not isinstance(query, dict):
        raise RuntimeError("CLI flag bridge returned no query object.")
    return query
