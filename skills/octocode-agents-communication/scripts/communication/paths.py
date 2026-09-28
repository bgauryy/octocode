"""Filesystem resolution and the protocol's frozen caseless lease namespace."""
import os
from pathlib import Path
from .unicode16 import normalize_path_component


def resolve_path(base, path):
    links = [0]
    def resolve(value):
        resolved = Path(value.anchor)
        for part in value.parts:
            if part == value.anchor or part == '.':
                continue
            if part == '..':
                resolved = resolved.parent
                continue
            resolved = resolved / part
            try:
                resolved.lstat()
            except FileNotFoundError:
                continue
            if resolved.is_symlink():
                links[0] += 1
                if links[0] > 40:
                    raise ValueError('Too many symbolic links')
                resolved = resolve(resolved.parent / os.readlink(resolved))
            else:
                resolved = resolved.resolve(strict=True)
        return resolved
    return resolve(Path(base) / path)


def lease_key(path):
    p = Path(path)
    return ''.join('/' + normalize_path_component(c) for c in p.parts if c not in (p.anchor, '.'))


def ancestors(key):
    return [key[:i] for i, c in enumerate(key) if c == '/']


def keys_overlap(a, ak, b, bk):
    return a == b or (ak == 'tree' and b.startswith(a + '/')) or (bk == 'tree' and a.startswith(b + '/'))


def overlap(a, ak, b, bk):
    return keys_overlap(lease_key(a), ak, lease_key(b), bk)
