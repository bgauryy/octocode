"""Shared profile harness: per-process counters, an MCP session, one-shot runs, walking pages.

CPU is retired user instructions, not wall time. macOS reads proc_pid_rusage (a one-shot's counters come
from its zombie, so the whole process counts). Linux reads `perf stat`. Moved here from the 2026-11-05 audit
(`.octocode/octocode-dev/release-check-2026-11-05/eval/profile/lib.py`).
"""
import ctypes
import json
import os
import platform
import re
import select
import shutil
import signal
import subprocess
import tempfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[4]
NATIVE = ROOT / 'packages/octocode-native'
DARWIN = platform.system() == 'Darwin'
_ENV_DROP = ('GITHUB_', 'GH_', 'OCTOCODE_', 'NPM_CONFIG_', 'npm_config_', 'GIT_')


class _RusageV4(ctypes.Structure):
    _fields_ = [('uuid', ctypes.c_uint8 * 16)] + [(name, ctypes.c_uint64) for name in (
        'user', 'system', 'pkg_idle', 'intr', 'pageins', 'wired', 'resident', 'phys', 'start', 'exit',
        'cuser', 'csystem', 'cpkg', 'cintr', 'cpageins', 'celapsed', 'diskread', 'diskwrite',
        'q0', 'q1', 'q2', 'q3', 'q4', 'q5', 'q6', 'billed', 'serviced', 'logical_writes',
        'lifemax', 'instr', 'cycles', 'be', 'se', 'intervalmax', 'runnable')]


_libc = ctypes.CDLL('libSystem.B.dylib', use_errno=True) if DARWIN else None


def rusage(pid):
    """{instr, peak} of a live or zombie pid on macOS, or None."""
    usage = _RusageV4()
    if _libc.proc_pid_rusage(pid, 4, ctypes.byref(usage)) != 0:
        return None
    return {'instr': usage.instr, 'peak': usage.lifemax}


def linux_peak(pid):
    """VmHWM of a live pid in bytes, or None."""
    try:
        match = re.search(r'VmHWM:\s+(\d+) kB', Path(f'/proc/{pid}/status').read_text())
    except OSError:
        return None
    return int(match.group(1)) * 1024 if match else None


class PerfStat:
    """Linux: counts one pid's user instructions between start and stop."""

    def __init__(self, pid):
        self.file = tempfile.NamedTemporaryFile('r', suffix='.perf')
        self.process = subprocess.Popen(
            ['perf', 'stat', '-x,', '-e', 'instructions:u', '-p', str(pid), '-o', self.file.name],
            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)

    def stop(self):
        self.process.send_signal(signal.SIGINT)
        self.process.wait()
        return perf_value(Path(self.file.name).read_text())


def perf_value(text):
    """The counter of `perf stat -x,` output; None when perf could not count."""
    for line in text.splitlines():
        fields = line.split(',')
        if len(fields) > 2 and fields[2].startswith('instructions') and fields[0].isdigit():
            return int(fields[0])
    return None


def can_count():
    """Why retired instructions cannot be read here, or None when they can."""
    if DARWIN:
        return None
    if not shutil.which('perf'):
        return 'perf is not installed (linux-tools); instruction counts are unavailable'
    probe = subprocess.run(['perf', 'stat', '-x,', '-e', 'instructions:u', 'true'], capture_output=True, text=True)
    if perf_value(probe.stderr) is None:
        return f'perf cannot count instructions here (kernel.perf_event_paranoid?): {probe.stderr.strip()[:200]}'
    return None


def clean_env(home):
    """An isolated environment: its own home, memory storage, no user tokens, nothing auto-installed."""
    env = {key: value for key, value in os.environ.items() if not key.startswith(_ENV_DROP)}
    env.update({'OCTOCODE_HOME': str(home), 'OCTOCODE_STORAGE_MODE': 'memory', 'OCTOCODE_LSP_AUTO_INSTALL': 'off',
                'OCTOCODE_LSP_PREWARM': 'off', 'OCTOCODE_BETA': 'true', 'OCTOCODE_ENABLE_CLONE': 'true', 'GIT_TERMINAL_PROMPT': '0', 'GIT_CONFIG_NOSYSTEM': '1', 'NO_COLOR': '1'})
    return env


def queries(*rows):
    """A batch request; each row gets the required research fields when it has none."""
    return {'queries': [{'mainGoal': 'profile', 'reasoning': 'profile', **row} for row in rows]}


def request_of(text):
    """The input from a command line: `{"queries":[...]}`, `{"cursor":...}` or one bare query."""
    value = json.loads(text)
    return value if 'queries' in value or 'cursor' in value else queries(value)


def descendants(pid):
    found, stack = [], [pid]
    while stack:
        out = subprocess.run(['pgrep', '-P', str(stack.pop())], capture_output=True, text=True).stdout
        for child in map(int, out.split()):
            found.append(child)
            stack.append(child)
    return found


class Mcp:
    """One `octocode mcp` server over stdio, optionally started through a wrapper such as samply."""

    def __init__(self, binary, cwd, env, wrapper=()):
        self.process = subprocess.Popen([*wrapper, str(binary), 'mcp'], cwd=cwd, env=env, stdin=subprocess.PIPE,
                                        stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, bufsize=0)
        self.next_id = 0
        self.buffer = b''
        self.rpc('initialize', {'protocolVersion': '2025-06-18', 'capabilities': {}, 'clientInfo': {'name': 'profile', 'version': '1'}})
        self.process.stdin.write(b'{"jsonrpc":"2.0","method":"notifications/initialized"}\n')
        # Through a wrapper the counters belong to the server, its child.
        self.pid = descendants(self.process.pid)[-1] if wrapper else self.process.pid

    def rpc(self, method, params, patience=3600):
        self.next_id += 1
        rid = self.next_id
        self.process.stdin.write(json.dumps({'jsonrpc': '2.0', 'id': rid, 'method': method, 'params': params}).encode() + b'\n')
        end = time.time() + patience
        descriptor = self.process.stdout.fileno()
        while True:
            while b'\n' in self.buffer:
                line, _, self.buffer = self.buffer.partition(b'\n')
                try:
                    message = json.loads(line)
                except ValueError:
                    continue
                if message.get('id') == rid and ('result' in message or 'error' in message):
                    return message
            left = end - time.time()
            if left <= 0:
                raise TimeoutError(f'no reply to {method}')
            if select.select([descriptor], [], [], min(left, 5))[0]:
                chunk = os.read(descriptor, 1 << 20)
                if not chunk:
                    raise EOFError('server closed')
                self.buffer += chunk

    def call(self, tool, args):
        """One tools/call: {instr, peak, wall, bytes, content, error}. instr is None without a counter."""
        perf = PerfStat(self.pid) if not DARWIN and shutil.which('perf') else None
        before = rusage(self.pid) if DARWIN else None
        started = time.perf_counter()
        message = self.rpc('tools/call', {'name': tool, 'arguments': args})
        wall = time.perf_counter() - started
        after = rusage(self.pid) if DARWIN else None
        out = {'wall': wall, 'instr': (after['instr'] - before['instr']) if DARWIN else (perf.stop() if perf else None),
               'peak': after['peak'] if DARWIN else linux_peak(self.pid)}
        if 'error' in message:
            return {**out, 'content': None, 'error': str(message['error'])[:300], 'bytes': 0}
        result = message['result']
        content = result.get('structuredContent')
        if content is None:
            try:
                content = json.loads(result['content'][0]['text'])
            except (ValueError, LookupError, TypeError):
                content = None
        error = row_error(content)
        if result.get('isError') and (not error or error == 'noJson'):
            error = '\n'.join(part.get('text', '') for part in result.get('content', [])) or 'toolError'
        return {**out, 'content': content, 'error': error, 'bytes': sum(len(part.get('text', '')) for part in result.get('content', []))}

    def close(self, patience=30):
        try:
            self.process.stdin.close()
        except OSError:
            pass
        try:
            self.process.wait(patience)
        except subprocess.TimeoutExpired:
            self.process.terminate()
            self.process.wait(30)
        self.process.stdout.close()


def oneshot(binary, tool, args, cwd, env, wrapper=()):
    """Whole-process counters of one CLI call: {instr, peak, wall, rc}."""
    argv = [*wrapper, str(binary), tool, json.dumps(args), '--json']
    started = time.perf_counter()
    if DARWIN and not wrapper:
        process = subprocess.Popen(argv, cwd=cwd, env=env, stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        os.waitid(os.P_PID, process.pid, os.WEXITED | os.WNOWAIT)
        counters = rusage(process.pid)
        return {'wall': time.perf_counter() - started, 'rc': process.wait(), **counters}
    if not wrapper and shutil.which('perf'):
        with tempfile.NamedTemporaryFile('r', suffix='.perf') as file:
            done = subprocess.run(['perf', 'stat', '-x,', '-e', 'instructions:u', '-o', file.name, *argv], cwd=cwd, env=env,
                                  stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=False)
            return {'wall': time.perf_counter() - started, 'rc': done.returncode, 'instr': perf_value(Path(file.name).read_text()), 'peak': None}
    done = subprocess.run(argv, cwd=cwd, env=env, stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=False)
    return {'wall': time.perf_counter() - started, 'rc': done.returncode, 'instr': None, 'peak': None}


# Channels that continue the same claim (contract `continuationChannels`): `nextPage` is linear; these branch
# channels resume what a page left out of the same answer, not a wider or a different read.
SAME_CLAIM = ('expandScan', 'nextCandidatePage', 'continueMaterialize', 'continueWalk', 'continueCommitFiles',
              'continueCommentBody', 'continueReviewBody', 'nextImporterWindow')
# ghGetHistoryItem's per-section pages: each continues under the row's `next.<section>`.
SECTIONS = ('body', 'reviewBody', 'patches', 'commentBody', 'files', 'comments', 'reviewComments', 'reviews',
            'commits')
_CHANNEL = re.compile(r'^(?:nextPage|' + '|'.join(SECTIONS) + r'|(?:' + '|'.join(SAME_CLAIM) + r')(?:[2-9]|[1-9][0-9]+)?)$')


def continuations(content, tool=None):
    """Every next call of a walk, as [(channel, input)], in the contract's order: a response split alone when
    there is one; else each row's `next.nextPage` and same-claim branches, then every history section's
    `next.<section>` (comments, files, patches, commits, ...), wherever it sits in the row. A call owned by another tool (`read`, `related`) is no page of this walk."""
    if not isinstance(content, dict):
        return []
    split = content.get('responsePagination')
    if isinstance(split, dict) and split.get('hasMore') and isinstance(split.get('next'), dict):
        found = _as_input(split['next'])
        return [('responsePagination', found)] if found else []
    out = []

    def channel_of(holder, where):
        for key in sorted((k for k in holder if _CHANNEL.match(k)), key=lambda k: (k != 'nextPage', k)):
            value = holder[key]
            if isinstance(value, dict) and value.get('tool') not in (None, tool) and tool:
                continue
            found = _as_input(value)
            if found:
                out.append((f'{where}{key}', found))

    def visit(value, where):
        if isinstance(value, dict):
            for key, child in value.items():
                if key == 'next' and isinstance(child, dict):
                    channel_of(child, f'{where}next.')
                elif key not in ('related', 'hints', 'warnings'):
                    visit(child, f'{where}{key}.' if key != 'data' else where)
        elif isinstance(value, list):
            for item in value:
                visit(item, where)

    for row in content.get('results') or []:
        visit(row, '')
    if not content.get('results'):
        visit(content, '')
    seen, unique = set(), []
    for name, found in out:
        key = json.dumps(found, sort_keys=True)
        if key not in seen:
            seen.add(key)
            unique.append((name, found))
    return unique


def next_input(content, tool=None):
    """The first next call of a walk (see continuations), or None."""
    found = continuations(content, tool)
    return found[0][1] if found else None


def _as_input(value):
    if not isinstance(value, dict):
        return None
    for key in ('input', 'arguments', 'query'):
        if isinstance(value.get(key), dict):
            value = value[key]
            break
    if isinstance(value.get('cursor'), str):
        return {'cursor': value['cursor']}
    return value if 'queries' in value else None


def row_error(content):
    """A short error marker, so a broken request never passes as a measurement."""
    if not isinstance(content, dict):
        return 'noJson'
    if content.get('errorCode') or content.get('error'):
        return str(content.get('errorCode') or content.get('error'))[:80]
    for row in content.get('results') or []:
        if isinstance(row, dict):
            data = row.get('data') if isinstance(row.get('data'), dict) else row
            if row.get('errorCode') or data.get('errorCode') or row.get('status') == 'error':
                return str(row.get('errorCode') or data.get('errorCode') or 'error')[:80]
    return None
