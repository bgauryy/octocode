#!/usr/bin/env python3
"""Local check: retired instructions of the cheap flows, against a per-platform baseline. No wall time.

  dev.mjs perf:check [--bin PATH] [--flows a,b] [--reps N]    compare (default target/profiling/octocode)
  dev.mjs perf:check --write                                  record the current counts as the baseline
  dev.mjs perf:check --self-test

Measurement conditions:
- it counts instructions, not time, on a frozen corpus (flows.CORPUS_REV), so load and tree edits move nothing;
- min of 3 reps, and a flow fails only past both limits: +10% and +20M instructions;
- missing counters, flows or compatible baselines are UNVERIFIED (exit 3), never a passing comparison;
- a flow past the same limits downward prints `cheaper`, so you can --write; it does not fail;
- flows in the list are under about 2 G instructions each. Add a dear one to flows.py as not `gate`.
"""
import argparse
import json
import math
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
from harness import NATIVE, can_count, corpus, queries, stub_env, stub_server, with_stub
from profile import count, fmt

BASELINE = Path(__file__).with_name('perf-baseline.json')
MAX_GROWTH = 0.10
MIN_GROWTH_INSTR = 20e6


def platform_key():
    return f'{sys.platform}-{platform.machine()}'


def toolchain():
    try:
        run = subprocess.run(['rustc', '--version'], cwd=NATIVE, capture_output=True, text=True)
        return run.stdout.strip() if run.returncode == 0 else None
    except OSError:
        return None


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
    rows, missing, failed = {}, {}, {}
    try:
        if any(item['stub'] for item in selected):
            stub, url = stub_server(work)
        for item in selected:
            name = item['id']
            if item['stub'] and stub is None:
                missing[name] = f'stub unavailable: {url}'
                continue
            extra = stub_env(url) if item['stub'] else {}
            counted = count(binary, item, queries(with_stub(item['query'], url if item['stub'] else None)), cwd, extra, reps)
            if counted['errors']:
                failed[name] = f"answered with errors: {counted['errors']}. A broken request is no measurement."
                continue
            invalid = invalid_measurement(counted, reps)
            if invalid:
                missing[name] = invalid
                continue
            rows[name] = counted
    finally:
        if stub:
            stub.terminate()
        shutil.rmtree(work, ignore_errors=True)
    return rows, missing, failed


def positive_count(value):
    return type(value) in (int, float) and math.isfinite(value) and value > 0


def invalid_measurement(row, reps):
    if row.get('reps') != reps or row.get('instructionSamples') != reps:
        return f"instruction counters cover {row.get('instructionSamples', 0)}/{reps} repetitions"
    if not positive_count(row.get('instr')) or not positive_count(row.get('max')):
        return 'instruction counters must be finite and positive'
    if row['max'] < row['instr']:
        return 'maximum instructions are below the minimum'
    return None


def select_flows(parser, wanted):
    eligible = {item['id']: item for item in FLOWS if item['gate']}
    names = set(wanted.split(',')) if wanted is not None else set(eligible)
    unknown = names - eligible.keys()
    if unknown:
        parser.error(f"not perf:check flows: {', '.join(repr(name) for name in sorted(unknown))}; use profile --list")
    if not names:
        parser.error('no performance flows selected')
    return [item for name, item in eligible.items() if name in names]


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
    if args.reps < 1:
        parser.error('--reps must be positive')
    selected = select_flows(parser, args.flows)
    why = can_count()
    if why:
        print(f'perf:check: UNVERIFIED: 0/{len(selected)} flows measured: {why}')
        return 3
    binary = Path(args.bin)
    if not binary.exists():
        raise SystemExit(f'{binary} is missing. Build it first: cargo build --locked --profile profiling -p octocode-cli in packages/octocode-native.')
    started = time.time()
    rows, missing, failed = measure(binary, selected, args.reps, corpus(CORPUS_REV))
    for name, reason in {**missing, **failed}.items():
        print(f'{name}: {reason}')
    print(f'perf:check: {len(rows)}/{len(selected)} flows measured in {time.time() - started:.0f} s.')
    stored = json.loads(BASELINE.read_text()) if BASELINE.exists() else {'corpus': CORPUS_REV, 'platforms': {}}
    key, rustc = platform_key(), toolchain()
    if not rustc:
        print(f'perf:check: UNVERIFIED: 0/{len(selected)} flows compared; rustc identity is unavailable. Baseline unchanged.')
        return 1 if failed else 3
    entry = stored['platforms'].get(key)
    stale = (f'no baseline for {key}' if not entry
             else f"baseline is from {entry['rustc']}, this is {rustc}" if entry['rustc'] != rustc
             else f"baseline corpus {stored['corpus']} is not {CORPUS_REV}" if stored['corpus'] != CORPUS_REV else None)
    if args.write:
        if missing or failed:
            print('perf:check: baseline unchanged: requested measurements are incomplete.')
            return 1 if failed else 3
        if stored['corpus'] != CORPUS_REV:
            stored = {'corpus': CORPUS_REV, 'platforms': {}}
        flows = {} if stale or args.flows is None else dict(entry['flows'])
        flows.update({name: row['instr'] for name, row in rows.items()})
        stored['corpus'] = CORPUS_REV
        stored['platforms'][key] = {'rustc': rustc, 'flows': dict(sorted(flows.items()))}
        BASELINE.write_text(json.dumps(stored, indent=2) + '\n')
        print(f'perf:check: wrote {len(flows)} flows for {key}, {rustc}')
        return 0
    regressed, compared = [], 0
    for name, row in rows.items():
        base = entry['flows'].get(name) if entry and not stale else None
        if not positive_count(base):
            missing[name] = stale or 'no finite positive baseline for this flow'
            print(f"{name}: UNVERIFIED: {missing[name]}; measured {fmt(row['instr'])} instructions")
            continue
        compared += 1
        state = verdict(base, row['instr'])
        ratio = f"{row['instr'] / base:.2f}x"
        print(f"{name:18} {fmt(row['instr']):>9}  base {fmt(base):>9}  {ratio:>6}  spread {row['max'] / row['instr']:.3f}  peak {fmt(row['peak'])}B  {state}")
        if state == 'regressed':
            regressed.append(name)
    print(f'perf:check: {compared}/{len(selected)} flows compared.')
    if regressed or failed:
        print(f"perf:check: FAILED: {', '.join([*regressed, *failed])}. Find the cost: dev.mjs profile --flow <id>. If it is worth it, --write and say why.")
        return 1
    if missing:
        print(f"perf:check: UNVERIFIED: {', '.join(missing)}. No passing comparison for these flows.")
        return 3
    return 0


if __name__ == '__main__':
    sys.exit(main())
