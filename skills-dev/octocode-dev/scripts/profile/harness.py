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


def clean_env(home, mode='memory', extra=None):
    """An isolated environment: its own home, memory storage, no user tokens, nothing auto-installed."""
    env = {key: value for key, value in os.environ.items() if not key.startswith(_ENV_DROP)}
    env.update({'OCTOCODE_HOME': str(home), 'OCTOCODE_STORAGE_MODE': mode, 'OCTOCODE_LSP_AUTO_INSTALL': 'off',
                'OCTOCODE_LSP_PREWARM': 'off', 'OCTOCODE_BETA': 'true', 'OCTOCODE_ENABLE_CLONE': 'true', 'GIT_TERMINAL_PROMPT': '0', 'GIT_CONFIG_NOSYSTEM': '1', 'NO_COLOR': '1'})
    env.update(extra or {})
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
                content = {}
        return {**out, 'content': content, 'error': row_error(content), 'bytes': sum(len(part.get('text', '')) for part in result.get('content', []))}

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
_CHANNEL = re.compile(r'^(?:nextPage|(?:' + '|'.join(SAME_CLAIM) + r')(?:[2-9]|[1-9][0-9]+)?)$')


def continuations(content, tool=None):
    """Every next call of a walk, as [(channel, input)], in the contract's order: a response split alone when
    there is one; else each row's `next.nextPage` and same-claim branches, then every collection's
    `contentPagination.<name>.next` (comments, files, patches, sections, candidates), wherever it sits in the
    row. A call owned by another tool (`read`, `related`) is no page of this walk."""
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


def corpus(rev):
    """A frozen copy of the native crates at one commit, so a change in the tree moves no count."""
    target = ROOT / '.octocode/octocode-dev/profile' / f'corpus-{rev}'
    workspace = target / 'packages/octocode-native'
    if workspace.exists():
        return extras(workspace)
    target.mkdir(parents=True, exist_ok=True)
    archive = subprocess.run(
        ['git', 'archive', '--format=tar', rev, '--', 'packages/octocode-native/crates', 'packages/octocode-native/Cargo.toml',
         'packages/octocode-native/Cargo.lock', 'packages/octocode-native/rust-toolchain.toml',
         'packages/octocode-config/contract/tool_types.rs'],
        cwd=ROOT, capture_output=True, check=False)
    if archive.returncode != 0:
        raise SystemExit(f'no corpus: commit {rev} is not in this clone ({archive.stderr.decode()[:200].strip()}). Re-pin CORPUS_REV and rewrite the baseline.')
    subprocess.run(['tar', '-xf', '-', '-C', str(target)], input=archive.stdout, check=True)
    # The largest source in the repo, inside the workspace so that path policy admits it.
    (workspace / 'big').mkdir()
    shutil.move(target / 'packages/octocode-config/contract/tool_types.rs', workspace / 'big/tool_types.rs')
    subprocess.run(['git', 'init', '-q', str(workspace)], check=True)
    return extras(workspace)


def minified_line(size):
    """One deterministic minified JavaScript line of `size` bytes; a `needle_call` every ~55 bytes."""
    parts, total, i = [], 0, 0
    while total < size:
        part = f'function f{i}(a){{return needle_call(a,{i})}}var v{i}=f{i}(1);'
        parts.append(part)
        total += len(part)
        i += 1
    return ''.join(parts)[:size]


def extras(workspace):
    """Next-wave fixtures beside the frozen crates, outside `crates/` so no older flow sees them:
    `longline/` one 1 MiB line, `longline-huge/` one 12 MiB line, `public/vite/` public MIT text for live flows."""
    for directory, size in (('longline', 1 << 20), ('longline-huge', 12 << 20)):
        target = workspace / directory / ('bundle.min.js' if directory == 'longline' else 'huge.min.js')
        if not target.exists() or target.stat().st_size != size + 1:
            target.parent.mkdir(exist_ok=True)
            target.write_text(minified_line(size) + '\n')
    vite = workspace / 'public/vite'
    if not vite.exists():
        shutil.copytree(Path(__file__).with_name('fixtures') / 'public-vite', vite)
    return workspace


def stub_server(work):
    """Tool-health's loopback GitHub, the registry proxy and OSV. Returns (process, {url, proxyUrl, caFile}),
    or (None, reason) when it cannot start."""
    url_file = Path(work) / 'stub.url'
    env = {**os.environ, 'STUB_TOKENS': 'profile-fake-token', 'PROF_TMP': str(work)}
    process = subprocess.Popen(['node', str(Path(__file__).with_name('stub.mjs')), str(url_file)], env=env,
                               stdout=subprocess.DEVNULL, stderr=subprocess.PIPE, text=True)
    for _ in range(300):
        if url_file.exists() and url_file.read_text():
            return process, json.loads(url_file.read_text())
        if process.poll() is not None:
            return None, f'stub did not start: {process.stderr.read()[:300]}'
        time.sleep(0.1)
    process.terminate()
    return None, 'stub did not start in 30 s'


def stub_env(stub):
    """GitHub and the classification provider at the stub; HTTPS registries and OSV through its proxy."""
    url = stub['url']
    return {'GITHUB_TOKEN': 'profile-fake-token', 'GITHUB_API_URL': f'{url}/api/v3', 'OCTOCODE_ALLOW_PRIVATE_REGISTRY': 'true',
            'OCTOCODE_CLASSIFICATION_API_HOST': url, 'OCTOCODE_CLASSIFICATION_API': 'profile-fake-token', 'OCTOCODE_SECURITY_CHECKS': 'true',
            'HTTPS_PROXY': stub['proxyUrl'], 'https_proxy': stub['proxyUrl'], 'SSL_CERT_FILE': stub['caFile'], 'SSL_CERT_DIR': '',
            'NO_PROXY': '127.0.0.1,localhost,::1', 'no_proxy': '127.0.0.1,localhost,::1'}


def live_env(kind):
    """The configured classification key from ~/.octocode/.env, for live flows. The value is never printed."""
    if kind != 'classification':
        raise SystemExit(f'unknown live kind {kind}')
    dotenv = Path.home() / '.octocode/.env'
    for line in dotenv.read_text().splitlines() if dotenv.exists() else ():
        match = re.match(r'\s*(?:export\s+)?OCTOCODE_CLASSIFICATION_API\s*=\s*[\'"]?([^\'"\s#]+)', line)
        if match:
            return {'OCTOCODE_CLASSIFICATION_API': match.group(1)}
    raise SystemExit('live flow: no OCTOCODE_CLASSIFICATION_API in ~/.octocode/.env')


def with_stub(query, stub):
    """The query with every '{stub}' replaced by the stub's base URL."""
    if not stub:
        return query
    return json.loads(json.dumps(query).replace('{stub}', stub['url']))
