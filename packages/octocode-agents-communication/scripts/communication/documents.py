"""Immutable workspace documents and bounded, scoped context discovery."""
import copy
import hashlib
import json
import os
import re
import stat
import tempfile
from pathlib import Path
from .catalog import text
from .database import query, read_transaction, transaction
from .paths import overlap, resolve_path
from .records import append

MAX_BYTES = 1024 * 1024
CONTEXT_SCAN = 200
PAGE_BYTES = 16 * 1024


def _now():
    from .store import now
    return now()


def _json(value):
    return json.dumps(value, ensure_ascii=False, separators=(',', ':'), allow_nan=False)


def scope_path(workspace, path):
    resolved = Path(resolve_path(Path(workspace), path))
    try:
        relative = resolved.relative_to(workspace)
    except ValueError:
        raise ValueError('Context path must stay inside this workspace') from None
    return relative.as_posix() or '.'


def context_metadata(workspace, value):
    context = copy.deepcopy(value.get('context'))
    if context is None:
        return None
    if not context.get('summary', '').strip():
        raise ValueError('Context summary must explain a useful fact or gotcha')
    context['path'] = scope_path(workspace, context.get('path', '.'))
    context.setdefault('kind', 'file' if (Path(workspace) / context['path']).is_file() else 'tree')
    context.setdefault('ttlMs', 86400000)
    return context


def document_name(value):
    name = value.get('name')
    if not isinstance(name, str) or not re.fullmatch(r'[a-z0-9][a-z0-9._-]{0,127}', name) or '..' in name:
        raise ValueError("Use a lowercase document filename (letters, digits, dot, dash, underscore), without directories or '..'")
    return name


def directory(workspace, create=False, coordination=None):
    path = Path(workspace)
    components = ('.octocode', 'communication')
    if coordination is not None and coordination != workspace:
        path, components = Path(coordination), ('octocode-communication',)
    for component in components:
        path = path / component
        if create:
            try:
                path.mkdir()
            except FileExistsError:
                pass
        if not stat.S_ISDIR(path.lstat().st_mode):
            raise ValueError('Document directories must be real directories, not symlinks')
    return path


def open_document(path):
    if not stat.S_ISREG(path.lstat().st_mode):
        raise ValueError('Document must be a regular file, not a symlink')
    fd = os.open(path, os.O_RDONLY | getattr(os, 'O_NOFOLLOW', 0) | getattr(os, 'O_NONBLOCK', 0))
    try:
        if not stat.S_ISREG(os.fstat(fd).st_mode):
            raise ValueError('Document must be a regular file')
        return os.fdopen(fd, 'rb')
    except BaseException:
        os.close(fd)
        raise


def read(path):
    with open_document(path) as stream:
        data = stream.read(MAX_BYTES + 1)
    if len(data) > MAX_BYTES:
        raise ValueError('Document exceeds 1 MiB')
    return data.decode('utf-8')


class DocumentsMixin:
    def document_record(self, db, name, workspace=None):
        scopes = self.shared_workspaces()
        if workspace is not None:
            workspace = str(Path(workspace).resolve())
            if workspace not in scopes:
                raise ValueError('Document workspace is outside this repository')
            scopes = [workspace]
        rows = query(db, 'SELECT a.data,d.workspace FROM documents d JOIN records a ON a.id=d.id WHERE d.workspace IN (SELECT value FROM json_each(?)) AND d.name=?', (json.dumps(scopes), name))
        if len(rows) > 1:
            raise ValueError('Ambiguous historical document name; set workspace to its origin: ' + _json([r['workspace'] for r in rows]))
        return json.loads(rows[0]['data']) if rows else None

    def document_path(self, record):
        origin = self.peer(record['author'])['workspace']
        path = Path(record['path'])
        if not path.is_absolute():
            path = Path(origin) / path
        legacy = Path(origin) / '.octocode' / 'communication' / record['name']
        shared = Path(self.coordination_scope) / 'octocode-communication' / record['name']
        if path not in (legacy, shared):
            raise ValueError('Document path is outside its registered storage')
        folder = directory(origin) if path == legacy else directory(origin, coordination=self.coordination_scope)
        return folder / record['name']

    @staticmethod
    def same_document(record, path, content, metadata, reasoning):
        stored = copy.deepcopy(record.get('context'))
        if stored is not None:
            stored.pop('expiresAt', None)
        if (record['sha256'] != hashlib.sha256(content.encode('utf-8')).hexdigest()
                or read(path) != content or stored != metadata or record.get('reasoning') != reasoning):
            raise ValueError('Document is immutable or has changed on disk; publish a new name')
        return {'created': False, 'document': record}

    def share_document(self, session, value):
        name = document_name(value)
        reasoning = text(value, 'reasoning')
        content = value.get('content')
        if not isinstance(content, str):
            raise ValueError('Document content required')
        data = content.encode('utf-8')
        if len(data) > MAX_BYTES:
            raise ValueError('Document exceeds 1 MiB')
        metadata = context_metadata(self.workspace, value)
        folder = directory(self.workspace, True, self.coordination_scope)
        path = folder / name
        with read_transaction(self.db):
            self.known(session, True)
            record = self.document_record(self.db, name)
        if record is not None:
            return self.same_document(record, self.document_path(record), content, metadata, reasoning)
        if os.path.lexists(path):
            raise ValueError('Unregistered document already exists; preserve it and publish a new name')
        fd, temp = tempfile.mkstemp(prefix='.document-', dir=folder)
        try:
            with os.fdopen(fd, 'wb') as stream:
                stream.write(data)
                stream.flush()
                os.fsync(stream.fileno())
            with transaction(self.db):
                self.known(session, True)
                record = self.document_record(self.db, name)
                if record is not None:
                    return self.same_document(record, self.document_path(record), content, metadata, reasoning)
                if os.path.lexists(path):
                    raise ValueError('Unregistered document already exists; preserve it and publish a new name')
                # Same-directory link publication never exposes a partial file or overwrites.
                os.link(temp, path)
                record = {'name': name, 'path': str(path) if self.coordination_scope != self.workspace else '.octocode/communication/' + name,
                          'author': session, 'reasoning': reasoning, 'bytes': len(data),
                          'sha256': hashlib.sha256(data).hexdigest()}
                if metadata is not None:
                    record['context'] = metadata
                    record['context']['expiresAt'] = _now() + metadata['ttlMs']
                append(self.db, session, 'document', record, _now(), key=name, entity=name)
                return {'created': True, 'document': record}
        finally:
            os.unlink(temp)

    def context(self, session, value):
        path = scope_path(self.workspace, value.get('path', '.'))
        after, limit = value.get('after', 0), value.get('limit', 10)
        with read_transaction(self.db):
            self.known(session, True)
            through = value.get('through')
            if through is None:
                through = self.db.execute('SELECT coalesce(max(id),0) FROM documents WHERE workspace IN (SELECT value FROM json_each(?))', (json.dumps(self.shared_workspaces()),)).fetchone()[0]
            if through < after:
                raise ValueError('Context through must be at least after')
            rows = query(self.db, 'SELECT d.id,d.workspace,a.branch,a.data FROM documents d JOIN records a ON a.id=d.id WHERE d.workspace IN (SELECT value FROM json_each(?)) AND d.id>? AND d.id<=? ORDER BY d.id LIMIT ?',
                         (json.dumps(self.shared_workspaces()), after, through, CONTEXT_SCAN))
            items, cursor, scanned, size, at = [], after, 0, 0, _now()
            for row in rows:
                previous, cursor = cursor, row['id']
                scanned += 1
                record = json.loads(row['data'])
                context = record.get('context') or {}
                scope = context.get('path')
                if (scope is None or context.get('expiresAt', 0) <= at
                        or (context.get('branch') is not None and context['branch'] != value.get('branch'))
                        or not overlap(scope, context.get('kind', 'tree'), path, 'file')):
                    continue
                item = {'id': cursor, 'name': record['name'], 'author': record['author'], 'workspace': row['workspace'], 'branch': row.get('branch'), 'context': context}
                amount = len(_json(item).encode('utf-8'))
                if items and size + amount > PAGE_BYTES:
                    cursor, scanned = previous, scanned - 1
                    break
                size += amount
                items.append(item)
                if len(items) == limit:
                    break
            if scanned == len(rows) and len(rows) < CONTEXT_SCAN:
                cursor = through
            result = {'items': items, 'cursor': cursor, 'scanned': scanned}
            if cursor < through:
                result['next'] = {'command': 'context', 'input': dict(value, after=cursor, through=through)}
            return result

    def read_document(self, session, value):
        self.known(session, True)
        name = document_name(value)
        record = self.document_record(self.db, name, value.get('workspace'))
        if record is None:
            candidates = query(self.db, 'SELECT name FROM documents WHERE workspace IN (SELECT value FROM json_each(?)) AND name>=? AND name<? ORDER BY name LIMIT 5', (json.dumps(self.shared_workspaces()), name + '.', name + '/'))
            raise ValueError("Unknown document '%s' in this repository. Use the exact published document.name, including its extension. Matching names (up to 5): %s. Read the document successfully before using its contents." % (name, _json([r['name'] for r in candidates])))
        offset, limit = value.get('offset', 0), value.get('limit', 8192)
        length = record.get('bytes')
        if type(length) is not int or not 0 <= length <= MAX_BYTES:
            raise ValueError('Invalid document size')
        if offset > length:
            raise ValueError('Offset must be a UTF-8 byte boundary within the document')
        digest, scanned, page = hashlib.sha256(), 0, bytearray()
        with open_document(self.document_path(record)) as stream:
            while scanned <= MAX_BYTES:
                data = stream.read(min(8192, MAX_BYTES + 1 - scanned))
                if not data:
                    break
                digest.update(data)
                start = min(max(offset - scanned, 0), len(data))
                end = min(max(offset + limit - scanned, 0), len(data))
                page.extend(data[start:end])
                scanned += len(data)
        if scanned != length or record['sha256'] != digest.hexdigest():
            raise ValueError('Document integrity mismatch; ask the author to publish a new document')
        try:
            content = page.decode('utf-8')
        except UnicodeDecodeError as error:
            if error.reason == 'unexpected end of data':
                content = page[:error.start].decode('utf-8')
            else:
                raise ValueError('Offset must be a UTF-8 byte boundary within the document') from None
        end = offset + len(content.encode('utf-8'))
        result = {'document': record, 'offset': offset, 'content': content}
        if end < length:
            result['next'] = {'command': 'read_document', 'input': dict(value, name=name, offset=end, limit=limit)}
        return result
