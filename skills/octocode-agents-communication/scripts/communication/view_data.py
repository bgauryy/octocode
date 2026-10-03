"""Read-only workspace operator views; no synthetic agent identity."""
import base64
import json
import uuid
from .database import query

ENTITIES = ['session', 'message', 'lease', 'audit', 'delivery', 'dispatch', 'document', 'attachment', 'subscriptions']
QUERIES = {
 'session': ('SELECT s.*,s.expiresAt>?2 AS active FROM sessions s WHERE s.workspace=?1', 'id', False),
 'message': ('SELECT m.*,m.expiresAt-m.ttlMs AS createdAt,s.name AS senderName,s.vendor AS senderVendor,t.name AS targetName,t.vendor AS targetVendor,(SELECT count(*) FROM deliveries d WHERE d.message=m.id AND d.acknowledgedAt IS NULL) AS pending,(SELECT count(*) FROM deliveries d WHERE d.message=m.id) AS recipients FROM messages m JOIN sessions s ON s.id=m.sender LEFT JOIN sessions t ON t.id=m.target WHERE s.workspace=?1 AND ?2>0', 'sender', False),
 'lease': ('SELECT l.id,l.workspace,l.path,l.kind,l.owner,s.name AS ownerName,l.reasoning,l.acquiredAt,l.refreshedAt,l.expiresAt FROM leases l JOIN sessions s ON s.id=l.owner WHERE l.workspace=?1 AND s.workspace=?1 AND l.expiresAt>?2 AND s.expiresAt>?2', 'owner', False),
 'audit': ('SELECT a.*,s.name AS agentName,s.vendor FROM audit a JOIN sessions s ON s.id=a.session WHERE s.workspace=?1 AND ?2>0', 'session', False),
 'delivery': ("SELECT CAST(d.message AS TEXT)||':'||d.recipient AS id,d.*,s.name AS recipientName FROM deliveries d JOIN sessions s ON s.id=d.recipient WHERE s.workspace=?1 AND ?2>0", 'recipient', True),
 'dispatch': ("SELECT CAST(d.message AS TEXT)||':'||d.recipient AS id,d.message,d.recipient,s.name AS recipientName,d.transport,d.state,d.attemptedAt,d.submittedAt,d.error FROM dispatches d JOIN sessions s ON s.id=d.recipient WHERE s.workspace=?1 AND ?2>0", 'recipient', True),
 'document': ('SELECT d.id,d.name,a.session,s.name AS agentName,a.at,a.data FROM documents d JOIN audit a ON a.id=d.id JOIN sessions s ON s.id=a.session WHERE d.workspace=?1 AND s.workspace=?1 AND ?2>0', 'session', False),
 'attachment': ('SELECT a.session AS id,a.session,a.transport,a.updatedAt,s.name AS agentName FROM attachments a JOIN sessions s ON s.id=a.session WHERE s.workspace=?1 AND ?2>0', 'session', False),
 'subscriptions': ("SELECT s.id,s.id AS session,s.name AS agentName,coalesce((SELECT json_group_array(topic) FROM subscriptions WHERE session=s.id),'[]') AS topics FROM sessions s WHERE s.workspace=?1 AND ?2>0", 'session', False),
}


def summary(store):
    from .store import now
    store.check_database()
    # One statement gives a consistent read snapshot without acquiring a writer lock.
    counts = query(store.db, """SELECT
      (SELECT count(*) FROM sessions WHERE workspace=?1 AND expiresAt>?2) AS activeAgents,
      (SELECT count(*) FROM messages m JOIN sessions s ON s.id=m.sender WHERE s.workspace=?1) AS messages,
      (SELECT count(*) FROM leases l JOIN sessions s ON s.id=l.owner WHERE l.workspace=?1 AND s.workspace=?1 AND l.expiresAt>?2 AND s.expiresAt>?2) AS activeLocks,
      (SELECT count(*) FROM deliveries d JOIN sessions s ON s.id=d.recipient WHERE s.workspace=?1 AND d.acknowledgedAt IS NULL) AS pending,
      (SELECT count(*) FROM dispatches d JOIN sessions s ON s.id=d.recipient WHERE s.workspace=?1 AND d.state='uncertain') AS uncertain""", (store.workspace, now()))
    return {'workspace': store.workspace, 'database': str(store.database), 'at': now(), 'counts': counts[0], 'entities': ENTITIES}


def page(store, entity, after=None, agent=None, filters=None):
    from .store import now
    store.check_database()
    if entity not in QUERIES:
        raise ValueError('Unknown view entity')
    sql, agent_field, pair = QUERIES[entity]
    at = now()
    args, conditions = [store.workspace, at], ['1=1']
    if agent is not None:
        uuid.UUID(agent)
        args.append(agent)
        n = len(args)
        conditions.append(f'(sender=?{n} OR id IN (SELECT message FROM deliveries WHERE recipient=?{n}))' if entity == 'message' else f'{agent_field}=?{n}')
    filters = {} if filters is None else filters
    if not isinstance(filters, dict):
        raise ValueError('Invalid message filters')
    for key, value in filters.items():
        if entity != 'message':
            raise ValueError('Message filters require the Messages view')
        if not isinstance(value, str):
            raise ValueError('Invalid filter')
        if not value or len(value.encode()) > 256:
            raise ValueError('Filter must contain 1–256 bytes')
        if key == 'q':
            args.append(value)
            n = len(args)
            conditions.append(f'(instr(lower(body),lower(?{n}))>0 OR instr(lower(senderName),lower(?{n}))>0 OR instr(lower(targetName),lower(?{n}))>0 OR instr(lower(conversationId),lower(?{n}))>0 OR CAST(id AS TEXT)=?{n})')
        elif key == 'conversation':
            args.append(value)
            conditions.append(f'conversationId=?{len(args)}')
        elif key == 'status':
            if value not in ('pending', 'handled'):
                raise ValueError('Unknown message status')
            conditions.append('pending>0' if value == 'pending' else 'pending=0 AND recipients>0')
        else:
            raise ValueError('Unknown message filter')
    if after is not None:
        cursor = json.loads(base64.b64decode(after + '=' * (-len(after) % 4), altchars=b'-_', validate=True))
        if not isinstance(cursor, dict) or cursor.get('entity') != entity or cursor.get('agent') != agent or cursor.get('filters') != filters:
            raise ValueError('Cursor belongs to another view; refresh the first page')
        if pair:
            message, recipient = cursor.get('message'), cursor.get('recipient')
            if type(message) is not int or not isinstance(recipient, str):
                raise ValueError('Invalid cursor')
            n = len(args) + 1
            conditions.append(f'(message<?{n} OR (message=?{n} AND recipient<?{n+1}))')
            args.extend((message, recipient))
        else:
            identity = cursor.get('id')
            if type(identity) not in (int, str):
                raise ValueError('Invalid cursor')
            args.append(identity)
            conditions.append(f'id<?{len(args)}')
    order = 'message DESC,recipient DESC' if pair else 'id DESC'
    rows = query(store.db, f"SELECT * FROM ({sql}) WHERE {' AND '.join(conditions)} ORDER BY {order} LIMIT 51", args)
    size, count = 0, 0
    for row in rows[:50]:
        length = len(json.dumps(row, ensure_ascii=False, separators=(',', ':')).encode())
        if size and size + length > 128 * 1024:
            break
        size += length
        count += 1
    next_cursor = None
    if len(rows) > count and count:
        row = rows[count - 1]
        cursor = dict(entity=entity, agent=agent, filters=filters, id=row.get('id'), message=row.get('message'), recipient=row.get('recipient'))
        next_cursor = base64.urlsafe_b64encode(json.dumps(cursor, ensure_ascii=False, separators=(',', ':')).encode()).decode().rstrip('=')
    rows = rows[:count]
    for row in rows:
        for field in ('data', 'topics'):
            if isinstance(row.get(field), str):
                row[field] = json.loads(row[field])
    return {'items': rows, 'next': next_cursor, 'at': at}
