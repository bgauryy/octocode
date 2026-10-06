"""One scoped, indexed event envelope; operational transitions own their writes."""
import json
import re
from . import catalog
from .database import query, read_transaction, transaction


def encode(value):
    return json.dumps(value, ensure_ascii=False, separators=(',', ':'), allow_nan=False)


def append(db, session, type, data, timestamp, key=None, to=None, branch=None, entity=None):
    return db.execute('INSERT INTO records(path,"from","to",type,timestamp,branch,data,key,entityId) '
                      'SELECT workspace,id,?,?,?,coalesce(?,branch),?,?,? FROM sessions WHERE id=?',
                      (to, type, timestamp, branch, encode(data), key, entity, session)).lastrowid


def envelope(row):
    # recordId is a history cursor; naming it apart from message IDs keeps it out of complete.
    result = {'recordId': row['id']}
    result.update((key, row[key]) for key in ('path', 'from', 'type', 'timestamp'))
    result['to'] = row.get('to')
    if row.get('branch') is not None:
        result['branch'] = row['branch']
    result['data'] = json.loads(row['data'])
    return result


PAYLOAD = "CASE WHEN r.type='message' THEN json_set(r.data,'$.body',m.body,'$.expiresAt',m.expiresAt,'$.ttlMs',m.ttlMs) ELSE r.data END"
SELECT = 'SELECT r.id,r.path,r."from",r."to",r.type,r.timestamp,r.branch,' + PAYLOAD + ' AS data FROM records r LEFT JOIN messages m ON r.type=\'message\' AND m.id=CAST(r.entityId AS INTEGER)'
# Mail history is visible only to its author and the recipients fixed at send time.
# Custom targeted records use the same author/recipient rule; NULL means workspace-shared.
VISIBILITY = '''((r.type='message' OR r.type LIKE 'delivery.%' OR r.type LIKE 'dispatch.%')
 AND EXISTS(SELECT 1 FROM messages v WHERE v.id=CAST(r.entityId AS INTEGER)
 AND (v.sender=? OR EXISTS(SELECT 1 FROM deliveries d WHERE d.message=v.id AND d.recipient=?)))
 OR (r.type<>'message' AND r.type NOT LIKE 'delivery.%' AND r.type NOT LIKE 'dispatch.%'
 AND (r."to" IS NULL OR r."to"='*' OR r."from"=? OR r."to"=?)))'''


class RecordsMixin:
    def fetch(self, session, value):
        from .store import now, page
        catalog.command('fetch', value)
        with read_transaction(self.db):
            self.known(session, False)
            through = value.get('through')
            if through is None:
                through = self.db.execute('SELECT coalesce(max(id),0) FROM records WHERE path IN (SELECT value FROM json_each(?))', (encode(self.shared_workspaces()),)).fetchone()[0]
            if through < value.get('after', 0):
                raise ValueError('through must be at least after')
            conditions = ['r.path IN (SELECT value FROM json_each(?))', 'r.id>?', 'r.id<=?', VISIBILITY]
            args = [encode(self.shared_workspaces()), value.get('after', 0), through] + [session] * 4
            for field, column in (('recordId', 'id'), ('type', 'type'), ('from', 'from'), ('to', 'to'), ('branch', 'branch')):
                if field in value:
                    conditions.append('r."' + column + '"=?')
                    args.append(value[field])
            if 'types' in value:
                conditions.append('r.type IN (SELECT value FROM json_each(?))')
                args.append(encode(value['types']))
            for field, op in (('since', '>='), ('until', '<=')):
                if field in value:
                    conditions.append('r.timestamp' + op + '?')
                    args.append(value[field])
            if value.get('incoming'):
                conditions.append("r.type='message' AND r.entityId IN (SELECT CAST(d.message AS TEXT) FROM deliveries d JOIN messages i ON i.id=d.message WHERE d.recipient=? AND d.acknowledgedAt IS NULL AND i.expiresAt>?)")
                args.extend((session, now()))
            if value.get('current'):
                conditions.append("r.type GLOB 'dispatch.*' AND EXISTS(SELECT 1 FROM dispatches x WHERE CAST(x.message AS TEXT)=r.entityId AND x.recipient=r.\"from\" AND x.state=substr(r.type,10) AND x.token=json_extract(r.data,'$.token'))")
                conditions.append("r.id=(SELECT max(latest.id) FROM records latest WHERE latest.path=r.path AND latest.\"from\"=r.\"from\" AND latest.entityId=r.entityId AND latest.type GLOB 'dispatch.*')")
            for field, expected in value.get('where', {}).items():
                if not re.fullmatch(r'[A-Za-z_][A-Za-z0-9_]*(?:\.[A-Za-z_][A-Za-z0-9_]*)*', field):
                    raise ValueError('where keys must be dotted data field names')
                # Known operational IDs use the reference index before JSON filtering.
                stream = value.get('type', '')
                if (type(expected) is int and ((field == 'messageId' and (stream == 'message' or stream.startswith(('delivery.', 'dispatch.'))))
                                               or (field == 'leaseId' and stream.startswith('lease.')))):
                    conditions.append('r.entityId=?')
                    args.append(str(expected))
                # Type checks distinguish JSON false from 0, and explicit null from a missing field.
                kind = 'null' if expected is None else 'true' if expected is True else 'false' if expected is False else 'text' if isinstance(expected, str) else None
                conditions.append('json_extract(' + PAYLOAD + ',?) IS ?')
                args.extend(('$.' + field, expected))
                conditions.append('json_type(' + PAYLOAD + ',?) ' + ('=?' if kind else "IN ('integer','real')"))
                args.append('$.' + field)
                if kind:
                    args.append(kind)
            if 'search' in value:
                words = re.findall(r'"([^"]+)"|(\S+)', value['search'])
                if not words:
                    raise ValueError('search requires words or phrases')
                terms = ' AND '.join('"' + (phrase or word).replace('"', '""') + '"' for phrase, word in words)
                conditions.append('r.id IN (SELECT rowid FROM records_search WHERE records_search MATCH ?)')
                args.append(terms)
            rows = query(self.db, SELECT + ' WHERE ' + ' AND '.join(conditions) + ' ORDER BY r.id LIMIT ?', args + [value.get('limit', 100) + 1])
            result = page([envelope(row) for row in rows], False, 'fetch', dict(value, through=through))
            # page also has a byte budget; honor the caller's smaller item budget.
            limit = value.get('limit', 100)
            if len(result['items']) > limit:
                result['items'] = result['items'][:limit]
                result['next'] = {'command': 'fetch', 'input': dict(value, through=through, after=result['items'][-1]['recordId'])}
            result['through'] = through
            return result

    def record(self, session, value):
        from .store import now
        catalog.command('record', value)
        data = encode(value['data'])
        if len(data.encode()) > 16384:
            raise ValueError('Record data exceeds 16384 UTF-8 bytes')
        with transaction(self.db):
            identity = self.known(session)
            if 'to' in value and value['to'] != '*':
                value = dict(value, to=self.resolve_peer(value['to']))
                self.peer(value['to'])
            branch = value.get('branch', identity.get('branch'))
            existing = query(self.db, 'SELECT * FROM records WHERE "from"=? AND type=? AND key=?', (session, value['type'], value.get('key')))
            if existing:
                row = existing[0]
                if (json.dumps(json.loads(row['data']), sort_keys=True) != json.dumps(value['data'], sort_keys=True)
                        or row.get('to') != value.get('to') or row.get('branch') != branch):
                    raise ValueError('Record key reused with different data, routing or branch')
                return envelope(row)
            # Explicit null branch suppresses the session default.
            id = self.db.execute('INSERT INTO records(path,"from","to",type,timestamp,branch,data,key) VALUES(?,?,?,?,?,?,?,?)',
                                 (self.workspace, session, value.get('to'), value['type'], now(), branch, data, value.get('key'))).lastrowid
            return envelope(query(self.db, 'SELECT * FROM records WHERE id=?', (id,))[0])

    def message_envelopes(self, items):
        if not items:
            return []
        rows = query(self.db, SELECT + " WHERE r.path IN (SELECT value FROM json_each(?)) AND r.type='message' AND r.entityId IN (SELECT CAST(value AS TEXT) FROM json_each(?))",
                     (encode(self.shared_workspaces()), encode([item['id'] for item in items])))
        by_message = {row['data']['messageId']: row for row in map(envelope, rows)}
        return [by_message[item['id']] for item in items]
