"""Explicit, history-preserving maintenance; expiry never authorizes deletion."""
import stat
from pathlib import Path
from . import catalog, database


def storage(db, path):
    path = Path(path)
    size = db.execute('PRAGMA page_size').fetchone()[0]
    count = db.execute('PRAGMA page_count').fetchone()[0]
    free = db.execute('PRAGMA freelist_count').fetchone()[0]
    wal = Path(str(path) + '-wal')
    try:
        wal_bytes = wal.stat().st_size
    except FileNotFoundError:
        wal_bytes = 0
    return dict(pageSize=size, pageCount=count, freePages=free, logicalBytes=size*count, reusableBytes=size*free, mainFileBytes=path.stat().st_size, walBytes=wal_bytes)


def report(path, input):
    from .store import now
    catalog.command('db retention', input)
    at = now()
    before = int(input.get('before', max(0, at - 30*86400000)))
    if before > at:
        raise ValueError('before must be no later than now')
    after, limit = int(input.get('afterId', 0)), int(input.get('limit', 100))
    db = database.open(Path(path), True, False)
    try:
        with database.read_transaction(db):
            rows = database.query(db, "SELECT m.id,m.expiresAt,length(CAST(m.body AS BLOB)) AS bodyBytes, EXISTS(SELECT 1 FROM deliveries d WHERE d.message=m.id AND d.acknowledgedAt IS NULL) AS unacknowledged, EXISTS(SELECT 1 FROM dispatches x WHERE x.message=m.id AND x.state IN ('staged','uncertain')) AS unresolved FROM messages m WHERE m.id>? ORDER BY m.id LIMIT ?", (after, limit+1))
            more, rows = len(rows) > limit, rows[:limit]
            settled = [r for r in rows if r['expiresAt'] <= before and not r['unacknowledged'] and not r['unresolved']]
            result = dict(scope='all-workspaces', readOnly=True, deletionSupported=False, before=before, cutoffField='messages.expiresAt', observedAt=at, storage=storage(db, path), page=dict(afterId=after, limit=limit, count=len(rows), hasMore=more, expiredSettledMessages=len(settled), expiredSettledBodyBytes=sum(r['bodyBytes'] for r in settled), unacknowledgedMessages=sum(bool(r['unacknowledged']) for r in rows), unresolvedDispatchMessages=sum(bool(r['unresolved']) for r in rows)), retainedBecause=['Audit is append-only and does not duplicate message bodies.', 'Message keys preserve idempotency; replyTo and delivery records preserve correlation and visibility.', 'Expiry and acknowledgement do not authorize deletion; pending and uncertain attempts remain recoverable.'], maintenance={'compact': 'db compact {} reclaims reusable SQLite pages without deleting history.', 'archive': 'db export creates a verified full snapshot; preserve workspace documents separately.'})
            if more:
                result['next'] = {'command': 'db retention', 'input': dict(before=before, afterId=rows[-1]['id'], limit=limit)}
            return result
    finally:
        db.close()


def compact(path):
    path = Path(path)
    initial = path.lstat()
    if not stat.S_ISREG(initial.st_mode):
        raise ValueError('Compaction requires a regular database file, not a symlink')
    db = database.open(path, False, False)
    try:
        db.execute('PRAGMA busy_timeout=2000')
        fingerprint, before = database.fingerprint(db), storage(db, path)
        try:
            db.execute('VACUUM')
        except Exception as error:
            raise ValueError('Compaction failed; no automatic retry was attempted: {}'.format(error)) from error
        if database.fingerprint(db) != fingerprint:
            raise ValueError('Compaction completed but schema verification failed')
        if database.query(db, 'PRAGMA integrity_check') != [{'integrity_check': 'ok'}] or database.query(db, 'PRAGMA foreign_key_check'):
            raise ValueError('Compaction completed but database integrity verification failed')
        checkpoint = database.query(db, 'PRAGMA wal_checkpoint(PASSIVE)')
        current = path.lstat()
        if not stat.S_ISREG(current.st_mode) or (initial.st_dev, initial.st_ino) != (current.st_dev, current.st_ino):
            raise ValueError('Database path was replaced during compaction')
        after = storage(db, path)
        return dict(scope='all-workspaces', compacted=True, deletedRecords=0, schemaVersion=database.schema()['schemaVersion'], schemaSha256=fingerprint, integrity='ok', reclaimedBytes=max(0, before['mainFileBytes']-after['mainFileBytes']), logicalReclaimedBytes=max(0, before['logicalBytes']-after['logicalBytes']), before=before, after=after, checkpoint=checkpoint, measurement='reclaimedBytes is the nonnegative main-file size difference; WAL allocation and concurrent writers can mask savings. No WAL truncation is forced.')
    finally:
        db.close()
