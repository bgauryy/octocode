"""Permission-scoped entity reads and dedicated session updates."""
import json
from . import catalog
from .database import execute, query, transaction


class EntityMixin:
    def entity_query(self, session, name):
        from .store import now
        self.known(session, False)
        visible = 'SELECT id FROM messages WHERE sender=?1'
        if name == 'session':
            return 'SELECT *,expiresAt>?2 AS active FROM sessions WHERE workspace=?1', [self.workspace, now()], 'text'
        if name == 'lease':
            return 'SELECT l.*,l.expiresAt>?2 AND s.expiresAt>?2 AS active FROM leases l JOIN sessions s ON s.id=l.owner WHERE l.workspace=?1 AND s.workspace=?1', [self.workspace, now()], 'integer'
        if name == 'message':
            return 'SELECT m.* FROM messages m WHERE m.id IN ({} UNION SELECT message FROM deliveries WHERE recipient=?1) AND EXISTS(SELECT 1 FROM sessions s WHERE s.id=m.sender AND s.workspace=?2)'.format(visible), [session, self.workspace], 'integer'
        if name in ('delivery', 'dispatch'):
            table = 'deliveries' if name == 'delivery' else 'dispatches'
            return "SELECT CAST(d.message AS TEXT)||':'||d.recipient AS id,d.* FROM {} d WHERE (d.recipient=?1 OR d.message IN ({})) AND EXISTS(SELECT 1 FROM messages m JOIN sessions s ON s.id=m.sender WHERE m.id=d.message AND s.workspace=?2)".format(table, visible), [session, self.workspace], 'pair'
        if name == 'subscriptions':
            return "SELECT s.id,s.id AS session,coalesce((SELECT json_group_array(topic) FROM (SELECT topic FROM subscriptions WHERE session=s.id ORDER BY topic)), '[]') AS topics FROM sessions s WHERE s.workspace=?1", [self.workspace], 'text'
        if name == 'attachment':
            return 'SELECT a.session AS id,a.* FROM attachments a JOIN sessions s ON s.id=a.session WHERE s.workspace=?1', [self.workspace], 'text'
        if name == 'audit':
            return 'SELECT a.* FROM audit a WHERE EXISTS(SELECT 1 FROM sessions s WHERE s.id=a.session AND s.workspace=?1)', [self.workspace], 'integer'
        raise ValueError('Unknown entity: ' + name)

    def entity_get(self, session, name, id):
        sql, args, _ = self.entity_query(session, name)
        rows = query(self.db, 'SELECT * FROM ({}) WHERE id=?'.format(sql), args + [id])
        return decode(name, rows[0]) if rows else None

    def entity_list(self, session, name, filters):
        from .store import page
        catalog.validate(catalog.entity(name)['list'], filters)
        sql, args, order = self.entity_query(session, name)
        conditions = []
        if isinstance(filters.get('after'), str):
            after = filters['after']
            try:
                if order == 'pair':
                    message, recipient = after.split(':', 1)
                    conditions.append('(message>? OR (message=? AND recipient>?))')
                    args.extend([int(message), int(message), recipient])
                else:
                    conditions.append('id>?')
                    args.append(int(after) if order == 'integer' else after)
            except ValueError:
                raise ValueError('after must be a previous next value')
        if name in ('session', 'lease') and filters.get('presence', 'active') != 'all':
            conditions.append('active=?')
            args.append(filters.get('presence', 'active') == 'active')
        for field in ('vendor', 'owner', 'message', 'conversationId', 'replyTo'):
            if field in filters:
                conditions.append(field + '=?')
                args.append(filters[field])
        if 'kind' in filters and 'path' not in filters:
            raise ValueError('kind requires a path conflict query')
        if 'path' in filters:
            target = self.lease_target(filters)
            conditions.append("(pathKey=? OR (kind='tree' AND pathKey IN (SELECT value FROM json_each(?))) OR (?='tree' AND pathKey>? AND pathKey<?))")
            args.extend(target.key_args())
        if filters.get('direction') in ('sent', 'received'):
            conditions.append('sender=?' if filters['direction'] == 'sent' else 'EXISTS(SELECT 1 FROM deliveries d WHERE d.message=e.id AND d.recipient=?)')
            args.append(session)
        if 'topic' in filters:
            conditions.append('EXISTS(SELECT 1 FROM subscriptions t WHERE t.session=e.id AND t.topic=?)' if name == 'subscriptions' else 'topic=?')
            args.append(filters['topic'])
        if isinstance(filters.get('acknowledged'), bool):
            conditions.append('acknowledgedAt IS ' + ('NOT ' if filters['acknowledged'] else '') + 'NULL')
        clauses = ' WHERE ' + ' AND '.join(conditions) if conditions else ''
        rows = query(self.db, 'SELECT * FROM ({}) e{} ORDER BY {} LIMIT 101'.format(sql, clauses, 'message,recipient' if order == 'pair' else 'id'), args)
        return page([decode(name, row) for row in rows], True, 'entity list ' + name, filters)

    def entity_set(self, session, name, id, input):
        if id != session:
            raise ValueError('Only the bound session can be updated')
        definition = catalog.entity(name)
        if definition.get('set') is None:
            raise ValueError('Use dedicated transitions for leases, messages and deliveries')
        catalog.validate(definition['set'], input)
        if name == 'subscriptions':
            self.call(session, 'subscribe', input)
            return self.entity_get(session, name, id)
        with transaction(self.db):
            identity = self.known(session)
            for field in ('name', 'vendorSession', 'task', 'status'):
                if field in input:
                    if input[field] is not None and field != 'task':
                        catalog.text(input, field)
                    if field == 'vendorSession':
                        self.validate_vendor_session_update(session, identity, input[field])
                    execute(self.db, 'UPDATE sessions SET {}=? WHERE id=?'.format(field), (input[field], id))
            return self.entity_get(session, name, id)


def decode(name, row):
    from .store import strip_nulls
    row.pop('pathKey', None)
    if name == 'audit':
        row['data'] = strip_nulls(json.loads(row['data']))
    if name == 'subscriptions':
        row['topics'] = json.loads(row['topics'])
    return row
