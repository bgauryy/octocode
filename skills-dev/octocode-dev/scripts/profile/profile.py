#!/usr/bin/env python3
"""Profile one tool flow: retired instructions, peak memory, a flamegraph and a top-frame table.

  dev.mjs profile <tool> '<query json>' [--phase first|repeat|page:N|one] [--cwd DIR]
  dev.mjs profile --flow <id>      a named flow on the frozen corpus (--list shows them)

  --bin PATH   binary to profile (default target/profiling/octocode; --build builds it first)
  --secs N     keep sampling for at least N seconds (default 10)
  --reps N     counting reps, min taken (default 3)
  --heap       also write what the server holds after the flow (macOS malloc_history, Linux heaptrack)
  --top N      rows per table (default 25)

The phase picks what is measured: `first` a fresh MCP server's first call of the flow, `repeat` the same
call again, `page:N` page N of its walk, `one` a whole one-shot CLI process. Counting runs without a
sampler. Then samply records a server looping that phase, and the table covers only the loop.
Open the flamegraph with `samply load <dir>/profile.json.gz`. Output: .octocode/octocode-dev/profile/.
"""
import argparse
import collections
import gzip
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from flows import BY_ID, CORPUS_REV, FLOWS
from harness import DARWIN, Mcp, NATIVE, ROOT, can_count, clean_env, corpus, next_input, oneshot, queries, request_of, stub_env, stub_server

DEFAULT_BIN = NATIVE / 'target/profiling/octocode'
IDLE = re.compile(r'^(swtch_pri|thread_switch|cthread_yield|sched_yield|__psynch_cvwait|semaphore_wait_trap|semaphore_timedwait_trap|__psynch_mutexwait|kevent|__semwait_signal|mach_msg|__workq_kernreturn|__ulock_wait|_pthread_wqthread|__select|poll|__accept|nanosleep|start_wqthread|thread_start|__wait4|__recvfrom|__read_nocancel|epoll_wait|futex|syscall$)')
SCAFFOLD = re.compile(r'^(std::|core::|alloc::|tokio::|rayon|<|__rust|_pthread|thread_start|start$|main$)')


def fmt(value):
    if value is None:
        return 'n/a'
    return f'{value / 1e9:.1f} G' if value >= 1e10 else f'{value / 1e6:.1f}M'


def prepare(server, flow, query):
    """Run what comes before the measured call; returns the request the phase measures."""
    if flow.get('build'):
        server.call('graphBuild', queries({'path': flow['build']}))
    phase = flow['phase']
    if phase == 'first':
        return query
    first = settle(server, flow, query)
    if phase == 'repeat':
        return query
    following = next_input(first['content'])
    for _ in range(int(phase.split(':')[1]) - 2):
        if following is None:
            break
        following = next_input(server.call(flow['tool'], following)['content'])
    if following is None:
        raise SystemExit(f"{flow['id']}: the walk ended before {phase}")
    return following


def settle(server, flow, query):
    """The first answer; a language server's flow waits until its index stops answering partially."""
    answer = server.call(flow['tool'], query)
    if flow.get('server'):
        for _ in range(120):
            rows = (answer['content'] or {}).get('results') or [{}]
            if not (rows[0].get('data') or {}).get('isPartial'):
                break
            time.sleep(2)
            answer = server.call(flow['tool'], query)
    return answer


def count(binary, flow, query, cwd, extra, reps):
    """Min instructions over reps, each on a fresh home and server."""
    rows = []
    for _ in range(reps):
        with tempfile.TemporaryDirectory(prefix='octocode-profile-') as home:
            env = clean_env(home, extra=extra)
            if flow['phase'] == 'one':
                rows.append(oneshot(binary, flow['tool'], query, cwd, env))
                continue
            server = Mcp(binary, cwd, env)
            try:
                server.call('localFetch', queries({'path': '.', 'view': 'symbols'}))
                rows.append(server.call(flow['tool'], prepare(server, flow, query)))
            finally:
                server.close()
    errors = {row.get('error') for row in rows if row.get('error')} | {f"exit {row['rc']}" for row in rows if row.get('rc')}
    instr = [row['instr'] for row in rows if row.get('instr') is not None]
    return {'instr': min(instr) if instr else None, 'max': max(instr) if instr else None, 'peak': max((row.get('peak') or 0) for row in rows) or None,
            'wall': min(row['wall'] for row in rows), 'bytes': rows[-1].get('bytes'), 'errors': sorted(errors)}


def record(binary, flow, query, cwd, extra, secs, out):
    """A samply profile of the phase looping for at least secs; returns the loop's clock window."""
    profile = out / 'profile.json.gz'
    wrapper = ('samply', 'record', '--save-only', '--unstable-presymbolicate', '--rate', '1000', '--output', str(profile), '--')
    with tempfile.TemporaryDirectory(prefix='octocode-profile-') as home:
        env = clean_env(home, extra=extra)
        if flow['phase'] == 'one':
            subprocess.run(['samply', 'record', '--save-only', '--unstable-presymbolicate', '--rate', '1000', '--iteration-count', str(max(1, secs // 2)),
                            '--output', str(profile), '--', str(binary), flow['tool'], json.dumps(query), '--json'],
                           cwd=cwd, env=env, stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=True)
            return None, 0
        server = Mcp(binary, cwd, env, wrapper=wrapper)
        try:
            request = prepare(server, flow, query)
            started, calls = time.time(), 0
            while True:
                server.call(flow['tool'], request)
                calls += 1
                if time.time() - started >= secs:
                    break
            window = (started * 1000, time.time() * 1000)
        finally:
            server.close()
    return window, calls


def symbolizer(profile, sidecar):
    tables = {}
    strings = sidecar['string_table']
    for entry in sidecar['data']:
        known = dict(map(tuple, entry['known_addresses']))
        symbols = entry['symbol_table']
        tables[entry['debug_name']] = (known, symbols)
    libs = [lib.get('debugName') or lib['name'] for lib in profile['libs']]

    def resolve(lib_index, address):
        table = tables.get(libs[lib_index]) if lib_index is not None else None
        if table:
            index = table[0].get(address)
            if index is not None and index < len(table[1]):
                return strings[table[1][index]['symbol']]
        return f'{libs[lib_index] if lib_index is not None else "?"}+0x{address:x}'
    return resolve


def clean(symbol):
    symbol = re.sub(r'::h[0-9a-f]{16}$', '', symbol)
    symbol = re.sub(r'\bcore::ptr::drop_in_place<', 'drop<', symbol)
    return symbol[:130]


def tables(path, window, binary_name):
    """{'self','inclusive','busy_ms'} by CPU time of the binary's threads inside the window."""
    with gzip.open(path) as file:
        profile = json.load(file)
    resolve = symbolizer(profile, json.loads(Path(str(path).removesuffix('.gz') + '.syms.json').read_text()))
    own, inclusive = collections.Counter(), collections.Counter()
    start = profile['meta']['startTime']
    busy = 0.0
    for thread in profile['threads']:
        if binary_name not in (thread.get('processName') or ''):
            continue
        frames, funcs, resources, stacks = thread['frameTable'], thread['funcTable'], thread['resourceTable'], thread['stackTable']
        memo = {}

        def symbols_of(stack):
            if stack not in memo:
                frame = stacks['frame'][stack]
                resource = funcs['resource'][frames['func'][frame]]
                lib = resources['lib'][resource] if resource is not None and resource >= 0 else None
                symbol = clean(resolve(lib, frames['address'][frame]))
                prefix = stacks['prefix'][stack]
                memo[stack] = (symbol, (symbols_of(prefix)[1] if prefix is not None else frozenset()) | {symbol})
            return memo[stack]
        samples = thread['samples']
        deltas = samples.get('threadCPUDelta') or [None] * samples['length']
        for stack, moment, delta in zip(samples['stack'], samples['time'], deltas):
            if stack is None or not delta:
                continue
            if window and not window[0] <= start + moment <= window[1]:
                continue
            leaf, chain = symbols_of(stack)
            if IDLE.match(leaf):
                continue
            busy += delta
            own[leaf] += delta
            for symbol in chain:
                inclusive[symbol] += delta
    return {'self': own, 'inclusive': inclusive, 'busy_ms': busy / 1000}


def top_rows(found, count_):
    busy = max(found['busy_ms'] * 1000, 1)
    own = [(value * 100 / busy, name) for name, value in found['self'].most_common(count_)]
    ours = [(value * 100 / busy, name) for name, value in found['inclusive'].most_common(2000) if 'octocode' in name and not SCAFFOLD.match(name)]
    other = [(value * 100 / busy, name) for name, value in found['inclusive'].most_common(400) if 'octocode' not in name and not SCAFFOLD.match(name)]
    return own, ours[:count_], other[:count_ // 2]


def write_report(out, title, counted, found, top, calls, binary):
    own, ours, other = top_rows(found, top)
    lines = [f'# {title}', '', f'Binary: `{binary}`, built {time.strftime("%F %T", time.localtime(binary.stat().st_mtime))}.', '',
             f'Instructions, min of {counted["reps"]}: **{fmt(counted["instr"])}** (max {fmt(counted["max"])}). Peak footprint {fmt(counted["peak"])}B. Wall {counted["wall"]:.2f} s. Answer {counted["bytes"]} bytes.',
             f'Sampled: {calls} calls, {found["busy_ms"] / 1000:.1f} s of busy CPU. Flamegraph: `samply load {out}/profile.json.gz`.', '']
    if counted['errors']:
        lines += [f'**The flow answered with errors: {counted["errors"]}. Do not read this as a cost.**', '']
    for name, rows in (('Self', own), ('Inclusive, our frames', ours), ('Inclusive, dependencies', other)):
        lines += [f'## {name}', '', '| Share | Frame |', '|---:|---|'] + [f'| {share:.1f}% | `{frame}` |' for share, frame in rows] + ['']
    (out / 'top.md').write_text('\n'.join(lines))
    return '\n'.join(lines)


def heap(binary, flow, query, cwd, extra, out):
    """What the server holds after the flow: live allocations by call tree."""
    with tempfile.TemporaryDirectory(prefix='octocode-profile-') as home:
        env = clean_env(home, extra=extra)
        if not DARWIN:
            if not shutil.which('heaptrack'):
                print('heap: heaptrack is not installed; apt install heaptrack')
                return
            server = Mcp(binary, cwd, env, wrapper=('heaptrack', '-o', str(out / 'heaptrack')))
            server.call(flow['tool'], prepare(server, flow, query))
            server.close()
            print(f'heap: heaptrack_print {out}/heaptrack.*.zst --print-peaks')
            return
        server = Mcp(binary, cwd, {**env, 'MallocStackLogging': 'lite'})
        server.call(flow['tool'], prepare(server, flow, query))
        report = out / 'heap.txt'
        with report.open('w') as file:
            subprocess.run(['malloc_history', str(server.pid), '-callTree', '-invert', '-consolidateByFunction'], stdout=file, stderr=subprocess.STDOUT, check=False)
        server.close()
        print(f'heap: live allocations after the flow: {report}')


def parse_args(argv):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('tool', nargs='?')
    parser.add_argument('query', nargs='?')
    parser.add_argument('--flow')
    parser.add_argument('--list', action='store_true')
    parser.add_argument('--phase', default=None)
    parser.add_argument('--cwd', default=None)
    parser.add_argument('--bin', default=str(DEFAULT_BIN))
    parser.add_argument('--build', action='store_true')
    parser.add_argument('--secs', type=int, default=10)
    parser.add_argument('--reps', type=int, default=3)
    parser.add_argument('--top', type=int, default=25)
    parser.add_argument('--heap', action='store_true')
    parser.add_argument('--no-sample', action='store_true', help='Counters only')
    parser.add_argument('--out', default=None)
    return parser.parse_args(argv)


def resolve_flow(args):
    if args.flow:
        flow = dict(BY_ID[args.flow])
        return flow, queries(flow['query']), Path(args.cwd) if args.cwd else corpus(CORPUS_REV)
    if not args.tool or not args.query:
        raise SystemExit('Name a tool and its query, or a --flow. See --help and --list.')
    flow = {'id': args.tool, 'tool': args.tool, 'phase': args.phase or 'first', 'stub': False, 'server': None, 'build': None}
    return flow, request_of(args.query), Path(args.cwd).resolve() if args.cwd else ROOT


def main(argv=None):
    args = parse_args(sys.argv[1:] if argv is None else argv)
    if args.list:
        for item in FLOWS:
            print(f"{item['id']:22} {item['phase']:8} {'gate' if item['gate'] else '    '} {item['why']}")
        return 0
    flow, query, cwd = resolve_flow(args)
    if args.phase:
        flow['phase'] = args.phase
    binary = Path(args.bin)
    if args.build:
        subprocess.run(['cargo', 'build', '--locked', '--profile', 'profiling', '-p', 'octocode-cli'], cwd=NATIVE, check=True)
    if not binary.exists():
        raise SystemExit(f'{binary} is missing. Build it: --build, or cargo build --locked --profile profiling -p octocode-cli in packages/octocode-native.')
    why = can_count()
    if why:
        print(f'warning: {why}')
    if not args.no_sample and not shutil.which('samply'):
        raise SystemExit('samply is missing: cargo install --locked samply (pinned in dev-tools.json).')
    out = Path(args.out) if args.out else ROOT / '.octocode/octocode-dev/profile' / f'{time.strftime("%Y%m%d-%H%M%S")}-{flow["id"]}-{flow["phase"].replace(":", "")}'
    out.mkdir(parents=True, exist_ok=True)
    work = tempfile.mkdtemp(prefix='octocode-profile-')
    stub, extra = None, {}
    try:
        if flow.get('stub'):
            stub, url = stub_server(work)
            if stub is None:
                raise SystemExit(url)
            extra = stub_env(url)
        counted = {**count(binary, flow, query, cwd, extra, args.reps), 'reps': args.reps}
        (out / 'counters.json').write_text(json.dumps({'flow': flow['id'], 'phase': flow['phase'], **counted}, indent=1))
        print(f"{flow['id']} {flow['phase']}: {fmt(counted['instr'])} (max {fmt(counted['max'])}), peak {fmt(counted['peak'])}B, {counted['wall']:.2f} s, errors {counted['errors']}")
        if not args.no_sample:
            window, calls = record(binary, flow, query, cwd, extra, args.secs, out)
            found = tables(out / 'profile.json.gz', window, binary.name)
            print(write_report(out, f"{flow['id']}, phase {flow['phase']}", counted, found, args.top, calls, binary))
        if args.heap:
            heap(binary, flow, query, cwd, extra, out)
        print(f'Saved: {out}')
    finally:
        if stub:
            stub.terminate()
        shutil.rmtree(work, ignore_errors=True)
    return 0


if __name__ == '__main__':
    sys.exit(main())
