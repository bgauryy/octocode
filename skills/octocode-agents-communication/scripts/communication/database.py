"""SQLite protocol storage, compatibility checks, and no-clobber snapshots."""
import hashlib
import json
import os
import sqlite3
import stat
import tempfile
import time
from contextlib import contextmanager
from functools import lru_cache
from pathlib import Path
from .paths import overlap

SQL = (Path(__file__).resolve().parent.parent / 'schema.sql').read_text(encoding='utf-8')


def query(db, sql, args=()):
    cur = db.execute(sql, args)
    names = [c[0] for c in cur.description]
    return [{k: bool(v) if k == 'replyRequired' else v for k, v in zip(names, row) if v is not None} for row in cur]


def execute(db, sql, args=()):
    return db.execute(sql, args).rowcount


@contextmanager
def transaction(db):
    db.execute('BEGIN IMMEDIATE')
    try:
        yield db
        db.execute('COMMIT')
    except BaseException:
        if db.in_transaction:
            db.execute('ROLLBACK')
        raise


@contextmanager
def read_transaction(db):
    db.execute('BEGIN DEFERRED')
    try:
        yield db
        db.execute('COMMIT')
    except BaseException:
        if db.in_transaction:
            db.execute('ROLLBACK')
        raise


def metadata(db):
    def pragma(name):
        return db.execute('PRAGMA ' + name).fetchone()[0]
    return dict(applicationId=pragma('application_id'), schemaVersion=pragma('user_version'), sqliteVersion=sqlite3.sqlite_version, journalMode=pragma('journal_mode'))


def fingerprint(db):
    rows = [dict(type=t, name=n, tbl_name=table, sql=' '.join((sql or '').split())) for t, n, table, sql in db.execute("SELECT type,name,tbl_name,sql FROM sqlite_schema WHERE name NOT LIKE 'sqlite_%' ORDER BY type,name")]
    return hashlib.sha256(json.dumps(rows, ensure_ascii=False, separators=(',', ':')).encode()).hexdigest()


@lru_cache(maxsize=1)
def schema():
    db = sqlite3.connect(':memory:')
    try:
        db.executescript(SQL)
        info = metadata(db)
        relationships = []
        for (table,) in db.execute("SELECT name FROM sqlite_schema WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name").fetchall():
            keys = {}
            for key, _, target, source_column, target_column, on_delete in db.execute(
                    'SELECT id,seq,"table","from","to",on_delete FROM pragma_foreign_key_list(?) ORDER BY id,seq', (table,)):
                relation = keys.setdefault(key, {'table': table, 'columns': [],
                    'references': {'table': target, 'columns': []}, 'onDelete': on_delete})
                relation['columns'].append(source_column)
                relation['references']['columns'].append(target_column)
            relationships.extend(keys.values())
        return dict(applicationId=info['applicationId'], schemaVersion=info['schemaVersion'],
                    schemaSha256=fingerprint(db), relationships=relationships)
    finally:
        db.close()


def compatible(db, info=None):
    info = info or metadata(db)
    current = schema()
    return all(info[key] == current[key] for key in ('applicationId', 'schemaVersion')) and fingerprint(db) == current['schemaSha256']


def validate(db, info=None):
    if not compatible(db, info):
        raise ValueError('Incompatible agents-communication database. Use a fresh development database with the current schema; existing data is not modified.')


def path(database=None):
    if database is not None:
        return Path(os.path.abspath(database))
    from .config import database_path
    return database_path()


def inspect(path, workspace):
    path = Path(path)
    current = schema()
    result = dict(path=str(path), workspace=str(Path(workspace).resolve(strict=True)), exists=path.exists(), expectedApplicationId=current['applicationId'], expectedSchemaVersion=current['schemaVersion'], expectedSchemaSha256=current['schemaSha256'], compatible=False)
    if path.exists():
        db = sqlite3.connect(path.resolve().as_uri() + '?mode=ro', uri=True)
        try:
            result.update(metadata(db), schemaSha256=fingerprint(db), compatible=compatible(db))
        finally:
            db.close()
    return result


def wal_safe(version):
    """Upstream WAL-reset fixes, including maintained backport branches."""
    version = tuple(version)
    return version >= (3, 51, 3) or ((3, 50, 7) <= version < (3, 51, 0)) or ((3, 44, 6) <= version < (3, 45, 0))


def open(path, read_only=False, create=False):
    if sqlite3.sqlite_version_info < (3, 42, 0):
        raise ValueError('SQLite >=3.42.0 required; use Python linked to a newer SQLite')
    path = Path(path)
    if not path.exists():
        if read_only or not create:
            raise ValueError('Database does not exist: {}. Run join first.'.format(path))
        path.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
        try:
            fd = os.open(str(path), os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
            os.close(fd)
        except FileExistsError:
            pass
    deadline = time.monotonic() + 5
    while True:
        try:
            return _open_connection(path, read_only, create)
        except sqlite3.OperationalError as error:
            if not any(x in str(error).lower() for x in ('locked', 'busy')) or time.monotonic() >= deadline:
                raise
            time.sleep(.02)


def _open_connection(path, read_only, create):
    db = sqlite3.connect(path.absolute().as_uri() + ('?mode=ro' if read_only else '?mode=rw'), uri=True, timeout=5, isolation_level=None)
    try:
        db.create_function('lease_overlap', 4, overlap, deterministic=True)
        db.execute('PRAGMA foreign_keys=ON')
        if read_only:
            with read_transaction(db):
                validate(db)
            return db
        with read_transaction(db):
            info = metadata(db)
            uninitialized = info['applicationId'] == 0 and info['schemaVersion'] == 0
            if not uninitialized:
                validate(db, info)
        initialized_here = False
        if info['journalMode'] == 'wal' and not wal_safe(sqlite3.sqlite_version_info):
            raise ValueError('This database uses WAL; upgrade Python to SQLite 3.51.3+, 3.50.7+, or 3.44.6+ on their respective release branches before writing. Existing journal mode is unchanged.')
        if uninitialized:
            with transaction(db):
                info = metadata(db)
                empty = db.execute('SELECT count(*) FROM sqlite_schema').fetchone()[0] == 0
                if empty and info['applicationId'] == 0 and info['schemaVersion'] == 0:
                    if not create:
                        raise ValueError('Database is not initialized. Run join first.')
                    # executescript commits implicitly; execute complete statements to keep
                    # initialization and protocol markers within the writer transaction.
                    statement = ''
                    for line in SQL.splitlines(True):
                        statement += line
                        if sqlite3.complete_statement(statement):
                            db.execute(statement)
                            statement = ''
                    initialized_here = True
                else:
                    validate(db, info)
        if initialized_here and wal_safe(sqlite3.sqlite_version_info):
            db.execute('PRAGMA journal_mode=WAL')
        db.execute('PRAGMA synchronous=FULL')
        return db
    except BaseException:
        db.close()
        raise


def export(source, destination):
    source, destination = Path(source), Path(destination)
    if not destination.is_absolute():
        raise ValueError('Export path must be absolute')
    parent = destination.parent.resolve(strict=True)
    destination = parent / destination.name
    if os.path.lexists(destination):
        raise ValueError('Export destination already exists; never overwrite a file or symlink')
    initial = source.lstat()
    if not stat.S_ISREG(initial.st_mode):
        raise ValueError('Export source must be a regular database file, not a symlink')
    source = source.resolve(strict=True)
    db = open(source, True, False)
    fd, temp = tempfile.mkstemp(prefix='.communication-export-', dir=parent)
    os.close(fd)
    try:
        target = sqlite3.connect(temp)
        try:
            db.backup(target)
            target.execute('PRAGMA journal_mode=DELETE')
            target.execute('VACUUM')
        finally:
            target.close()
        current = source.lstat()
        if not stat.S_ISREG(current.st_mode) or (initial.st_dev, initial.st_ino) != (current.st_dev, current.st_ino):
            raise ValueError('Source database was replaced during export')
        snapshot = open(Path(temp), True, False)
        try:
            if query(snapshot, 'PRAGMA integrity_check') != [{'integrity_check': 'ok'}]:
                raise ValueError('Export integrity check failed')
            if query(snapshot, 'PRAGMA foreign_key_check'):
                raise ValueError('Export foreign key check failed')
        finally:
            snapshot.close()
        with Path(temp).open('rb') as reader:
            os.fsync(reader.fileno())
            digest = hashlib.sha256()
            for block in iter(lambda: reader.read(65536), b''):
                digest.update(block)
        size = Path(temp).stat().st_size
        os.link(temp, destination)
        synced = False
        try:
            directory = os.open(parent, os.O_RDONLY)
            try:
                os.fsync(directory)
                synced = True
            finally:
                os.close(directory)
        except OSError:
            pass
        return dict(path=str(destination), source=str(source), schemaVersion=schema()['schemaVersion'], sha256=digest.hexdigest(), bytes=size, scope='all-workspaces', includesWorkspaceDocuments=False, documents='Preserve referenced workspace .octocode/communication files separately; this snapshot contains their audit metadata only.', integrity='ok', directorySynced=synced)
    finally:
        db.close()
        for suffix in ('', '-wal', '-shm', '-journal'):
            Path(temp + suffix).unlink(missing_ok=True)
