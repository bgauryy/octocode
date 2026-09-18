"""Expose the private curator tests through the benchmark package's test command."""

from __future__ import annotations

import sys
from pathlib import Path


PRIVATE = Path(__file__).resolve().parent.parent / "terra-v3" / "private"
sys.path.insert(0, str(PRIVATE))

from test_curator_bundle import CuratorBundleTests  # noqa: E402,F401

