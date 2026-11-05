#!/usr/bin/env python3
"""Local check: retired instructions of the cheap flows, against a per-platform baseline. No wall time.

  dev.mjs perf:check [--bin PATH] [--flows a,b] [--reps N]    compare (default target/profiling/octocode)
  dev.mjs perf:check --write                                  record the current counts as the baseline
  dev.mjs perf:check --self-test

It cannot turn slow or flaky by construction:
- it counts instructions, not time, on a frozen corpus (flows.CORPUS_REV), so load and tree edits move nothing;
- min of 3 reps, and a flow fails only past both limits: +10% and +20M instructions;
- a platform or toolchain without a baseline prints that and passes: never a red gate on another machine;
- a flow past the same limits downward prints `cheaper`, so you can --write; it does not fail;
- flows in the list are under about 2 G instructions each. Add a dear one to flows.py as not `gate`.
"""
import argparse
import json
import os
import platform
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from flows import CORPUS_REV, FLOWS
from harness import NATIVE, can_count, corpus, queries, stub_env, stub_server
from profile import count, fmt

BASELINE = Path(__file__).with_name('perf-baseline.json')
MAX_GROWTH = 0.10
MIN_GROWTH_INSTR = 20e6


def platform_key():
    return f'{sys.platform}-{platform.machine()}'


def toolchain():
    return subprocess.run(['rustc', '--version'], cwd=NATIVE, capture_output=True, text=True).stdout.strip()


def verdict(base, now):
    """regressed | cheaper | same, for one flow's instructions against its baseline."""
    limit = max(base * MAX_GROWTH, MIN_GROWTH_INSTR)
    if now - base > limit:
        return 'regressed'
    return 'cheaper' if base - now > limit else 'same'


def self_test():
    assert verdict(1_000_000_000, 1_090_000_000) == 'same'
    assert verdict(1_000_000_000, 1_110_000_000) == 'regressed'
    assert verdict(10_000_000, 25_000_000) == 'same', 'small flows need the absolute limit too'
    assert verdict(10_000_000, 31_000_000) == 'regressed'
    assert verdict(1_000_000_000, 850_000_000) == 'cheaper'
    assert verdict(5_000_000, 4_000_000) == 'same', 'noise on a tiny flow is not a saving'
    print('perf_check self-test: ok')
    return 0


def measure(binary, selected, reps, cwd):
    work = tempfile.mkdtemp(prefix='octocode-perf-')
    stub, url = (None, None)
    if any(item['stub'] for item in selected):
        stub, url = stub_server(work)
        if stub is None:
            print(f'skipping stub flows: {url}')
            selected = [item for item in selected if not item['stub']]
    rows = {}
    try:
        for item in selected:
            extra = stub_env(url) if item['stub'] else {}
            counted = count(binary, item, queries(item['query']), cwd, extra, reps)
            if counted['errors']:
                raise SystemExit(f"{item['id']} answered with errors: {counted['errors']}. A broken request is no measurement.")
            rows[item['id']] = counted
    finally:
        if stub:
            stub.terminate()
        shutil.rmtree(work, ignore_errors=True)
    return rows


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--bin', default=str(NATIVE / 'target/profiling/octocode'))
    parser.add_argument('--flows')
    parser.add_argument('--reps', type=int, default=3)
    parser.add_argument('--write', action='store_true')
    parser.add_argument('--self-test', action='store_true')
    args = parser.parse_args()
    if args.self_test:
        return self_test()
    why = can_count()
    if why:
        print(f'perf:check: skipped: {why}')
        return 0
    binary = Path(args.bin)
    if not binary.exists():
        raise SystemExit(f'{binary} is missing. Build it first: cargo build --locked --profile profiling -p octocode-cli in packages/octocode-native.')
    wanted = set(args.flows.split(',')) if args.flows else None
    selected = [item for item in FLOWS if item['gate'] and (wanted is None or item['id'] in wanted)]
    started = time.time()
    rows = measure(binary, selected, args.reps, corpus(CORPUS_REV))
    stored = json.loads(BASELINE.read_text()) if BASELINE.exists() else {'corpus': CORPUS_REV, 'platforms': {}}
    key, rustc = platform_key(), toolchain()
    entry = stored['platforms'].get(key)
    if args.write:
        flows = {} if not entry or wanted is None else dict(entry['flows'])
        flows.update({name: row['instr'] for name, row in rows.items()})
        stored['corpus'] = CORPUS_REV
        stored['platforms'][key] = {'rustc': rustc, 'flows': dict(sorted(flows.items()))}
        BASELINE.write_text(json.dumps(stored, indent=2) + '\n')
        print(f'perf:check: wrote {len(flows)} flows for {key}, {rustc}')
        return 0
    stale = (f'no baseline for {key}' if not entry
             else f"baseline is from {entry['rustc']}, this is {rustc}" if entry['rustc'] != rustc
             else f"baseline corpus {stored['corpus']} is not {CORPUS_REV}" if stored['corpus'] != CORPUS_REV else None)
    regressed = []
    for name, row in rows.items():
        base = entry['flows'].get(name) if entry and not stale else None
        state = verdict(base, row['instr']) if base else 'new'
        ratio = f"{row['instr'] / base:.2f}x" if base else '-'
        print(f"{name:18} {fmt(row['instr']):>9}  base {fmt(base):>9}  {ratio:>6}  spread {row['max'] / row['instr']:.3f}  peak {fmt(row['peak'])}B  {state}")
        if state == 'regressed':
            regressed.append(name)
    print(f'perf:check: {len(rows)} flows in {time.time() - started:.0f} s.')
    if stale:
        print(f'perf:check: nothing compared: {stale}. Run with --write, and say why in the change.')
        return 0
    if regressed:
        print(f"perf:check: FAILED: {', '.join(regressed)}. Find the cost: dev.mjs profile --flow <id>. If it is worth it, --write and say why.")
        return 1
    return 0


if __name__ == '__main__':
    sys.exit(main())
