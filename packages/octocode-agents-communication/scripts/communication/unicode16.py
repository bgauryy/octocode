"""Frozen Unicode 16 canonical caseless keys, independent of the Python version.

Data: UnicodeData.txt and CaseFolding.txt from unicode.org/Public/16.0.0/ucd/.
Source SHA-256 digests are in unicode16.json; terms are in UNICODE-LICENSE.txt.
Changing these tables changes persistent lease identity and needs a coordinated upgrade.
"""
import json
from functools import lru_cache
from pathlib import Path


@lru_cache(maxsize=1)
def _tables():
    data = json.loads(Path(__file__).with_suffix('.json').read_text(encoding='utf-8'))
    return tuple({int(k): v for k, v in data[name].items()}
                 for name in ('decomposition', 'combining', 'casefold'))


def _nfd(points, decomposition, combining):
    expanded = []

    def expand(cp):
        # Unicode's algorithmic Hangul decomposition is absent from UnicodeData.
        index = cp - 0xAC00
        if 0 <= index < 11172:
            expanded.extend((0x1100 + index // 588, 0x1161 + (index % 588) // 28))
            if index % 28:
                expanded.append(0x11A7 + index % 28)
        elif cp in decomposition:
            for child in decomposition[cp]:
                expand(child)
        else:
            expanded.append(cp)

    for cp in points:
        expand(cp)
    result, marks = [], []
    for cp in expanded:
        if not combining.get(cp, 0):
            result.extend(sorted(marks, key=lambda c: combining[c]))
            marks.clear()
            result.append(cp)
        else:
            marks.append(cp)
    result.extend(sorted(marks, key=lambda c: combining[c]))
    return result


def normalize_path_component(value):
    if value.isascii():
        return value.lower()
    decomposition, combining, folding = _tables()
    decomposed = _nfd(map(ord, value), decomposition, combining)
    folded = (part for cp in decomposed for part in folding.get(cp, (cp,)))
    return ''.join(map(chr, _nfd(folded, decomposition, combining)))
