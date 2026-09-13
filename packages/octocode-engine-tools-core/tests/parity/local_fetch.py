"""Independent localFetch parity sensor; never treats a missing tool as a pass.

Run against frozen Node and native CLI argv, with the same synthetic files.
This is a scoped T01 suite, not an all-tool or performance acceptance report.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys


def fixtures(root):
    root.mkdir(parents=True, exist_ok=True)
    texts = {
        'sample.ts': '// synthetic parity fixture\nexport const alpha = 1;\nconst message = "שלום 😀";\n\nexport function beta() {\n  return alpha;\n}\n',
        'empty.txt': '',
        'crlf.txt': 'one\r\ntwo 😀\r\nthree\r\n',
        'long.txt': 'x' * 18000 + '\nlast\n',
        'outline.md': '# Heading\n\nA paragraph.\n\n## Child\n\nMore content.\n',
    }
    for name, content in texts.items():
        p = root / name
        raw = content.encode()
        if p.exists() and p.read_bytes() != raw:
            raise RuntimeError(f'fixture changed: {p}')
        p.write_bytes(raw)
        os.utime(p, (1600000000, 1600000000))
    (root / 'binary.bin').write_bytes(b'\x00\x01\x02\x03')
    os.utime(root / 'binary.bin', (1600000000, 1600000000))
    p = str(root / 'sample.ts')
    return [
        ('full', {'path': p, 'fullContent': True}),
        ('lines', {'path': p, 'startLine': 2, 'endLine': 3}),
        ('line-pages', {'path': p, 'chunkType': 'lines', 'limit': 2}),
        ('byte-pages', {'path': p, 'chunkType': 'bytes', 'limit': 17}),
        ('match', {'path': p, 'matchString': 'שלום', 'contextLines': 0}),
        ('match-regex', {'path': p, 'matchString': 'export.*', 'matchStringIsRegex': True, 'contextLines': 0}),
        ('match-absent', {'path': p, 'matchString': 'absent-synthetic-needle'}),
        ('standard-view', {'path': p, 'fullContent': True, 'minify': 'standard'}),
        ('symbols-view', {'path': p, 'fullContent': True, 'minify': 'symbols'}),
        ('markdown-outline', {'path': str(root / 'outline.md'), 'fullContent': True, 'minify': 'symbols'}),
        ('empty-file', {'path': str(root / 'empty.txt'), 'fullContent': True}),
        ('crlf-unicode', {'path': str(root / 'crlf.txt'), 'startLine': 1, 'endLine': 2}),
        ('long-line', {'path': str(root / 'long.txt'), 'chunkType': 'bytes', 'limit': 1000}),
        ('missing-file', {'path': str(root / 'does-not-exist.txt'), 'fullContent': True}),
        ('binary-file', {'path': str(root / 'binary.bin'), 'fullContent': True}),
        ('invalid-range', {'path': p, 'startLine': 5, 'endLine': 2}),
        ('invalid-regex', {'path': p, 'matchString': '[', 'matchStringIsRegex': True}),
        ('unknown-field', {'path': p, 'fullContent': True, 'madeUp': True}),
    ]


def invoke(argv, query, home, cwd):
    home.mkdir(parents=True, exist_ok=True)
    env = {'PATH': '/usr/bin:/bin', 'HOME': str(home), 'OCTOCODE_HOME': str(home),
           'ENABLE_LOCAL': 'true', 'ENABLE_CLONE': 'false', 'NO_COLOR': '1',
           'ALLOWED_PATHS': str(cwd), 'WORKSPACE_ROOT': str(cwd),
           'OCTOCODE_ENABLE_STATS': 'false', 'OCTOCODE_OUTPUT_FORMAT': 'json'}
    command = [*argv, 'tools', 'localFetch', '--queries', json.dumps(query), '--json', '--compact']
    result = subprocess.run(command, cwd=cwd, env=env, text=True, capture_output=True, timeout=30)
    try:
        payload = json.loads(result.stdout)
    except json.JSONDecodeError:
        payload = None
    return {'exit': result.returncode, 'payload': payload, 'stdout': result.stdout, 'stderr': result.stderr}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--node', required=True)
    parser.add_argument('--reference', required=True)
    parser.add_argument('--candidate')
    parser.add_argument('--state', type=Path, required=True)
    args = parser.parse_args()
    root = args.state.resolve()
    cases = fixtures(root / 'fixtures')
    rows = []
    for ident, query in cases:
        reference = invoke([args.node, args.reference], query, root / 'reference-home', root)
        if reference['payload'] is None and not (
            ident in ('unknown-field', 'invalid-range') and reference['exit'] != 0 and reference['stderr']
        ):
            raise RuntimeError(f'reference sensor failed for {ident}: {reference["stderr"]}')
        if ident in ('full', 'lines', 'line-pages', 'byte-pages', 'standard-view', 'symbols-view') and reference['exit'] != 0:
            raise RuntimeError(f'positive reference case failed: {ident}: {reference}')
        candidate = invoke([args.candidate], query, root / 'candidate-home', root) if args.candidate else None
        equal = candidate is not None and equivalent(reference, candidate)
        rows.append({'id': ident, 'query': query, 'reference': reference, 'candidate': candidate, 'equal': equal})
    report = {'scope': 'T01 localFetch initial parity cases; continuation union is a separate required gate',
              'fixtureVersion': 1, 'cases': rows, 'passed': sum(r['equal'] for r in rows),
              'total': len(rows), 'candidateTested': bool(args.candidate),
              'suiteSha256': hashlib.sha256(Path(__file__).read_bytes()).hexdigest()}
    root.mkdir(parents=True, exist_ok=True)
    (root / 'report.json').write_text(json.dumps(report, ensure_ascii=False, indent=2) + '\n')
    print(json.dumps({k: v for k, v in report.items() if k != 'cases'}))
    return 0 if not args.candidate or all(r['equal'] for r in rows) else 1


def equivalent(reference, candidate):
    """JSON whitespace/key order is transport detail; errors and data remain exact."""
    if reference['exit'] != candidate['exit'] or reference['stderr'] != candidate['stderr']:
        return False
    if reference['payload'] is None or candidate['payload'] is None:
        return reference['stdout'] == candidate['stdout']
    return reference['payload'] == candidate['payload']


if __name__ == '__main__':
    sys.exit(main())
