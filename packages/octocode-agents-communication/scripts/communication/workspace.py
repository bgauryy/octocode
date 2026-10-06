"""Repository coordination identity without a Git executable or environment overrides."""
import os
import stat
from pathlib import Path


def metadata_path(path, prefix=''):
    descriptor = os.open(path, os.O_RDONLY | getattr(os, 'O_NONBLOCK', 0))
    with os.fdopen(descriptor, 'rb') as stream:
        if not stat.S_ISREG(os.fstat(stream.fileno()).st_mode):
            raise ValueError('Git metadata must be a regular file')
        raw = stream.read(4097)
    if len(raw) > 4096:
        raise ValueError('Git workspace metadata exceeds 4096 bytes')
    value = raw.decode('utf-8').rstrip('\r\n')
    if prefix and not value.startswith(prefix):
        raise ValueError('Invalid Git workspace metadata: ' + str(path))
    value = value[len(prefix):].lstrip() if prefix else value
    if not value or '\n' in value or '\r' in value:
        raise ValueError('Invalid Git workspace metadata: ' + str(path))
    return (path.parent / value).resolve(strict=True)


def coordination_scope(workspace):
    workspace = Path(workspace).resolve()
    for root in (workspace, *workspace.parents):
        marker = root / '.git'
        if not os.path.lexists(marker):
            continue
        gitdir = marker.resolve(strict=True) if marker.is_dir() else metadata_path(marker, 'gitdir:')
        common = metadata_path(gitdir / 'commondir') if (gitdir / 'commondir').exists() else gitdir
        if not (gitdir / 'HEAD').is_file() or not (common / 'objects').is_dir():
            raise ValueError('Invalid Git repository metadata: ' + str(marker))
        return str(common)
    return str(workspace)


def stored_scope(db, workspace):
    row = db.execute('SELECT coordinationScope FROM workspaces WHERE workspace=?', (workspace,)).fetchone()
    return row[0] if row else workspace


def scope_workspaces(db, scope, workspace):
    return [workspace] + [row[0] for row in db.execute('SELECT workspace FROM workspaces WHERE coordinationScope=? AND workspace<>? ORDER BY workspace', (scope, workspace))]
