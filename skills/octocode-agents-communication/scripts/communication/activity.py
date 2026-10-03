"""Bounded Git observations, never an attribution or complete command audit."""
import hashlib
import json
import os
from pathlib import Path
import re
import signal
import subprocess
import sys
import tempfile
import time

MAX_BYTES = 8 * 1024 * 1024


def encoded(value):
    return json.dumps(value, ensure_ascii=False, separators=(',', ':')).encode()


class Git:
    def __init__(self):
        self.deadline = time.monotonic() + 5

    def execute(self, command, cwd=None, env=None, input=None):
        with tempfile.TemporaryFile() as out, tempfile.TemporaryFile() as err, tempfile.TemporaryFile() as source:
            if input is not None:
                if len(input) > MAX_BYTES:
                    raise ValueError('Git activity exceeds 8 MiB')
                source.write(input)
                source.seek(0)
            try:
                child = subprocess.Popen(command, cwd=cwd, env=env, stdin=source if input is not None else subprocess.DEVNULL, stdout=out, stderr=err, start_new_session=os.name != 'nt')
            except FileNotFoundError as error:
                raise ValueError('Git is required for activity') from error
            try:
                while True:
                    if os.fstat(out.fileno()).st_size > MAX_BYTES or os.fstat(err.fileno()).st_size > MAX_BYTES:
                        raise ValueError('Git activity exceeds 8 MiB; use a smaller workspace for files or lower scanLimit for history')
                    status = child.poll()
                    if status is not None:
                        out.seek(0)
                        result = out.read(MAX_BYTES + 1).decode('utf-8')
                        if len(result.encode()) > MAX_BYTES:
                            raise ValueError('Git activity exceeds 8 MiB')
                        if status:
                            err.seek(0)
                            raise ValueError('Git activity failed: ' + err.read(MAX_BYTES).decode('utf-8')[:2000])
                        return result
                    if time.monotonic() >= self.deadline:
                        raise ValueError('Git activity exceeded 5 seconds; use a smaller workspace for files or lower scanLimit for history')
                    time.sleep(.005)
            finally:
                if child.poll() is None:
                    if os.name != 'nt':
                        try:
                            os.killpg(child.pid, signal.SIGKILL)
                        except ProcessLookupError:
                            pass
                    child.kill()
                    child.wait()

    def read(self, cwd, args):
        env = os.environ.copy()
        for key in ('GIT_DIR', 'GIT_WORK_TREE', 'GIT_INDEX_FILE', 'GIT_COMMON_DIR', 'GIT_OBJECT_DIRECTORY', 'GIT_ALTERNATE_OBJECT_DIRECTORIES', 'GIT_CONFIG_COUNT', 'GIT_CONFIG_PARAMETERS'):
            env.pop(key, None)
        env['GIT_NO_LAZY_FETCH'] = '1'
        command = ['git', '--no-pager', '--no-optional-locks', '-c', 'core.fsmonitor=false', '-c', 'core.untrackedCache=false', '-c', 'log.showSignature=false']
        return self.execute(command + args, cwd, env)


class Filter:
    def __init__(self, input, now):
        prefix = input.get('path', '')
        if prefix.startswith('/') or any(p in ('.', '..') for p in prefix.split('/')) or '\\' in prefix:
            raise ValueError('path must be a workspace-relative slash-separated prefix without . or ..')
        self.prefix = prefix.rstrip('/')
        self.pattern = input.get('pathRegex')
        self.allowed = None
        if self.pattern is not None:
            # Preserve the old linear matcher syntax boundary. Matching itself is isolated
            # under the same request deadline because Python's engine can backtrack.
            if re.search(r'\(\?(?:[=!]|<[=!]|P=|\()|\\[1-9]|\\[gk][<{]', self.pattern):
                raise ValueError('Invalid pathRegex: look-around and backreferences are not supported')
            try:
                re.compile(self.pattern)
            except re.error as error:
                raise ValueError('Invalid pathRegex: ' + str(error)) from error
        self.since = input.get('sinceMs', max(0, now - input['withinMs']) if 'withinMs' in input else None)
        self.until = input.get('untilMs')
        if self.since is not None and self.until is not None and self.since > self.until:
            raise ValueError('sinceMs must not exceed untilMs')

    def prepare(self, paths, git):
        if self.pattern is None:
            return
        paths = sorted(set(paths))
        script = 'import json,re,sys\np,paths=json.load(sys.stdin)\nr=re.compile(p)\njson.dump([s for s in paths if r.search(s)],sys.stdout,ensure_ascii=False,separators=(",",":"))'
        self.allowed = set(json.loads(git.execute([sys.executable, '-c', script], input=encoded([self.pattern, paths]))))

    def path(self, path):
        return (not self.prefix or path == self.prefix or path.startswith(self.prefix + '/')) and (self.pattern is None or path in self.allowed)

    def time(self, at):
        if at is None:
            return self.since is None and self.until is None
        return (self.since is None or at >= self.since) and (self.until is None or at <= self.until)


def relative(path, scope):
    return path if not scope else (path[len(scope) + 1:] if path.startswith(scope + '/') else None)


def modified(workspace, path):
    full = workspace / path
    try:
        full.parent.resolve(strict=True).relative_to(workspace)
        at = full.lstat().st_mtime_ns // 1000000
        return at if at >= 0 else None
    except (OSError, ValueError):
        return None


def change_state(code):
    return {' ': 'unchanged', 'M': 'modified', 'A': 'added', 'D': 'deleted', 'R': 'renamed', 'C': 'copied', 'T': 'type-changed', 'U': 'unmerged', '?': 'untracked', '!': 'ignored'}.get(code, 'unknown')


def files(git, workspace, scope, filter):
    raw = git.read(workspace, ['status', '--porcelain=v1', '-z', '--untracked-files=all', '--', '.'])
    records = iter(raw.split('\0')[:-1])
    candidates = []
    for record in records:
        if len(record) < 4 or record[2] != ' ':
            raise ValueError('Invalid Git status record')
        status = record[:2]
        original = next(records, None) if 'R' in status or 'C' in status else None
        if ('R' in status or 'C' in status) and original is None:
            raise ValueError('Missing rename source')
        path = relative(record[3:], scope)
        if path is not None:
            candidates.append((status, path, relative(original, scope) if original is not None else None))
    filter.prepare([p for _, path, original in candidates for p in (path, original) if p is not None], git)
    rows, unknown = [], 0
    for status, path, original in candidates:
        if not filter.path(path) and not (original is not None and filter.path(original)):
            continue
        at = modified(workspace, path)
        if at is None:
            unknown += 1
        if filter.time(at):
            index, worktree = change_state(status[0]), change_state(status[1])
            changes = list(dict.fromkeys(s for s in (index, worktree) if s != 'unchanged'))
            rows.append(dict(path=path, status=status, changes=changes, indexState=index, worktreeState=worktree, previousPath=original, modifiedAt=at))
    rows.sort(key=lambda row: (-(row['modifiedAt'] if row['modifiedAt'] is not None else -1), row['path']))
    return rows, unknown, len(candidates)


def epoch(value):
    at = int(value) * 1000
    if not -(2**63) <= at < 2**63:
        raise ValueError('Git timestamp overflow')
    return at


def commits(raw, scope, filter, git):
    fields = raw.split('\0')
    if fields[-1] == '':
        fields.pop()
    candidates, i = [], 0
    while i < len(fields):
        if i + 3 >= len(fields):
            raise ValueError('Missing commit metadata')
        hash, at, subject = fields[i+1], epoch(fields[i+2]), fields[i+3]
        i += 4
        paths, first = [], True
        while i < len(fields) and fields[i]:
            path = fields[i]
            if first and path.startswith('\n'):
                path = path[1:]
            first = False
            path = relative(path, scope)
            if path is not None:
                paths.append(path)
            i += 1
        candidates.append((hash, at, subject, paths))
    filter.prepare([path for _, _, _, paths in candidates for path in paths], git)
    rows = []
    for hash, at, subject, paths in candidates:
        paths = [path for path in paths if filter.path(path)]
        matches = bool(paths) or (not scope and not filter.prefix and filter.pattern is None)
        rows.append(dict(hash=hash, at=at, subject=subject[:256], subjectTruncated=len(subject)>256, paths=paths[:20], matchedPathCount=len(paths), pathsTruncated=len(paths)>20, matches=matches and filter.time(at)))
    return rows


def reflog(raw, filter):
    fields = raw.split('\0')
    if fields[-1] == '':
        fields.pop()
    if len(fields) % 3:
        raise ValueError('Invalid reflog output')
    rows = []
    for i in range(0, len(fields), 3):
        hash, reference, action = fields[i:i+3]
        if '@{' not in reference or not reference.endswith('}'):
            raise ValueError('Missing reflog event time')
        at = epoch(reference.rsplit('@{', 1)[1][:-1])
        rows.append(dict(hash=hash, at=at, action=action[:256], actionTruncated=len(action)>256, matches=filter.time(at)))
    return rows


def read(workspace, input):
    from .store import now, PAGE_BYTES, strip_nulls
    observed = now()
    filter = Filter(input, observed)
    view = input.get('view', 'files')
    if view == 'reflog' and ('path' in input or 'pathRegex' in input):
        raise ValueError('Reflog events do not identify changed paths; use view:commits for path filters')
    workspace = Path(workspace).resolve(strict=True)
    git = Git()
    root = Path(git.read(workspace, ['rev-parse', '--show-toplevel']).removesuffix('\n')).resolve(strict=True)
    scope = workspace.relative_to(root).as_posix()
    if scope == '.':
        scope = ''
    cap, unknown = input.get('scanLimit', 200), 0
    if view == 'files':
        rows, unknown, scanned = files(git, workspace, scope, filter)
        truncated = False
    else:
        has_head = bool(git.read(workspace, ['rev-parse', '--revs-only', 'HEAD']).strip())
        if not has_head:
            raw = ''
        elif view == 'commits':
            raw = git.read(workspace, ['log', '-' + str(cap + 1), '--date-order', '-z', '--name-only', '--format=%x00%H%x00%ct%x00%s', '--no-renames', '--no-ext-diff', '--diff-merges=first-parent', '--full-history'])
        else:
            raw = git.read(workspace, ['reflog', 'show', '-' + str(cap + 1), '-z', '--date=unix', '--format=%H%x00%gD%x00%gs', 'HEAD'])
        rows = commits(raw, scope, filter, git) if view == 'commits' else reflog(raw, filter)
        truncated, rows = len(rows) > cap, rows[:cap]
        scanned = len(rows)
        rows = [row for row in rows if row.pop('matches')]
        rows.sort(key=lambda row: -row['at'])
    digest = hashlib.sha256(encoded(rows)).hexdigest()
    if 'snapshot' in input and input['snapshot'] != digest:
        raise ValueError('Activity changed during pagination; restart without after/snapshot')
    after = input.get('after', 0)
    if after > len(rows):
        raise ValueError('Activity cursor exceeds result count')
    end, size, count = after, 0, input.get('limit', 20)
    while end < len(rows) and end - after < count:
        length = len(encoded(rows[end]))
        if end > after and size + length > PAGE_BYTES:
            break
        size += length
        end += 1
    next = None
    if end < len(rows):
        continuation = dict(input)
        continuation.pop('withinMs', None)
        if filter.since is not None:
            continuation['sinceMs'] = filter.since
        continuation.update(after=end, snapshot=digest)
        next = {'command': 'activity', 'input': continuation}
    unknown_query = None
    if unknown and any(key in input for key in ('withinMs', 'sinceMs', 'untilMs')):
        unknown_query = {key: value for key, value in input.items() if key not in ('withinMs', 'sinceMs', 'untilMs', 'after', 'snapshot')}
    basis = {'files': 'Current filesystem mtime; not an edit/staging timestamp. Deletions may have unknown time and are excluded by time filters.', 'commits': 'Committer time on current HEAD history; not command execution time.'}.get(view, 'Local HEAD reflog time; reference updates only, not complete Git command history.')
    result = dict(view=view, workspace=str(workspace), observedAt=observed, items=rows[after:end], totalMatched=len(rows), next=next, coverage=dict(scanned=scanned, scanLimit=None if view == 'files' else cap, truncated=truncated, limitReason='History scan limit reached; increase scanLimit (max 2000). This is not complete history.' if truncated else None, unknownFileTimes=unknown, unknownTimeQuery=unknown_query), timeBasis=basis, attribution='Observed Git/filesystem state does not identify the acting agent.')
    if size > PAGE_BYTES:
        result['budget'] = dict(targetBytes=PAGE_BYTES, reason='Single oversized row returned intact to preserve evidence and cursor progress')
    stripped = strip_nulls(result)
    return result if stripped is None else stripped
