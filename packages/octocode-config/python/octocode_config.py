"""Octocode home policy for standalone Python consumers; owned by @octocodeai/config.

Package builds copy this module into a skill's scripts directory. Like src/home.ts
and rust/home.rs, this resolves lexical paths without following symlinks.
"""
import os
from pathlib import Path


def get_octocode_home(env=None, cwd=None, home=None):
    env = os.environ if env is None else env
    override = env.get('OCTOCODE_HOME', '').strip()
    base = Path(cwd) if cwd is not None else Path.cwd()
    value = base / override if override else (Path(home) if home is not None else Path.home()) / '.octocode'
    return Path(os.path.abspath(os.path.normpath(value)))
