"""Atomic, overlap-aware advisory leases and read-only write checks."""
import hashlib
import json
from pathlib import Path
from . import catalog
from .database import execute, query, transaction, read_transaction
from .paths import ancestors, keys_overlap, lease_key, resolve_path

OVERLAPS = """WITH hits(id) AS (
SELECT id FROM leases WHERE workspace=?1 AND pathKey=?2
UNION ALL SELECT id FROM leases WHERE workspace=?1 AND kind='tree' AND pathKey IN (SELECT value FROM json_each(?3))
UNION ALL SELECT id FROM leases WHERE ?4='tree' AND workspace=?1 AND pathKey>?5 AND pathKey<?6)
SELECT l.id,l.path,l.kind,l.owner,l.acquiredAt,l.refreshedAt,l.expiresAt,l.reasoning,s.name AS ownerName,s.vendor AS ownerVendor,s.expiresAt AS ownerExpiresAt
FROM hits h JOIN leases l ON l.id=h.id JOIN sessions s ON s.id=l.owner
WHERE l.expiresAt>?7 AND s.expiresAt>?7 AND l.owner<>?8 ORDER BY l.id LIMIT ?9"""


class Target:
    def __init__(self, path, kind):
        self.path, self.kind, self.key = str(path), kind, lease_key(str(path))

    def key_args(self):
        return [self.key, json.dumps(ancestors(self.key)), self.kind, self.key + '/', self.key + '0']


class LeaseMixin:
    def lease_target(self, request):
        path = resolve_path(Path(self.workspace), catalog.text(request, 'path'))
        try:
            path.relative_to(self.workspace)
        except ValueError:
            raise ValueError('Path escapes workspace')
        return Target(path, request.get('kind', 'file'))

    def relative(self, path):
        try:
            return str(Path(path).relative_to(self.workspace)).replace('\\', '/')
        except ValueError:
            return str(path)

    def overlapping(self, db, target, at, exclude_owner, limit):
        return query(db, OVERLAPS, [self.workspace] + target.key_args() + [at, exclude_owner, limit])

    def check_paths(self, session, input):
        from .store import now
        targets = [self.lease_target(r) for r in input['paths']]
        with read_transaction(self.db):
            self.known(session)
            at, seen, conflicts, truncated = now(), set(), [], False
            for target in targets:
                for lease in self.overlapping(self.db, target, at, session, 101):
                    if lease['id'] in seen:
                        continue
                    seen.add(lease['id'])
                    if len(conflicts) == 100:
                        truncated = True
                        break
                    row = {k: lease[k] for k in ('id', 'path', 'kind', 'owner', 'acquiredAt', 'refreshedAt', 'expiresAt', 'reasoning')}
                    row['path'] = self.relative(row['path'])
                    conflicts.append(row)
                if truncated:
                    break
            result = dict(ok=not conflicts, conflicts=conflicts)
            if truncated:
                result['truncated'] = True
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
            for target in targets:
                rows = self.overlapping(self.db, target, at, '', 1)
                if not rows:
                    continue
                row = rows[0]
                owner, path = row['owner'], self.relative(row['path'])
                held = [r['id'] for r in query(self.db, 'SELECT id FROM leases WHERE owner=? AND workspace=? AND expiresAt>? ORDER BY id', (session, self.workspace, at))]
                conflict = {k: row[k] for k in ('id', 'kind', 'acquiredAt', 'refreshedAt', 'expiresAt', 'reasoning')}
                conflict['path'] = path
                result = dict(ok=False, conflict=conflict, owner=dict(id=owner, name=row['ownerName'], vendor=row['ownerVendor'], expiresAt=row['ownerExpiresAt']), retryAfterMs=max(0, min(row['expiresAt'], row['ownerExpiresAt'])-at), heldLeaseIds=held)
                if owner == session:
                    result['guidance'] = 'You hold an overlapping lease: reuse it, or unlock it and lock the complete set.'
                else:
                    result['next'] = {'command': 'send_message', 'input': {'to': owner, 'body': 'Need access overlapping your lease {} on {}; can you release it or agree a handoff?'.format(row['id'], path), 'reasoning': reasoning, 'key': 'lease-request-{}-{}'.format(row['id'], hashlib.sha256(reasoning.encode()).hexdigest()[:16])}}
                    result['guidance'] = 'Nothing acquired. Unlock held leases; send next once if coordination is needed, or wait/do independent work. Retry after handoff or expiry; never edit without ok:true or poll in a loop.'
                return result
            expires, leases = at + duration, []
            for target in targets:
                execute(self.db, 'INSERT INTO leases(workspace,path,kind,owner,acquiredAt,refreshedAt,expiresAt,reasoning,pathKey) VALUES(?,?,?,?,?,?,?,?,?)', (self.workspace, target.path, target.kind, session, at, at, expires, reasoning, target.key))
                leases.append(dict(id=self.db.execute('SELECT last_insert_rowid()').fetchone()[0], path=self.relative(target.path), kind=target.kind, owner=session, reasoning=reasoning, acquiredAt=at, refreshedAt=at))
            if multiple:
                return dict(ok=True, expiresAt=expires, leases=leases)
            leases[0]['expiresAt'] = expires
            return dict(ok=True, lease=leases[0])

    def lease_transition(self, session, input, renew):
        from .store import now
        with transaction(self.db):
            self.known(session)
            at = now()
            if renew:
                expires = at + catalog.ttl(input, 60000)
                count = execute(self.db, 'UPDATE leases SET expiresAt=?,refreshedAt=? WHERE id=? AND owner=? AND expiresAt>?', (expires, at, input['leaseId'], session, at))
                return dict(renewed=True, refreshedAt=at, expiresAt=expires) if count == 1 else dict(renewed=False, guidance='No live owned lease covers this ID. Stop writing; acquire a fresh lock before resuming.')
            return dict(released=execute(self.db, 'DELETE FROM leases WHERE id=? AND owner=? AND expiresAt>?', (input['leaseId'], session, at)) == 1)

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
