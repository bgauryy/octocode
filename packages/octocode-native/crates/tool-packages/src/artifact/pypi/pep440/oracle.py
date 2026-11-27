"""Regenerates oracle.jsonl, the PEP 440 reference table pep440/tests.rs reads.

Run with `packaging` installed: python3 -I oracle.py > oracle.jsonl
Tests never run Python; they read the committed oracle.jsonl, one JSON
array per line: [kind, inputs..., expected].
"""

import itertools
import json

import packaging
from packaging.specifiers import InvalidSpecifier, SpecifierSet
from packaging.version import InvalidVersion, Version

# Spellings PEP 440 normalises, each beside its normal form.
SPELLINGS = [
    ("1.0a1", "1.0alpha1"), ("1.0a1", "1.0ALPHA1"), ("1.0a1", "1.0-a1"), ("1.0a1", "1.0_a1"),
    ("1.0a1", "1.0.a1"), ("1.0a1", "1.0a.1"), ("1.0a1", "1.0-a-1"), ("1.0a0", "1.0a"),
    ("1.0b2", "1.0beta2"), ("1.0rc1", "1.0c1"), ("1.0rc1", "1.0pre1"), ("1.0rc1", "1.0preview1"),
    ("1.0rc1", "1.0RC1"), ("1.0.post1", "1.0post1"), ("1.0.post1", "1.0-post1"),
    ("1.0.post1", "1.0_post1"), ("1.0.post1", "1.0.post.1"), ("1.0.post1", "1.0-1"),
    ("1.0.post1", "1.0.rev1"), ("1.0.post1", "1.0r1"), ("1.0.post0", "1.0.post"),
    ("1.0.post0", "1.0-r"), ("1.0.dev0", "1.0dev"), ("1.0.dev0", "1.0-dev0"),
    ("1.0.dev0", "1.0_dev0"), ("1.0.dev1", "1.0.DEV-1"), ("1.0", "v1.0"), ("1.0", "V1.0"),
    ("1.0", " 1.0\n"), ("1.0", "1.0.0"), ("1.0", "1.0.0.0"), ("1.2", "01.02"),
    ("1.0", "0!1.0"), ("1.0+ubuntu.1", "1.0+ubuntu-1"), ("1.0+ubuntu.1", "1.0+ubuntu_1"),
    ("1.0+ubuntu.1", "1.0+UBUNTU.1"), ("1.0+1", "1.0+01"),
    ("2.9.0.post0", "2.9.0-post0"), ("1.0a1.post2.dev3", "1.0-alpha1-2-dev3"),
]

# PEP 440's ordering example plus epochs and local labels, ascending.
ORDER = [
    "1.dev0", "1.0.dev456", "1.0a1", "1.0a2.dev456", "1.0a12.dev456", "1.0a12",
    "1.0b1.dev456", "1.0b2", "1.0b2.post345.dev456", "1.0b2.post345", "1.0rc1.dev456",
    "1.0rc1", "1.0", "1.0+abc", "1.0+abc.5", "1.0+abc.7", "1.0+5", "1.0+5.abc",
    "1.0.post456.dev34", "1.0.post456", "1.0.15", "1.1.dev1", "2.9.0", "2.9.0.post0",
    "3", "1!0.1", "1!1.0a1", "2!0.0.1",
]

# Malformed spellings, and edge spellings whose validity is checked.
VERSIONS = [
    "", "1.0-", "1.0.", "a1.0", "1.0+", "1.0+abc..1", "1.0+_abc", "1.0.post1a1",
    "1.0dev1a1", "1..0", "1.0 1", "1!", "-1.0", "1.0a1a2", "1.0+abc+def", "!1.0",
    "1.0.*", "1.0-dev-1-2", "1.0a.", "1.0.post.", "1.0-dev.", "1.0a-", "1.0_", "v", "1.0+a-",
    "1.0-r-1", "1.0.r", "1.0rc", "1.0+a.b-c_d", "1.0.post1.dev2+x.3",
]

MATCHES = {
    "~=2.2": ["2.2", "2.3", "3.0", "2.1.9"],
    "~=1.4.5": ["1.4.5", "1.4.6", "1.5.0", "1.4.4"],
    "~=2.2.post3": ["2.2.post3", "2.3", "3.0", "2.2", "2.2.post4"],
    "~=1.4.5a4": ["1.4.5a4", "1.4.5", "1.4.9", "1.5", "1.4.5a3"],
    "~=2.2.0": ["2.2.9", "2.3", "2.2"],
    "~=1!1.0": ["1!1.5", "1.5", "1!2.0"],
    "==1.1.*": ["1.1", "1.1.0", "1.1.post1", "1.1a1", "1.1.dev1", "1.10", "1.1+local", "1", "1!1.1"],
    "==1.1": ["1.1.0", "1.1+local", "1.1.post1", "1.1a1"],
    "==1.1+local": ["1.1+local", "1.1", "1.1+other", "1.1.0+LOCAL"],
    "!=1.1": ["1.1+local", "1.2", "1.1"],
    "!=1.1.*": ["1.1.5", "1.2", "1.1a1"],
    "<1.0": ["1.0a1", "0.9", "1.0.dev0", "0.9a1", "0.9+local", "1.0"],
    "<1.0rc1": ["1.0a1", "1.0rc1.dev1", "1.0rc1", "0.9"],
    "<1.0.post1": ["1.0", "1.0a1", "1.0.post1.dev1"],
    ">1.7": ["1.7.post2", "1.7.1", "1.8.dev0", "1.7+local", "1.7.0.post1", "1.7"],
    ">1.7.post2": ["1.7.post3", "1.7.1", "1.7.post2+local", "1.7.post2"],
    ">=1.0": ["1.0+local", "1.0a1", "1.0.post1", "1!0.5", "0.9"],
    "<=1.0": ["1.0+local", "1.0.post1", "1.0a1", "1.0"],
    "<2": ["1!0.5", "1.9", "2.0a1"],
    "===1.0": ["1.0", "1.0.0", "1.0+local"],
    ">1.0.dev1": ["1.0.dev2", "1.0", "1.0.dev1"],
    ">=1.0,<2.0,!=1.5.*": ["1.5.1", "1.6", "2.0"],
    ">=2.9.0.post0,<3": ["2.9.0.post0", "2.9.0", "3.0.0"],
    ">1.0a1": ["1.0a1.post1", "1.0a2.post1", "1.0a1+local", "1.0"],
    ">1.7.post2,<2": ["1.7.post3+local", "1.7.post3.dev0", "1.7.post2.dev9"],
    ">1.0.dev1": ["1.0.dev1+local", "1.0a1"],
    ">1.0.post1.dev1": ["1.0.post1.dev2", "1.0.post1", "1.0.post1.dev1+x"],
    "~=2.2,<9": ["3.0a1", "2.9+local", "3.dev0", "2.2a1"],
    "==1.1.*,<9": ["1.2.dev0", "1.1.dev0", "1.0.post9"],
    "==1!1.*": ["1!1.5", "1.5", "1!2.0"],
    "<1.0.post1,>0": ["1.0.post1.dev0", "1.0.post0", "1.0+local"],
    "!=1.0+local": ["1.0+local", "1.0", "1.0+other"],
    "<=1.0a1": ["1.0a1+x", "1.0a1.post0", "1.0a0"],
    "===1.0RC1": ["1.0rc1", "1.0c1"],
}

RESOLVE = [
    (">=1.0", "1.0 1.1a1"), (">=1.0", "0.9 1.1a1"), (">=1.1a1", "1.0 1.1a1 1.1b1"),
    ("!=1.1a1", "1.0 1.2a1"), ("==1.*", "1.0 1.1 2.0a1"), ("<1.0", "1.0a1 0.9"),
    ("<1.0", "1.0a1"), (">1.0", "1.0.post1"), (">1.0", "1.0.post1 1.0.1.dev1"),
    ("==1.0.dev1", "1.0.dev1 1.0"), (">=1.0", "1.0 1.0.post1.dev1"),
    (">=5.0rc1,<5.1", "4.2 5.0rc1 5.0 5.0.1 5.1a1"), (">=5.0rc1,<5.1", "4.2 5.0rc1 5.0rc2"),
    ("~=2.31", "2.30.0 2.31.0 2.32.3 3.0.0rc1"), ("~=2.31.0", "2.31.0 2.31.1 2.32.0"),
    (">=2.8,<3", "2.8.2 2.9.0 2.9.0.post0 3.0.0"),
    (">=2.9.0.post0,<3", "2.9.0 2.9.0.post0 3.0.0"),
    (">=1.0,,<2", "1.5 2.0"), (" >= 1.0 , < 2 ", "1.5 2.0"), ("== 1.0", "1.0 1.0.0+x"),
    ("===1.0rc1", "1.0rc1 1.0"), ("<=2.0", "2.0+local 1.9"),
    ("!=2.0.*,>=1", "1.9 2.0.1 2.1rc1"), (">=3", "1.0 2.0"),
]

INVALID_SPECIFIERS = [
    ">=", "=>1", "<<1", "~=1", "~=1.*", ">=1.*", "==1.0a1.*", ">1.0+local", "<=1.0+l",
    "~=1.0+l", "==1.0+local.*", "1.0", ">=1.0 <2", "==1.0.*.*", ">=1.0,foo", "=1.0",
    "==1.0.post1.*", "~=1.0.dev1.*", ">=1.0;",
]


def sign(left, right):
    return (left > right) - (left < right)


def row(*fields):
    print(json.dumps(fields))


def main():
    row("packaging", packaging.__version__)
    for normal, spelling in SPELLINGS:
        row("order", spelling, normal, sign(Version(spelling), Version(normal)))
    for left, right in itertools.combinations(ORDER, 2):
        row("order", left, right, sign(Version(left), Version(right)))
    for value in VERSIONS:
        try:
            Version(value)
            row("version", value, True)
        except InvalidVersion:
            row("version", value, False)
    for spec, versions in MATCHES.items():
        for version in versions:
            row("match", spec, version, SpecifierSet(spec).contains(version, prereleases=True))
    for spec, versions in RESOLVE:
        matched = list(SpecifierSet(spec).filter(versions.split()))
        row("resolve", spec, versions.split(), max(matched, key=Version) if matched else None)
    for spec in INVALID_SPECIFIERS:
        try:
            SpecifierSet(spec)
            row("specifier", spec, True)
        except InvalidSpecifier:
            row("specifier", spec, False)


main()
