#!/usr/bin/env python3
"""Corpus sweep for view:"compact" and the parse work bound, on an optimized build.

  python3 -I skills-dev/octocode-dev/scripts/compact_sweep.py [ROOT ...]

Runs `examples/compact_sweep.rs` of octocode-engine (profiling profile) over ROOT (default: the
benchmark corpus, packages/octocode-benchmark/local/repos). It prints, per grammar, the most parse
work per byte any clean file needed (the measurement behind REPORTS_PER_KIB and READS_PER_CHUNK in
crates/engine/src/signatures/extractor/parse.rs), savings per extension, whitespace-only counts by
reason, and every file whose compact view changed its tokens (`CHANGED`). Output is also saved under
.octocode/octocode-dev/compact-sweep/<date>.txt. Exit status 1 when any file changed.
"""
import datetime
import os
import subprocess
import sys

REPO = os.path.dirname(os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__)))))
NATIVE = os.path.join(REPO, 'packages', 'octocode-native')
DEFAULT_ROOT = os.path.join(REPO, 'packages', 'octocode-benchmark', 'local', 'repos')


def main():
    roots = sys.argv[1:] or [DEFAULT_ROOT]
    command = ['cargo', 'run', '--locked', '--quiet', '--profile', 'profiling', '-p', 'octocode-engine',
               '--all-features', '--example', 'compact_sweep', '--', *roots]
    result = subprocess.run(command, cwd=NATIVE, capture_output=True, text=True)
    out_dir = os.path.join(REPO, '.octocode', 'octocode-dev', 'compact-sweep')
    os.makedirs(out_dir, exist_ok=True)
    stamp = datetime.datetime.now().strftime('%Y-%m-%dT%H%M')
    with open(os.path.join(out_dir, f'{stamp}.txt'), 'w') as saved:
        saved.write(result.stdout + result.stderr)
    sys.stdout.write(result.stdout)
    if result.returncode != 0:
        sys.stderr.write(result.stderr)
        return result.returncode
    changed = [line for line in result.stdout.splitlines() if line.startswith('CHANGED ')]
    return 1 if changed else 0


if __name__ == '__main__':
    sys.exit(main())
