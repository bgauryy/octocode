#!/usr/bin/env python3
"""Walk named flows to the end of their pagination: first call, repeat, and every page, with per-page cost.

  python3 -I walk.py --bin PATH [--flows a,b | --all] [--reps 3] [--out FILE] [--label NAME]
  python3 -I walk.py --compare PIN.json NOW.json        the WP gates: phase cost against the pin, flat pages
  python3 -I walk.py --table FILE.json                  a markdown table of one result file

The corpus is a private clone of harness.corpus (walk-corpus-<rev>); a graph flow's rep starts with no
stored graph. Per rep, one fresh home and MCP server: a warm-up read, the flow's first call (a language-server flow waits
until its index stops answering partially), the same call again (repeat), then every continuation of that
answer, breadth first: the response split when there is one, else each row's nextPage, same-claim branches
(harness.SAME_CLAIM) and each collection's contentPagination.<name>.next, until none remains. Instructions
come from the server process (harness.Mcp.call); child processes such as git or a language server are not in
them. Each figure is the min over reps (3; a live flow 1, --live-reps), per call position. Stub flows also count requests the stub received
(github, registry, osv, classification) per call; live flows sum the provider usage the answers report.

Output JSON per flow: first, repeat, pages[] (page 1 is the repeat), walk totals, `phase` (the value the flow's
own phase names: first, repeat or page:N), flat-page figures (walk.flat), and why the walk stopped.
"""
import argparse
import json
import os
import shutil
import statistics
import subprocess
import sys
import tempfile
import time
import urllib.request
from pathlib import Path

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from flows import BY_ID, CORPUS_REV, FLOWS
from harness import ROOT, Mcp, can_count, clean_env, continuations, corpus, live_env, queries, stub_env, stub_server, with_stub
from perf_check import verdict
from profile import fmt, settle

GROWTH = 1.10  # a long-line flow's pages are flat when the last third of its later pages costs at most 1.10x the first third


def counts(stub):
    if not stub or not stub.get('countsUrl'):
        return None
    with urllib.request.urlopen(stub['countsUrl'], timeout=10) as answer:
        return json.load(answer)


def usage(value):
    """Provider usage the answer reports (clasify, classification), summed."""
    total = {'input': 0, 'output': 0}

    def visit(item):
        if isinstance(item, dict):
            for key, child in item.items():
                if key == 'usage' and isinstance(child, dict):
                    total['input'] += int(child.get('input_tokens') or child.get('inputTokens') or 0)
                    total['output'] += int(child.get('output_tokens') or child.get('outputTokens') or 0)
                else:
                    visit(child)
        elif isinstance(item, list):
            for child in item:
                visit(child)
    visit(value)
    return total


def measured(server, tool, request, stub, channel):
    before = counts(stub)
    answer = server.call(tool, request)
    after = counts(stub)
    row = {k: answer[k] for k in ('instr', 'wall', 'bytes', 'peak', 'error')}
    row['channel'] = channel
    if before is not None:
        row['requests'] = {k: after[k] - before[k] for k in after if after[k] != before[k]}
    spent = usage(answer['content'])
    if spent['input'] or spent['output']:
        row['usage'] = spent
    return row, answer


def private_corpus(rev):
    """A copy-on-write clone of the frozen corpus for this runner alone (APFS `cp -c`: no disk until written).
    Graph flows write `.octocode/graph` into their workspace, and other agents profile the shared copy."""
    shared = corpus(rev)
    clone = ROOT / '.octocode/octocode-dev/profile' / f'walk-corpus-{rev}'
    if not clone.exists():
        subprocess.run(['cp', '-cR' if sys.platform == 'darwin' else '-R', str(shared), str(clone)], check=True)
    shutil.rmtree(clone / '.octocode', ignore_errors=True)
    return clone


def one_rep(binary, flow, query, cwd, extra, stub):
    if flow['tool'] in ('graphBuild', 'graphQuery') or flow.get('build'):
        shutil.rmtree(Path(cwd) / '.octocode', ignore_errors=True)  # each rep builds from nothing
    with tempfile.TemporaryDirectory(prefix='octocode-walk-') as home:
        server = Mcp(binary, cwd, clean_env(home, mode=flow.get('storage', 'memory'), extra=extra))
        try:
            server.call('localFetch', queries({'path': '.', 'view': 'symbols'}))
            if flow.get('build'):
                server.call('graphBuild', queries({'path': flow['build']}))
            if flow.get('server'):
                settle(server, flow, query)  # the index is warm before the measured first call
            first, _ = measured(server, flow['tool'], query, stub, 'first')
            repeat, answer = measured(server, flow['tool'], query, stub, 'repeat')
            pages, seen, stop = [repeat], {json.dumps(query, sort_keys=True)}, 'end'
            pending = continuations(answer['content'], flow['tool'])
            while pending:
                channel, request = pending.pop(0)
                key = json.dumps(request, sort_keys=True)
                if key in seen:
                    stop = 'loop: a continuation repeated an earlier input'
                    continue
                seen.add(key)
                page, answer = measured(server, flow['tool'], request, stub, channel)
                pages.append(page)
                if page['error']:
                    stop = f"error on page {len(pages)}: {page['error']}"
                    break
                pending.extend(continuations(answer['content'], flow['tool']))
            return {'first': first, 'repeat': repeat, 'pages': pages, 'stop': stop, 'peak': max((p.get('peak') or 0) for p in [first, *pages]) or None}
        finally:
            server.close()


def least(rows, key):
    values = [row[key] for row in rows if row and row.get(key) is not None]
    return min(values) if values else None


def merge(reps):
    """Min over reps per call position; a page index missing from a rep is left out of that position."""
    def call(rows):
        out = {'instr': least(rows, 'instr'), 'wall': least(rows, 'wall'), 'bytes': rows[-1].get('bytes'), 'channel': rows[-1].get('channel')}
        errors = sorted({row['error'] for row in rows if row.get('error')})
        if errors:
            out['errors'] = errors
        for extra in ('requests', 'usage'):
            if rows[-1].get(extra):
                out[extra] = rows[-1][extra]
        return out
    longest = max(len(rep['pages']) for rep in reps)
    pages = [call([rep['pages'][i] for rep in reps if i < len(rep['pages'])]) for i in range(longest)]
    later = [page['instr'] for page in pages[1:] if page['instr'] is not None]
    walk_instr = [sum(page['instr'] or 0 for page in rep['pages']) for rep in reps]
    return {
        'first': call([rep['first'] for rep in reps]),
        'repeat': call([rep['repeat'] for rep in reps]),
        'pages': pages,
        'pageCounts': [len(rep['pages']) for rep in reps],
        'walk': {'instr': min(walk_instr), 'bytes': sum(page['bytes'] or 0 for page in pages), 'pages': longest},
        'flat': flat(later, least([rep['first'] for rep in reps], 'instr')),
        'stop': sorted({rep['stop'] for rep in reps}),
        'peak': max((rep['peak'] or 0) for rep in reps) or None,
    }


def flat(later, first):
    """Is a later page's cost independent of its position? growth: median of the last third of the later pages
    over the median of the first third (1.0 is flat; a walk that rescans from the start grows with the page).
    laterOverFirst: the median later page over the first call (RL-L3 wants under 0.5)."""
    if len(later) < 3:
        return None
    third = max(1, len(later) // 3)
    head, tail = statistics.median(later[:third]), statistics.median(later[-third:])
    return {'laterMedian': statistics.median(later), 'laterMax': max(later), 'growth': round(tail / head, 3) if head else None,
            'maxOverMedian': round(max(later) / statistics.median(later), 3), 'laterOverFirst': round(statistics.median(later) / first, 3) if first else None}


def phase_value(flow, merged):
    phase = flow['phase']
    if phase == 'first':
        return merged['first']['instr']
    if phase == 'repeat':
        return merged['repeat']['instr']
    if phase.startswith('page:'):
        index = int(phase.split(':')[1]) - 1
        return merged['pages'][index]['instr'] if index < len(merged['pages']) else None
    return None


def run(binary, selected, reps, cwd, live_on_stub=False, live_reps=1):
    if live_on_stub:
        selected = [{**item, 'stub': True} if item.get('live') else item for item in selected]
    work = tempfile.mkdtemp(prefix='octocode-walk-')
    stub, info = (None, None)
    if any(item['stub'] for item in selected):
        stub, info = stub_server(work)
        if stub is None:
            raise SystemExit(info)
    results = {}
    try:
        for item in selected:
            started = time.time()
            extra = stub_env(info) if item['stub'] else live_env(item['live']) if item.get('live') else {}
            query = queries(with_stub(item['query'], info if item['stub'] else None))
            try:
                count_ = live_reps if item.get('live') and not live_on_stub else reps
                merged = merge([one_rep(binary, item, query, cwd, extra, info if item['stub'] else None) for _ in range(count_)])
            except (TimeoutError, EOFError, OSError) as failure:
                results[item['id']] = {'tool': item['tool'], 'failed': f'{type(failure).__name__}: {failure}'}
                print(f"{item['id']:26} FAILED {failure}", flush=True)
                continue
            merged['phase'] = {'name': item['phase'], 'instr': phase_value(item, merged)}
            merged.update({'tool': item['tool'], 'query': item['query'], 'reps': count_, 'seconds': round(time.time() - started, 1),
                           **({'stub': True} if item['stub'] else {}), **({'live': item['live']} if item.get('live') else {})})
            results[item['id']] = merged
            flat_ = merged['flat'] or {}
            print(f"{item['id']:26} first {fmt(merged['first']['instr']):>9} repeat {fmt(merged['repeat']['instr']):>9} "
                  f"pages {merged['walk']['pages']:>4} walk {fmt(merged['walk']['instr']):>9} growth {flat_.get('growth', '-')} "
                  f"stop {merged['stop']} errors {merged['first'].get('errors', '')}", flush=True)
    finally:
        if stub:
            stub.terminate()
            stub.wait()
        shutil.rmtree(work, ignore_errors=True)
    return results


def binary_facts(binary):
    digest = subprocess.run(['shasum', '-a', '256', str(binary)], capture_output=True, text=True).stdout.split()[0]
    version = subprocess.run([str(binary), '--version'], capture_output=True, text=True).stdout.strip()
    return {'path': str(binary), 'sha256': digest, 'version': version, 'mtime': time.strftime('%Y-%m-%dT%H:%M:%S%z', time.localtime(binary.stat().st_mtime)), 'bytes': binary.stat().st_size}


def compare(pin_path, now_path):
    """Exit 1 when a flow's phase cost exceeds GROWTH x the pin, or a flow's later pages are not flat."""
    pin, now = json.loads(Path(pin_path).read_text()), json.loads(Path(now_path).read_text())
    failed = []
    print(f"pin {pin['binary']['version']} {pin['binary']['sha256'][:12]}  now {now['binary']['version']} {now['binary']['sha256'][:12]}")
    for name, row in now['flows'].items():
        base = pin['flows'].get(name)
        if 'failed' in row or not base or 'failed' in base:
            print(f'{name:26} -  ({row.get("failed") or "no pin row"})')
            continue
        b, n = base['phase']['instr'], row['phase']['instr']
        ratio = n / b if b and n else None
        growth = (row.get('flat') or {}).get('growth')
        state = verdict(b, n) if b and n else 'same'  # perf_check's rule: over only past +10% and +20M
        verdict_ = 'over' if state == 'regressed' else 'ok' if state == 'same' else 'cheaper'
        if growth and growth > GROWTH and name in LONG_LINE:
            verdict_ = 'pages not flat' if verdict_ != 'over' else 'over, pages not flat'
        if verdict_ in ('over', 'pages not flat', 'over, pages not flat'):
            failed.append(name)
        print(f"{name:26} {row['phase']['name']:8} pin {fmt(b):>9} now {fmt(n):>9} {f'{ratio:.2f}x' if ratio else '-':>6}  "
              f"walk {fmt(base['walk']['instr'])} -> {fmt(row['walk']['instr'])}  growth {growth}  {verdict_}")
    print(f"walk.py compare: {'FAILED: ' + ', '.join(failed) if failed else 'every flow within the gates'}")
    return 1 if failed else 0


def table(path):
    """A markdown table of one result file, a row per flow."""
    stored = json.loads(Path(path).read_text())
    facts = stored['binary']
    lines = [f"Binary `{facts['path']}`: {facts['version']}, sha256 `{facts['sha256']}`, mtime {facts['mtime']}. Corpus {stored['corpus']}. Instructions of the server process, min over reps.", '',
             '| Flow | Tool | Phase | Phase cost | First | Repeat | Pages | Walk | Walk bytes | Growth | Later/first | Requests (first) | Tokens (first) | Peak | Stop |',
             '|---|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---|---|---:|---|']
    for name, row in stored['flows'].items():
        if 'failed' in row:
            lines.append(f"| {name} | {row['tool']} | - | failed: {row['failed']} | | | | | | | | | | | |")
            continue
        flat_ = row.get('flat') or {}
        requests = ', '.join(f'{k} {v}' for k, v in (row['first'].get('requests') or {}).items()) or '-'
        tokens = row['first'].get('usage')
        spent = f"{tokens['input']:,} in / {tokens['output']} out" if tokens else '-'
        errors = f" (errors: {', '.join(row['first']['errors'])})" if row['first'].get('errors') else ''
        lines.append(f"| {name} | {row['tool']} | {row['phase']['name']} | {fmt(row['phase']['instr'])} | {fmt(row['first']['instr'])} | {fmt(row['repeat']['instr'])} | "
                     f"{row['walk']['pages']} | {fmt(row['walk']['instr'])} | {row['walk']['bytes']:,} | {flat_.get('growth', '-')} | {flat_.get('laterOverFirst', '-')} | {requests} | "
                     f"{spent} | {fmt(row['peak'])}B | {', '.join(row['stop'])}{errors} |")
    print('\n'.join(lines))
    return 0


LONG_LINE = ('ls.longline', 'ls.longline.huge', 'lf.longline', 'as.longline', 'gh.item.patch.long')


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--bin')
    parser.add_argument('--flows')
    parser.add_argument('--all', action='store_true')
    parser.add_argument('--skip', default='', help='flow ids to leave out')
    parser.add_argument('--reps', type=int, default=3)
    parser.add_argument('--live-reps', type=int, default=1, help='reps of a live flow: it is judged by tokens and calls, not time')
    parser.add_argument('--out')
    parser.add_argument('--label', default='')
    parser.add_argument('--compare', nargs=2, metavar=('PIN', 'NOW'))
    parser.add_argument('--table', metavar='FILE', help='print a markdown table of a result file')
    parser.add_argument('--live-on-stub', action='store_true', help='send live flows to the stub provider: shape and provider-call counts, no tokens spent')
    args = parser.parse_args()
    if args.compare:
        return compare(*args.compare)
    if args.table:
        return table(args.table)
    why = can_count()
    if why:
        print(f'warning: {why}')
    binary = Path(args.bin)
    wanted = set(args.flows.split(',')) if args.flows else None
    if wanted is None and not args.all:
        raise SystemExit('Name --flows a,b or --all.')
    unknown = (wanted or set()) - set(BY_ID)
    if unknown:
        raise SystemExit(f'unknown flows: {sorted(unknown)}')
    skip = set(filter(None, args.skip.split(',')))
    selected = [item for item in FLOWS if (wanted is None or item['id'] in wanted) and item['id'] not in skip]
    results = run(binary, selected, args.reps, private_corpus(CORPUS_REV), args.live_on_stub, args.live_reps)
    if args.out:
        out = Path(args.out)
        stored = json.loads(out.read_text()) if out.exists() else {}
        if stored.get('binary', {}).get('sha256') not in (None, binary_facts(binary)['sha256']):
            raise SystemExit(f'{out} holds another binary; write to a new file')
        stored.update({'label': args.label or stored.get('label', ''), 'binary': binary_facts(binary), 'corpus': CORPUS_REV,
                       'host': {'platform': sys.platform, 'load': os.getloadavg()[0]}, 'flows': {**stored.get('flows', {}), **results}})
        out.write_text(json.dumps(stored, indent=1) + '\n')
        print(f'wrote {len(results)} flows to {out}')
    return 0


if __name__ == '__main__':
    sys.exit(main())
