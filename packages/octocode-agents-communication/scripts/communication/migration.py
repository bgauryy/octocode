"""Explicit, atomic legacy migration; a verified snapshot precedes schema changes."""
import hashlib
import json
import os
import re
import sqlite3
import stat
import tempfile
import time
from pathlib import Path
from . import database
from .workspace import coordination_scope

V2_SHA256 = '167d9307e2f5d8f8a994f56d798123cbdadeba2ffb72e2da0c684d9b477eeb24'
V1_SHA256 = '911e1b36522d7737d343f94aecf7f9e22f0fca2dfa7ce1cd3ba7dbab6d97ab7b'
V3_SHA256 = '7663c97e0ad55d5ef2475924cec0a84f5b08fa613adf344c2b0dae1f6c582983'
V4_SHA256 = '298c10196beed2c91c690146daf5ded1e521cbb97ff2bab1b925935232ede049'
RENAMES = {'session.created': 'coordinate.in', 'session.updated': 'coordinate.update',
           'session.profile': 'coordinate.profile', 'message.created': 'message', 'document.created': 'document'}


def statements(sql):
    statement = ''
    for line in sql.splitlines(True):
        statement += line
        if sqlite3.complete_statement(statement):
            yield statement
            statement = ''
    if statement.strip():
        raise ValueError('Incomplete migration SQL')


def upgrade_v1(db):
    session_rows = database.query(db, 'SELECT * FROM sessions')
    sessions = {row['id']: row for row in session_rows}
    audits = database.query(db, 'SELECT * FROM audit ORDER BY id')
    documents = database.query(db, 'SELECT * FROM documents')
    messages = {str(row['id']): row for row in database.query(db, 'SELECT id,sender,target,topic FROM messages')}
    dispatches = {(str(row['message']), row['recipient']): row for row in database.query(db, 'SELECT message,recipient,token,transport FROM dispatches')}
    db.execute('DROP TABLE documents')
    db.execute('DROP TABLE audit')
    db.execute('DROP TABLE sessions')
    # Reuse identical operational tables and indexes; only the event owner changes.
    for statement in statements(database.SQL):
        if statement.lstrip().startswith('CREATE TRIGGER'):
            continue
        match = re.search(r'CREATE (?:VIRTUAL )?(?:TABLE|INDEX) "?(\w+)', statement)
        if match and db.execute('SELECT 1 FROM sqlite_schema WHERE name=?', (match[1],)).fetchone():
            continue
        db.execute(statement)
    db.executemany('INSERT INTO sessions(id,workspace,name,vendor,vendorSession,branch,expiresAt,task,status) VALUES(?,?,?,?,?,?,?,?,?)',
                   [(r['id'], r['workspace'], r['name'], r['vendor'], r.get('vendorSession'), None, r['expiresAt'], r['task'], r['status']) for r in session_rows])
    for old in audits:
        type = RENAMES.get(old['kind'], old['kind'])
        data, target = json.loads(old['data']), None
        if type == 'coordinate.update' and data.get('expiresAt', old['at'] + 1) <= old['at']:
            type = 'coordinate.out'
        if type == 'message' or type.startswith(('delivery.', 'dispatch.')):
            mail = messages.get(old.get('entityId'))
            if not mail:
                raise ValueError('Event references a missing message; migration rolled back')
            target = ('topic:' + mail['topic'] if mail.get('topic') else mail['target']) if type == 'message' else mail['sender']
            data['messageId'] = int(old['entityId'])
            if type == 'message':
                data.pop('target', None)
            elif type.startswith('dispatch.'):
                current = dispatches.get((old.get('entityId'), old['session']))
                if current and current['token'] == data.get('token'):
                    data.setdefault('transport', current['transport'])
        elif type.startswith('lease.'):
            data['leaseId'] = int(old['entityId'])
        db.execute('INSERT INTO records(id,path,"from","to",type,timestamp,data,entityId,key) VALUES(?,?,?,?,?,?,?,?,?)',
                   (old['id'], sessions[old['session']]['workspace'], old['session'], target, type, old['at'],
                    json.dumps(data, ensure_ascii=False, separators=(',', ':')), old.get('entityId'), old.get('key')))
    db.executemany('INSERT INTO documents(id,workspace,name) VALUES(?,?,?)', [(row['id'], row['workspace'], row['name']) for row in documents])
    db.execute("INSERT INTO records_search(rowid,text) SELECT r.id,r.data||' '||coalesce(m.body,'') FROM records r LEFT JOIN messages m ON r.type='message' AND m.id=CAST(r.entityId AS INTEGER)")
    return len(audits)


def migrate(source, backup):
    source, backup = Path(source), Path(backup)
    initial = source.lstat()
    if not stat.S_ISREG(initial.st_mode):
        raise ValueError('Migration source must be a regular file, not a symlink')
    if not backup.is_absolute():
        raise ValueError('Backup path must be absolute')
    backup = backup.parent.resolve(strict=True) / backup.name
    if os.path.lexists(backup):
        raise ValueError('Backup destination already exists; never overwrite it')
    source = source.resolve(strict=True)
    db = sqlite3.connect(source.as_uri() + '?mode=rw', uri=True, isolation_level=None, timeout=5)
    temp = None
    try:
        info = database.metadata(db)
        source_version = info['schemaVersion']
        source_fingerprint = {1: V1_SHA256, 2: V2_SHA256, 3: V3_SHA256, 4: V4_SHA256}.get(source_version)
        if info['applicationId'] != 1329678147 or source_fingerprint is None or database.fingerprint(db) != source_fingerprint:
            raise ValueError('Migration requires a recognized v1, v2, v3 or v4 schema; database is unchanged')
        if info['journalMode'] == 'wal' and not database.wal_safe(sqlite3.sqlite_version_info):
            raise ValueError('Upgrade SQLite to a WAL-safe version before migration')
        if db.execute('PRAGMA integrity_check').fetchone() != ('ok',) or db.execute('PRAGMA foreign_key_check').fetchall():
            raise ValueError('Source database failed integrity checks; database is unchanged')
        # No writer can enter between the backup snapshot and the migration.
        db.execute('PRAGMA foreign_keys=OFF')
        with database.transaction(db):
            if database.fingerprint(db) != source_fingerprint:
                raise ValueError('Source schema changed before migration')
            fd, temp = tempfile.mkstemp(prefix='.communication-migration-', dir=backup.parent)
            os.close(fd)
            reader = sqlite3.connect(source.as_uri() + '?mode=ro', uri=True, timeout=5)
            snapshot = sqlite3.connect(temp)
            try:
                deadline = time.monotonic() + 30
                def progress(*_):
                    if time.monotonic() > deadline:
                        raise ValueError('Backup deadline exceeded; migration is unchanged')
                reader.backup(snapshot, pages=128, progress=progress)
                snapshot.execute('PRAGMA journal_mode=DELETE')
                if (database.fingerprint(snapshot) != source_fingerprint or snapshot.execute('PRAGMA integrity_check').fetchone() != ('ok',)
                        or snapshot.execute('PRAGMA foreign_key_check').fetchall()):
                    raise ValueError('Backup verification failed; migration is unchanged')
            finally:
                snapshot.close()
                reader.close()
            # Windows FlushFileBuffers requires a writable handle; 'r+b' never truncates.
            with Path(temp).open('r+b') as stream:
                os.fsync(stream.fileno())
            os.link(temp, backup)
            for name, in db.execute("SELECT name FROM sqlite_schema WHERE type='trigger'").fetchall():
                db.execute('DROP TRIGGER "' + name.replace('"', '""') + '"')
            if source_version == 1:
                count = upgrade_v1(db)
            else:
                count = db.execute('SELECT count(*) FROM records').fetchone()[0]
            # Later versions only add tables and indexes; create each one that is missing.
            for statement in statements(database.SQL):
                match = re.search(r'^CREATE (?:TABLE|INDEX) "?(\w+)', statement, re.M)
                if match and not db.execute('SELECT 1 FROM sqlite_schema WHERE name=?', (match[1],)).fetchone():
                    db.execute(statement)
            for workspace, in db.execute('SELECT DISTINCT workspace FROM sessions').fetchall():
                scope = coordination_scope(workspace) if Path(workspace).is_dir() else workspace
                db.execute('INSERT OR IGNORE INTO workspaces VALUES(?,?)', (workspace, scope))
            db.execute('DELETE FROM peer_revisions')
            db.execute('INSERT INTO peer_revisions SELECT coordinationScope,1 FROM workspaces GROUP BY coordinationScope')
            db.execute('PRAGMA user_version=%d' % database.schema()['schemaVersion'])
            for statement in statements(database.SQL):
                if statement.lstrip().startswith('CREATE TRIGGER'):
                    db.execute(statement)
            database.validate(db)
            if db.execute('PRAGMA foreign_key_check').fetchall() or db.execute('PRAGMA integrity_check').fetchone() != ('ok',):
                raise ValueError('Migrated database failed integrity checks; migration rolled back')
            current = source.lstat()
            if (current.st_dev, current.st_ino) != (initial.st_dev, initial.st_ino):
                raise ValueError('Source replaced during migration')
        db.execute('PRAGMA foreign_keys=ON')
        digest = hashlib.sha256()
        with backup.open('rb') as stream:
            for chunk in iter(lambda: stream.read(1024 * 1024), b''):
                digest.update(chunk)
        return {'migrated': True, 'schemaVersion': database.schema()['schemaVersion'], 'sourceSchemaVersion': source_version, 'records': count, 'backup': str(backup), 'backupSha256': digest.hexdigest(), 'integrity': 'ok'}
    finally:
        db.close()
        if temp is not None:
            Path(temp).unlink(missing_ok=True)
