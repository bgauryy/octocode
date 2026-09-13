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


_HERE = Path(__file__).resolve().parent
_REPOSITORY_ROOT = _HERE.parents[3]
_BRIDGE_SOURCE = _HERE / "cli_input_bridge.ts"
_ESBUILD = _REPOSITORY_ROOT / "node_modules" / "esbuild" / "bin" / "esbuild"


def _node() -> str:
    node = shutil.which("node")
    if node is None:
        raise CliInputError("Node.js is required to prepare the CLI flag bridge.")
    return node


def prepare_bridge(output_dir: Path | str) -> Path:
    """Bundle the canonical parser beneath ``output_dir`` and return its path."""
    destination = Path(output_dir).resolve()
    destination.mkdir(parents=True, exist_ok=True)
    bridge = destination / "cli-input-bridge.cjs"
    if not _ESBUILD.is_file():
        raise CliInputError(f"esbuild is unavailable at {_ESBUILD}.")
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
    )
    if completed.returncode != 0 or not bridge.is_file():
        detail = (completed.stderr or completed.stdout).strip()
        raise CliInputError(f"Could not bundle the CLI flag bridge: {detail}")
    return bridge


def parse_flag_query(
    bridge: Path | str, tool: str, tail: Sequence[str]
) -> dict[str, Any]:
    """Parse one CLI flag tail with the bundled canonical parser.

    This deliberately returns an unvalidated query. The caller owns tool/runtime
    flag restrictions and must apply its scope policy before tool execution.
    """
    completed = subprocess.run(
        [_node(), str(Path(bridge)), tool, *tail],
        cwd=_REPOSITORY_ROOT,
        capture_output=True,
        text=True,
        check=False,
    )
    try:
        result = json.loads(completed.stdout)
    except json.JSONDecodeError as error:
        raise CliInputError(
            f"CLI flag bridge returned invalid JSON: {completed.stderr.strip()}"
        ) from error
    if not isinstance(result, dict) or result.get("ok") is not True:
        error = result.get("error") if isinstance(result, dict) else None
        message = error.get("message") if isinstance(error, dict) else None
        raise CliInputError(str(message or "CLI flag parser rejected the input."))
    query = result.get("query")
    if not isinstance(query, dict):
        raise CliInputError("CLI flag bridge returned no query object.")
    return query
