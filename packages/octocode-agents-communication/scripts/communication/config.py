"""Communication storage location, using the shared Octocode home policy."""
import os
from pathlib import Path
from octocode_config import get_octocode_home


def database_path(override=None):
    if override is not None:
        return Path(os.path.abspath(override))
    return get_octocode_home() / 'agents-communication' / 'communication.sqlite'
