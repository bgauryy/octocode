"""Bound workspace/session operations over the shared coordination protocol."""
import json
import os
import stat
import uuid
from pathlib import Path
from . import catalog, database
from .database import query, execute, transaction
from .leases import LeaseMixin
from .records import RecordsMixin
from .health import HealthMixin
from .completion import CompletionMixin
from .documents import DocumentsMixin
from .dispatch import DeliveryMixin, now
from .peers import PeersMixin
from .workspace import coordination_scope, stored_scope, scope_workspaces

PAGE_BYTES = 16 * 1024



def current_branch(workspace):
    import subprocess
    try:
        result = subprocess.run(['git', '-C', workspace, 'symbolic-ref', '--quiet', '--short', 'HEAD'],
                                stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True, timeout=2,
                                env={key: value for key, value in os.environ.items() if not key.startswith('GIT_')})
        return (result.stdout.strip() or None) if result.returncode == 0 else None
    except (OSError, subprocess.TimeoutExpired):
        return None


def strip_nulls(value):
    if isinstance(value, dict):
        if all(key in value for key in ('path', 'from', 'type', 'timestamp', 'data')):
            return value
        for key in list(value):
            if value[key] is None:
                del value[key]
            else:
                strip_nulls(value[key])
    elif isinstance(value, list):
        for item in value:
            strip_nulls(item)
    return value


def page(rows, string_cursor, command, input, metadata_bytes=0):
    size, count = 64 + metadata_bytes, 0
    for row in rows[:100]:
        added = len(json.dumps(row, ensure_ascii=False, separators=(',', ':')).encode()) + 1
        if count > 0 and size + added > PAGE_BYTES:
            break
        size += added
        count += 1
    result = {'items': rows[:count]}
    if len(rows) > count and count:
        last = rows[count-1]
        cursor = last['recordId'] if 'recordId' in last else last['id']
        result['next'] = {'command': command, 'input': dict(input, after=str(cursor) if string_cursor else cursor)}
    if size > PAGE_BYTES:
        result['budget'] = {'targetBytes': PAGE_BYTES, 'reason': 'Single oversized row returned intact to preserve evidence and cursor progress'}
    return result


class Store(LeaseMixin, RecordsMixin, HealthMixin, CompletionMixin, DocumentsMixin, DeliveryMixin, PeersMixin):
    def __init__(self, database, workspace, read_only=False, create=False):
        self.workspace = str(Path(workspace).resolve(strict=True))
        self.coordination_scope = coordination_scope(self.workspace)
        self.database = Path(database)
        self.db = globals()['database'].open(self.database, read_only, create)
        self.database_identity = self.database.stat()
        self.writer = not read_only
        self.idle_inbox = None

    def close(self):
        if self.writer:
            try:
                self.db.execute('PRAGMA optimize')
            except Exception:
                pass
        self.db.close()

    def check_database(self):
        try:
            current = self.database.stat()
        except OSError:
            raise ValueError('Coordination database disappeared; stop this worker')
        if not stat.S_ISREG(current.st_mode):
            raise ValueError('Coordination database is no longer a file; stop this worker')
        if (current.st_dev, current.st_ino) != (self.database_identity.st_dev, self.database_identity.st_ino):
            raise ValueError('Coordination database was replaced; stop this worker')

    def known(self, id, active=True):
        self.check_database()
        if stored_scope(self.db, self.workspace) != self.coordination_scope:
            raise ValueError('Workspace repository changed since this path first joined this database; bind this path to a new --database file (existing records stay readable in the old one)')
        rows = query(self.db, 'SELECT * FROM sessions WHERE id=? AND workspace=? AND (?=0 OR expiresAt>?)', (id, self.workspace, active, now()))
        if not rows:
            raise ValueError('Unknown or expired session in this workspace. Refresh peers and copy the exact current ID; never reconstruct it. If your bound identity expired, its lifecycle owner resumes it (a raw CLI agent runs resume {"vendor":...} itself), then acquire fresh leases before writing.')
        return rows[0]

    def peer(self, id):
        rows = query(self.db, 'SELECT * FROM sessions WHERE id=?', (id,))
        if not rows or stored_scope(self.db, rows[0]['workspace']) != self.coordination_scope:
            raise ValueError('Unknown session in this repository. Refresh peers and use its exact ID.')
        return rows[0]

    def shared_workspaces(self):
        return scope_workspaces(self.db, self.coordination_scope, self.workspace)

    def live_names(self, exclude=''):
        return {row['name'] for row in query(self.db, 'SELECT name FROM sessions WHERE workspace IN (SELECT value FROM json_each(?)) AND expiresAt>? AND id<>?', (json.dumps(self.shared_workspaces()), now(), exclude))}

    def unique_name(self, name, exclude=''):
        """Live peers in one repository scope have distinct names, so a name can address a peer.
        A taken name gets the first free -2, -3 … suffix, as Claude Code does for sessions."""
        taken, candidate, number = self.live_names(exclude), name, 2
        while candidate in taken:
            suffix = '-' + str(number)
            candidate, number = name[:256 - len(suffix)] + suffix, number + 1
        return candidate

    def resolve_peer(self, value):
        """Map a `to` value to a session ID: an exact ID, else a unique live peer name,
        else the only session ever named so in this repository."""
        if query(self.db, 'SELECT 1 FROM sessions WHERE id=?', (value,)):
            return value
        scope = json.dumps(self.shared_workspaces())
        for live in (True, False):
            rows = query(self.db, 'SELECT id FROM sessions WHERE name=? AND workspace IN (SELECT value FROM json_each(?)) AND (?=0 OR expiresAt>?) ORDER BY id LIMIT 2', (value, scope, live, now()))
            if len(rows) == 1:
                return rows[0]['id']
            if rows:
                raise ValueError('Name {} matches several {}peers; use the exact ID from peers'.format(value, 'live ' if live else ''))
        raise ValueError('Unknown peer {}. Use a peer name or exact ID from peers.'.format(value))

    def host_identities(self, vendor, host):
        return query(self.db, 'SELECT s.id FROM sessions s LEFT JOIN attachments a ON a.session=s.id WHERE s.workspace=? AND s.vendorSession=? AND (s.vendor=? OR a.transport=?) ORDER BY s.id LIMIT 2', (self.workspace, host, vendor, vendor))

    def call(self, session, name, input):
        self.check_database()
        catalog.command(name, input)
        if name in ('fetch', 'record'):
            return getattr(self, name)(session, input)
        if name == 'binding':
            identity = self.known(session, False)
            return dict(identity, coordinationScope=self.coordination_scope, active=identity['expiresAt'] > now())
        if name == 'health':
            return self.health(input)
        if name == 'activity':
            self.known(session, False)
            from .activity import read
            return read(Path(self.workspace), input)
        if name == 'peers':
            return page(query(self.db, 'SELECT id,workspace,(SELECT coordinationScope FROM workspaces w WHERE w.workspace=sessions.workspace) AS coordinationScope,name,vendor,vendorSession,branch,task,status,expiresAt FROM sessions WHERE workspace IN (SELECT value FROM json_each(?)) AND expiresAt>? AND id>? ORDER BY id LIMIT 101', (json.dumps(self.shared_workspaces()), now(), input.get('after', ''))), True, name, input)
        if name == 'inbox':
            if 'message' not in input:
                return self.inbox(session, input.get('after', 0))
            self.known(session)
            return page(query(self.db, 'SELECT m.id,m.sender,s.name AS senderName,m.body,m.reasoning,m.topic,m.expiresAt,m.wake,m.conversationId,m.replyTo,m.replyRequired FROM deliveries d JOIN messages m ON m.id=d.message JOIN sessions s ON s.id=m.sender WHERE d.recipient=? AND d.acknowledgedAt IS NULL AND m.expiresAt>? AND m.id=?', (session, now(), input['message'])), False, name, input)
        if name == 'join':
            branch = input['branch'] if 'branch' in input else current_branch(self.workspace)
            with transaction(self.db):
                execute(self.db, 'INSERT OR IGNORE INTO workspaces(workspace,coordinationScope) VALUES(?,?)', (self.workspace, self.coordination_scope))
                if stored_scope(self.db, self.workspace) != self.coordination_scope:
                    raise ValueError('Workspace repository changed since this path first joined this database; bind this path to a new --database file (existing records stay readable in the old one)')
                id = str(uuid.uuid4())
                human_name, vendor = self.unique_name(catalog.text(input, 'name')), catalog.text(input, 'vendor')
                if 'vendorSession' in input:
                    catalog.text(input, 'vendorSession')
                expires = now() + 60000
                execute(self.db, 'INSERT INTO sessions(id,workspace,name,vendor,vendorSession,branch,expiresAt,task,status) VALUES(?,?,?,?,?,?,?,?,?)', (id, self.workspace, human_name, vendor, input.get('vendorSession'), branch, expires, input.get('task', ''), input.get('status', 'unknown')))
                return strip_nulls(dict(id=id, name=human_name, vendor=vendor, vendorSession=input.get('vendorSession'), expiresAt=expires))
        if name in ('resume', 'heartbeat', 'set_status', 'leave', 'subscribe', 'prune'):
            with transaction(self.db):
                if name == 'resume':
                    row = self.known(session, False)
                    if row['vendor'] != catalog.text(input, 'vendor') or row['expiresAt'] > now():
                        raise ValueError('Vendor mismatch or session still active')
                    execute(self.db, 'DELETE FROM leases WHERE owner=?', (session,))
                    execute(self.db, 'DELETE FROM lease_waits WHERE owner=?', (session,))
                    execute(self.db, 'UPDATE deliveries SET claimUntil=0,claimedBy=NULL WHERE recipient=? AND acknowledgedAt IS NULL', (session,))
                    execute(self.db, 'UPDATE sessions SET expiresAt=?,name=? WHERE id=?', (now()+60000, self.unique_name(row['name'], session), session))
                    self.grant_waits(now())
                    return self.known(session)
                if name == 'heartbeat':
                    identity = self.known(session)
                    if 'vendorSession' in input:
                        catalog.text(input, 'vendorSession')
                        self.validate_vendor_session_update(session, identity, input['vendorSession'])
                    at = now()
                    if identity['expiresAt'] <= at:
                        raise ValueError('Session expired before heartbeat; stop this worker')
                    expires = at + catalog.ttl(input, 60000, 600000)
                    execute(self.db, 'UPDATE sessions SET expiresAt=?,vendorSession=coalesce(?,vendorSession),task=coalesce(?,task),status=coalesce(?,status) WHERE id=?', (expires, input.get('vendorSession'), input.get('task'), input.get('status'), session))
                    if 'name' in input:
                        if catalog.text(input, 'name') in self.live_names(session):
                            raise ValueError('Name {} belongs to a live peer; choose another name'.format(input['name']))
                        execute(self.db, 'UPDATE sessions SET name=? WHERE id=?', (input['name'], session))
                    if 'branch' in input:
                        execute(self.db, 'UPDATE sessions SET branch=? WHERE id=?', (input['branch'], session))
                    result = {'alive': True, 'expiresAt': expires}
                    if input.get('renewLeases'):
                        result['renewedLeases'] = execute(self.db, 'UPDATE leases SET expiresAt=?,refreshedAt=? WHERE owner=? AND workspace=? AND expiresAt>? AND expiresAt<?', (expires, at, session, self.workspace, at, expires))
                    # Periodic heartbeats hand silently expired leases to waiters.
                    self.grant_waits(at)
                    return result
                if name == 'set_status':
                    self.known(session)
                    execute(self.db, 'UPDATE sessions SET task=coalesce(?,task),status=coalesce(?,status) WHERE id=?', (input.get('task'), input.get('status'), session))
                    row = self.known(session)
                    return {k: row[k] for k in ('id', 'task', 'status')}
                if name == 'leave':
                    self.known(session, False)
                    execute(self.db, 'DELETE FROM leases WHERE owner=?', (session,))
                    execute(self.db, 'DELETE FROM lease_waits WHERE owner=?', (session,))
                    execute(self.db, 'DELETE FROM subscriptions WHERE session=?', (session,))
                    execute(self.db, 'UPDATE deliveries SET claimUntil=0,claimedBy=NULL WHERE recipient=? AND acknowledgedAt IS NULL', (session,))
                    execute(self.db, 'UPDATE sessions SET expiresAt=? WHERE id=?', (now(), session))
                    self.grant_waits(now())
                    return {'left': True}
                if name == 'subscribe':
                    self.known(session)
                    topics = input['topics']
                    for topic in topics:
                        catalog.text({'topic': topic}, 'topic')
                    execute(self.db, 'DELETE FROM subscriptions WHERE session=? AND topic NOT IN (SELECT value FROM json_each(?))', (session, json.dumps(topics)))
                    for topic in topics:
                        execute(self.db, 'INSERT OR IGNORE INTO subscriptions VALUES(?,?)', (session, topic))
                    return {'subscribed': True}
                at = now()
                removed = execute(self.db, 'DELETE FROM leases WHERE id IN (SELECT l.id FROM leases l JOIN sessions s ON s.id=l.owner WHERE l.workspace=? AND (l.expiresAt<=? OR s.expiresAt<=?) ORDER BY l.id LIMIT 100)', (self.workspace, at, at))
                self.grant_waits(at)
                return dict(removed=removed, **({'next': {'command': 'prune', 'input': {}}} if removed == 100 else {}))
        if name in ('check_write', 'check_paths', 'share_document', 'read_document', 'context'):
            return getattr(self, name)(session, input)
        if name == 'locks':
            return self.locks(session, input)
        if name in ('lock', 'lock_many'):
            return self.lock(session, input, name == 'lock_many')
        if name in ('renew', 'unlock'):
            return self.lease_transition(session, input, name == 'renew')
        if name in ('send_message', 'notify_all'):
            return self.send(session, input, name == 'notify_all', False)
        if name == 'complete':
            return self.complete(session, input)
        raise ValueError('Unknown operation: ' + name)

    def validate_vendor_session_update(self, session, identity, value):
        if identity.get('vendorSession') != value:
            self.ensure_binding_change_allowed(session)
            if query(self.db, "SELECT 1 FROM attachments WHERE session=? AND transport<>'raw'", (session,)):
                raise ValueError("Use attach to change a native receiver's vendorSession")

    def complete(self, session, input):
        if 'reply' in input:
            return self.send(session, dict(replyTo=input['message'], body=input['reply'], reasoning=input.get('reasoning', 'Complete received message'), key='complete:{}'.format(input['message'])), False, True)
        with transaction(self.db):
            self.known(session)
            messages = input.get('messages', [input.get('message')])
            required = query(self.db, 'SELECT m.id FROM deliveries d JOIN messages m ON m.id=d.message WHERE d.recipient=? AND d.acknowledgedAt IS NULL AND m.replyRequired=1 AND m.id IN (SELECT value FROM json_each(?)) LIMIT 1', (session, json.dumps(messages)))
            if required:
                raise ValueError('Message {} requires a final answer: use complete with message and reply, or leave unfinished work pending'.format(required[0]['id']))
            at = now()
            for message in messages:
                count = execute(self.db, 'UPDATE deliveries SET acknowledgedAt=coalesce(acknowledgedAt,?) WHERE message=? AND recipient=?', (at, message, session))
                if count != 1:
                    raise ValueError('Completion requires every ID to be received by this session')
            return dict(completed=True, count=len(messages))

    def send(self, session, input, broadcast=False, completing=False):
        if not completing and 'replyTo' in input:
            raise ValueError('Final replies require complete with message and reply')
        if not broadcast and (('to' in input and 'topic' in input) or not any(k in input for k in ('to', 'topic', 'replyTo'))):
            raise ValueError('Supply exactly one of to or topic. For replies use complete with message and reply.')
        target = '*' if broadcast else catalog.text(input, 'to') if 'to' in input else catalog.text(input, 'topic') if 'topic' in input else None
        body, reasoning = catalog.text(input, 'body'), catalog.text(input, 'reasoning')
        reply_required = input.get('replyRequired', not broadcast and 'topic' not in input and 'replyTo' not in input)
        if 'replyTo' in input and reply_required:
            raise ValueError('Replies are informational; use replyRequired:false. Start a new direct request for new work')
        duration = catalog.ttl(input, 3600000)
        wake = input.get('wake', 'passive' if broadcast or 'topic' in input else 'action')
        key = catalog.text(input, 'key') if 'key' in input else str(uuid.uuid4())
        if not completing and key.startswith('complete:'):
            raise ValueError('Keys beginning complete: are reserved for complete replies')
        with transaction(self.db):
            self.known(session)
            if 'to' in input and not broadcast:
                target = self.resolve_peer(target)
            conversation, reply_sender = input.get('conversationId'), None
            if 'replyTo' in input:
                parents = query(self.db, 'SELECT m.conversationId,m.sender,m.replyRequired FROM messages m JOIN sessions s ON s.id=m.sender WHERE m.id=? AND s.workspace IN (SELECT value FROM json_each(?)) AND (m.sender=? OR EXISTS(SELECT 1 FROM deliveries d WHERE d.message=m.id AND d.recipient=?))', (input['replyTo'], json.dumps(self.shared_workspaces()), session, session))
                if not parents:
                    raise ValueError('Unknown message {} for this session. Use data.messageId from fetch or item.id from inbox, never a recordId.'.format(input['replyTo']))
                parent = parents[0]
                if not parent['replyRequired']:
                    raise ValueError('Message {} is informational and accepts no reply. Handle it with complete using message or messages only; start a new direct request for new work'.format(input['replyTo']))
                if 'conversationId' in input and conversation != parent.get('conversationId'):
                    raise ValueError('Reply conversationId must match its parent')
                conversation, reply_sender = parent.get('conversationId'), parent['sender']
            target = target if target is not None else reply_sender
            if target is None:
                raise ValueError('Reply parent has no sender')
            if completing and (reply_sender != target or not query(self.db, 'SELECT 1 FROM deliveries WHERE message=? AND recipient=?', (input['replyTo'], session))):
                raise ValueError('Completion requires replying to a message received by this session')
            def receipt(id, count):
                result = dict(id=id, recipients=count)
                if completing:
                    execute(self.db, 'UPDATE deliveries SET acknowledgedAt=? WHERE message=? AND recipient=? AND acknowledgedAt IS NULL', (now(), input['replyTo'], session))
                    result.update(completed=True, count=1)
                return result
            rows = query(self.db, 'SELECT * FROM messages WHERE sender=? AND key=?', (session, key))
            if rows:
                row = rows[0]
                expected = dict(target=target, body=body, topic=input.get('topic'), reasoning=reasoning, wake=wake, conversationId=conversation, replyTo=input.get('replyTo'), replyRequired=reply_required, ttlMs=duration)
                if any(row.get(k) != v for k, v in expected.items()):
                    raise ValueError('Final reply already stored with different content; use send_message for new work' if completing else 'Message key reused with different content (target, topic, body, reasoning, wake, correlation or ttlMs); use a new key')
                count = query(self.db, 'SELECT count(*) AS n FROM deliveries WHERE message=?', (row['id'],))[0]['n']
                return receipt(row['id'], count)
            if completing and query(self.db, 'SELECT 1 FROM deliveries WHERE message=? AND recipient=? AND acknowledgedAt IS NOT NULL', (input['replyTo'], session)):
                raise ValueError('Message already completed without this reply; send_message for new work')
            offline = False
            if broadcast:
                recipients = query(self.db, 'SELECT id FROM sessions WHERE workspace IN (SELECT value FROM json_each(?)) AND expiresAt>? AND id<>?', (json.dumps(self.shared_workspaces()), now(), session))
            elif 'topic' not in input:
                offline = self.peer(target)['expiresAt'] <= now()
                recipients = [{'id': target}]
            else:
                recipients = query(self.db, 'SELECT s.id FROM sessions s JOIN subscriptions t ON s.id=t.session WHERE t.topic=? AND s.workspace IN (SELECT value FROM json_each(?)) AND s.expiresAt>? AND s.id<>?', (target, json.dumps(self.shared_workspaces()), now(), session))
            execute(self.db, 'INSERT INTO messages(sender,target,topic,body,key,expiresAt,reasoning,wake,conversationId,replyTo,ttlMs,replyRequired) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)', (session, target, input.get('topic'), body, key, now()+duration, reasoning, wake, conversation, input.get('replyTo'), duration, reply_required))
            id = self.db.execute('SELECT last_insert_rowid()').fetchone()[0]
            for recipient in recipients:
                execute(self.db, 'INSERT INTO deliveries(message,recipient) VALUES(?,?)', (id, recipient['id']))
            result = receipt(id, len(recipients))
            if offline:
                result['recipientOffline'] = True
            return result

    def inbox(self, session, after=0):
        self.known(session)
        state = (session, after, self.db.execute('PRAGMA data_version').fetchone()[0], self.db.total_changes)
        if self.idle_inbox == state:
            return {'items': []}
        rows = query(self.db, 'SELECT m.id,m.sender,s.name AS senderName,m.body,m.reasoning,m.topic,m.expiresAt,m.wake,m.conversationId,m.replyTo,m.replyRequired FROM deliveries d JOIN messages m ON m.id=d.message JOIN sessions s ON s.id=m.sender WHERE d.recipient=? AND d.acknowledgedAt IS NULL AND m.expiresAt>? AND d.message>? ORDER BY d.message LIMIT 101', (session, now(), after))
        result = page(rows, False, 'inbox', {'after': after})
        self.idle_inbox = state if not rows else None
        return result

