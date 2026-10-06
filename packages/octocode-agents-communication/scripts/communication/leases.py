"""Atomic, overlap-aware advisory leases and read-only write checks."""
import json
from pathlib import Path
from . import catalog
from .database import execute, query, transaction, read_transaction
from .paths import ancestors, keys_overlap, lease_key, resolve_path
from .workspace import scope_workspaces

OVERLAPS = """WITH hits(id) AS (
SELECT id FROM leases WHERE workspace IN (SELECT value FROM json_each(?1)) AND pathKey=?2
UNION ALL SELECT id FROM leases WHERE workspace IN (SELECT value FROM json_each(?1)) AND kind='tree' AND pathKey IN (SELECT value FROM json_each(?3))
UNION ALL SELECT id FROM leases WHERE ?4='tree' AND workspace IN (SELECT value FROM json_each(?1)) AND pathKey>?5 AND pathKey<?6)
SELECT l.id,l.path,l.kind,l.owner,l.acquiredAt,l.refreshedAt,l.expiresAt,l.reasoning,s.name AS ownerName,s.vendor AS ownerVendor,s.expiresAt AS ownerExpiresAt
FROM hits h JOIN leases l ON l.id=h.id JOIN sessions s ON s.id=l.owner
WHERE l.expiresAt>?7 AND s.expiresAt>?7 AND l.owner<>?8 AND l.id>?10 ORDER BY l.id LIMIT ?9"""


WAIT_TTL = 600000
QUEUED = 'Queued; owners were notified. Keep the request that needs this lease pending and end your turn. A "Lease granted" message wakes you with the lease ID.'


def notice(db, sender, target, body, key, wake, at):
    """Runtime-authored informational mail; one key per wait keeps it idempotent."""
    message = db.execute('INSERT INTO messages(sender,target,body,key,expiresAt,reasoning,wake,ttlMs,replyRequired) VALUES(?,?,?,?,?,?,?,3600000,0)', (sender, target, body, key, at + 3600000, 'Lease queue notice', wake)).lastrowid
    execute(db, 'INSERT INTO deliveries(message,recipient) VALUES(?,?)', (message, target))


class Target:
    def __init__(self, path, kind):
        self.path, self.kind, self.key = str(path), kind, lease_key(str(path))

    def key_args(self):
        return [self.key, json.dumps(ancestors(self.key)), self.kind, self.key + '/', self.key + '0']


class LeaseMixin:
    def locks(self, session, input):
        from .store import now, page
        self.known(session, False)
        # A path query is a conflict check, so it spans the repository scope.
        workspaces = self.scope_workspaces_json() if 'path' in input else json.dumps([self.workspace])
        args = [workspaces, now(), int(input.get('after', '0'))]
        conditions = ['l.workspace IN (SELECT value FROM json_each(?1))', 'l.id>?3']
        active = input.get('presence', 'active')
        if active != 'all':
            conditions.append('(l.expiresAt>?2 AND s.expiresAt>?2)=?')
            args.append(active == 'active')
        if 'owner' in input:
            conditions.append('l.owner=?')
            args.append(input['owner'])
        if 'kind' in input and 'path' not in input:
            raise ValueError('kind requires a path conflict query')
        if 'path' in input:
            target = self.lease_target(input)
            conditions.append("(l.pathKey=? OR (l.kind='tree' AND l.pathKey IN (SELECT value FROM json_each(?))) OR (?='tree' AND l.pathKey>? AND l.pathKey<?))")
            args.extend(target.key_args())
        rows = query(self.db, 'SELECT l.*,l.expiresAt>?2 AND s.expiresAt>?2 AS active FROM leases l JOIN sessions s ON s.id=l.owner WHERE '
                     + ' AND '.join(conditions) + ' ORDER BY l.id LIMIT 101', args)
        for row in rows:
            row.pop('pathKey', None)
        return page(rows, True, 'locks', input)

    def lease_target(self, request):
        path = resolve_path(Path(self.workspace), catalog.text(request, 'path'))
        try:
            path.relative_to(self.workspace)
        except ValueError:
            raise ValueError('Path escapes workspace')
        return Target(path, request.get('kind', 'tree' if path.is_dir() else 'file'))

    def relative(self, path, workspace=None):
        try:
            return str(Path(path).relative_to(workspace or self.workspace)).replace('\\', '/')
        except ValueError:
            return str(path)

    def overlapping(self, db, target, at, exclude_owner, limit, after=0, workspaces=None):
        # Lease keys are absolute, so every workspace bound to this repository
        # (nested directories and linked worktrees) shares one conflict space.
        return query(db, OVERLAPS, [workspaces or self.scope_workspaces_json()] + target.key_args() + [at, exclude_owner, limit, after])

    def scope_workspaces_json(self, workspace=None):
        if workspace is None:
            return json.dumps(scope_workspaces(self.db, self.coordination_scope, self.workspace))
        scope = query(self.db, 'SELECT coordinationScope FROM workspaces WHERE workspace=?', (workspace,))
        return json.dumps(scope_workspaces(self.db, scope[0]['coordinationScope'], workspace) if scope else [workspace])

    def check_paths(self, session, input):
        from .store import now, page, PAGE_BYTES
        targets = [self.lease_target(r) for r in input['paths']]
        with read_transaction(self.db):
            self.known(session)
            at, candidates, blocked = now(), {}, False
            after = input.get('after', 0)
            for target in targets:
                rows = self.overlapping(self.db, target, at, session, 101, after)
                blocked = blocked or bool(rows)
                if after and not blocked:
                    blocked = bool(self.overlapping(self.db, target, at, session, 1))
                candidates.update((row['id'], row) for row in rows)
                # Each target's first 101 rows contain the union's first 101.
                candidates = {key: candidates[key] for key in sorted(candidates)[:101]}
            conflicts = []
            for lease in candidates.values():
                row = {k: lease[k] for k in ('id', 'path', 'kind', 'owner', 'acquiredAt', 'refreshedAt', 'expiresAt', 'reasoning')}
                row['path'] = self.relative(row['path'])
                conflicts.append(row)
            metadata_bytes = len(json.dumps(input, ensure_ascii=False, separators=(',', ':')).encode()) + 100
            result = page(conflicts, False, 'check_paths', input, metadata_bytes)
            result.update(ok=not blocked, conflicts=result.pop('items'), checkedAt=at)
            if len(json.dumps(result, ensure_ascii=False, separators=(',', ':')).encode()) > PAGE_BYTES:
                result['budget'] = dict(targetBytes=PAGE_BYTES, reason='One conflict and its full continuation query exceed the page target; returned intact to preserve evidence and cursor progress')
            else:
                result.pop('budget', None)
            return result

    def lock(self, session, input, multiple=False):
        from .store import now
        reasoning = catalog.text(input, 'reasoning')
        targets = []
        for request in input['paths'] if multiple else [input]:
            target = self.lease_target(request)
            if any(keys_overlap(t.key, t.kind, target.key, target.kind) for t in targets):
                raise ValueError('Requested paths overlap; use one covering tree lease or distinct paths')
            targets.append(target)
        targets.sort(key=lambda t: (t.path, t.kind))
        duration = catalog.ttl(input, 60000)
        with transaction(self.db):
            self.known(session)
            at = now()
            # Hand expired or released paths to earlier waiters before a newcomer competes.
            self.grant_waits(at)
            for target in targets:
                rows = self.overlapping(self.db, target, at, '', 1)
                if rows:
                    return self.lock_conflict(session, input, targets, rows[0], reasoning, duration, at)
            leases = self.insert_leases(self.workspace, session, targets, reasoning, at, at + duration)
            self.drop_satisfied_wait(session, targets)
            if multiple:
                return dict(ok=True, expiresAt=at + duration, leases=leases)
            leases[0]['expiresAt'] = at + duration
            return dict(ok=True, lease=leases[0])

    def drop_satisfied_wait(self, session, acquired):
        """A direct grant of a queued path ends that wait; unrelated locks keep it."""
        for wait in query(self.db, 'SELECT id,targets FROM lease_waits WHERE owner=?', (session,)):
            if any(keys_overlap(lease_key(t['path']), t['kind'], a.key, a.kind) for t in json.loads(wait['targets']) for a in acquired):
                execute(self.db, 'DELETE FROM lease_waits WHERE id=?', (wait['id'],))

    def insert_leases(self, workspace, owner, targets, reasoning, at, expires):
        leases = []
        for target in targets:
            lease = self.db.execute('INSERT INTO leases(workspace,path,kind,owner,acquiredAt,refreshedAt,expiresAt,reasoning,pathKey) VALUES(?,?,?,?,?,?,?,?,?)', (workspace, target.path, target.kind, owner, at, at, expires, reasoning, target.key)).lastrowid
            leases.append(dict(id=lease, path=self.relative(target.path, workspace), kind=target.kind, owner=owner, reasoning=reasoning, acquiredAt=at, refreshedAt=at))
        return leases

    def lock_conflict(self, session, input, targets, row, reasoning, duration, at):
        owner, path = row['owner'], self.relative(row['path'])
        held = [r['id'] for r in query(self.db, 'SELECT id FROM leases WHERE owner=? AND workspace=? AND expiresAt>? ORDER BY id', (session, self.workspace, at))]
        conflict = {k: row[k] for k in ('id', 'kind', 'acquiredAt', 'refreshedAt', 'expiresAt', 'reasoning')}
        conflict['path'] = path
        result = dict(ok=False, conflict=conflict, owner=dict(id=owner, name=row['ownerName'], vendor=row['ownerVendor'], expiresAt=row['ownerExpiresAt']), retryAfterMs=max(0, min(row['expiresAt'], row['ownerExpiresAt'])-at), heldLeaseIds=held)
        if owner == session:
            result['guidance'] = 'You hold an overlapping lease: reuse it, or unlock it and lock the complete set.'
            return result
        if not input.get('wait'):
            result['next'] = {'command': 'lock_many' if 'paths' in input else 'lock', 'input': dict(input, wait=True)}
            result['guidance'] = 'Nothing acquired. Run next to queue: the runtime grants the lease when it frees and wakes you with a message. Keep the request that needs it pending.'
            return result
        queued = json.dumps([{'path': t.path, 'kind': t.kind} for t in targets])
        same = query(self.db, 'SELECT id,expiresAt FROM lease_waits WHERE owner=? AND targets=? AND reasoning=? AND expiresAt>?', (session, queued, reasoning, at))
        if same:
            # A retried wait keeps its place and notifies nobody twice.
            result.update(queued=True, wait=dict(id=same[0]['id'], expiresAt=same[0]['expiresAt']), guidance=QUEUED)
            return result
        blockers = self.blocking_owners(targets, self.scope_workspaces_json(), at, session)
        if self.waits_on(blockers, session, at):
            result['guidance'] = 'Not queued: waiting would deadlock, because an owner you wait for is waiting for a lease you hold. Release your held leases, then queue.'
            return result
        execute(self.db, 'DELETE FROM lease_waits WHERE owner=?', (session,))
        wait = self.db.execute('INSERT INTO lease_waits(workspace,owner,targets,reasoning,leaseTtlMs,createdAt,expiresAt) VALUES(?,?,?,?,?,?,?)', (self.workspace, session, queued, reasoning, duration, at, at + WAIT_TTL)).lastrowid
        paths = ', '.join(self.relative(t.path) for t in targets)
        name = query(self.db, 'SELECT name FROM sessions WHERE id=?', (session,))[0]['name']
        for blocker in sorted(blockers):
            notice(self.db, session, blocker, '{} is queued for {} ({}). The runtime hands the lease over when you unlock it; no reply needed.'.format(name, paths, reasoning), 'lease-wait:{}:{}'.format(wait, blocker), 'passive', at)
        result.update(queued=True, wait=dict(id=wait, expiresAt=at + WAIT_TTL), guidance=QUEUED)
        return result

    def blocking_owners(self, targets, workspaces, at, exclude):
        return {row['owner'] for target in targets for row in self.overlapping(self.db, target, at, exclude, 100, workspaces=workspaces)}

    def waits_on(self, owners, session, at):
        """True when some owner transitively waits for a lease that session holds."""
        frontier, seen = list(owners), set()
        while frontier:
            owner = frontier.pop()
            if owner == session:
                return True
            if owner in seen:
                continue
            seen.add(owner)
            for wait in query(self.db, 'SELECT workspace,targets FROM lease_waits WHERE owner=? AND expiresAt>?', (owner, at)):
                targets = [Target(t['path'], t['kind']) for t in json.loads(wait['targets'])]
                frontier.extend(self.blocking_owners(targets, self.scope_workspaces_json(wait['workspace']), at, owner))
        return False

    def grant_waits(self, at):
        """Grant each live wait, oldest first, whose every target is free now; wake its owner."""
        execute(self.db, 'DELETE FROM lease_waits WHERE expiresAt<=? OR owner IN (SELECT id FROM sessions WHERE expiresAt<=?)', (at, at))
        for wait in query(self.db, 'SELECT * FROM lease_waits ORDER BY id'):
            targets = [Target(t['path'], t['kind']) for t in json.loads(wait['targets'])]
            if self.blocking_owners(targets, self.scope_workspaces_json(wait['workspace']), at, wait['owner']):
                continue
            execute(self.db, 'DELETE FROM lease_waits WHERE id=?', (wait['id'],))
            leases = self.insert_leases(wait['workspace'], wait['owner'], targets, wait['reasoning'], at, at + wait['leaseTtlMs'])
            granted = ', '.join('{} (lease {})'.format(lease['path'], lease['id']) for lease in leases)
            notice(self.db, wait['owner'], wait['owner'], 'Lease granted: {}, expires in {}s. Continue the work you queued it for; unlock when done.'.format(granted, wait['leaseTtlMs'] // 1000), 'lease-wait:{}'.format(wait['id']), 'action', at)

    def lease_transition(self, session, input, renew):
        from .store import now
        with transaction(self.db):
            self.known(session)
            at = now()
            if renew:
                expires = at + catalog.ttl(input, 60000)
                count = execute(self.db, 'UPDATE leases SET expiresAt=?,refreshedAt=? WHERE id=? AND owner=? AND expiresAt>?', (expires, at, input['leaseId'], session, at))
                return dict(renewed=True, refreshedAt=at, expiresAt=expires) if count == 1 else dict(renewed=False, guidance='No live owned lease covers this ID. Stop writing; acquire a fresh lock before resuming.')
            released = execute(self.db, 'DELETE FROM leases WHERE id=? AND owner=? AND expiresAt>?', (input['leaseId'], session, at)) == 1
            self.grant_waits(at)
            return dict(released=released)

    def check_write(self, session, input):
        from .store import now
        targets = []
        for request in input['paths']:
            target = self.lease_target(request)
            if target.kind != 'file' or Path(target.path).is_dir():
                raise ValueError('check_write accepts file targets only, not directories')
            targets.append(target.path)
        with read_transaction(self.db):
            identity = self.known(session)
            if 'vendorSession' in input:
                catalog.text(input, 'vendorSession')
                if identity.get('vendorSession') != input['vendorSession']:
                    raise ValueError('Native session does not match the bound DB identity')
            at, presence = now(), identity['expiresAt']
            if presence <= at:
                raise ValueError('Session expired before checking file coverage')
            checks = []
            for path in targets:
                rows = query(self.db, "SELECT id,expiresAt FROM leases WHERE owner=? AND workspace=? AND expiresAt>? AND lease_overlap(path,kind,?,'file') ORDER BY id LIMIT 1", (session, self.workspace, at, path))
                check = dict(path=path, covered=bool(rows))
                if rows:
                    check['lease'] = dict(id=rows[0]['id'], expiresAt=min(rows[0]['expiresAt'], presence))
                checks.append(check)
            return dict(ok=all(c['covered'] for c in checks), checkedAt=at, advisory=True, checks=checks)
