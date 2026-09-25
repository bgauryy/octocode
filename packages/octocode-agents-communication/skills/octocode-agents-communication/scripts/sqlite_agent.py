#!/usr/bin/env python3
"""Minimal direct-SQL v6 agent. Run agents-communication db protocol for the complete contract.

Usage: python sqlite_agent.py DATABASE WORKSPACE OPERATION JSON
Requires SQLite >=3.51.3. Initialize the store once with the package CLI.
This example supports direct messages, notify_all, and file/tree leases; it does not run models.
"""
import hashlib
import json
import os
import re
import sqlite3
import sys
import time
import uuid
import unicodedata
from pathlib import Path

APPLICATION_ID = 1329678147
SCHEMA_VERSION = 6
SCHEMA_SHA256 = '25ea31fe4a10046cc11bd6c8785891fcb00f2e237b951d436d6246949052a73f'


def now():
    return time.time_ns() // 1_000_000


def text(value, maximum=256):
    if not isinstance(value, str) or not value.strip() or len(value.encode('utf-16-le')) // 2 > maximum:
        raise ValueError('Invalid text')
    return value


def reasoning(data):
    value = text(data.get('reasoning'), 512)
    if len(value.encode('utf-8')) > 512:
        raise ValueError('reasoning exceeds 512 UTF-8 bytes')
    return value


def ttl(value, default):
    value = default if value is None else value
    if type(value) is not int or not 1000 <= value <= 86400000:
        raise ValueError('Invalid TTL')
    return value


def contains(parent, child):
    return Path(child).is_relative_to(Path(parent))


def lease_parts(path):
    # Match the native Unicode 16 canonical-caseless lease namespace.
    if unicodedata.unidata_version != '16.0.0':
        raise RuntimeError('Path leases require Unicode 16.0.0 (Python 3.14); other operations remain available')
    return tuple(unicodedata.normalize('NFD', unicodedata.normalize('NFD', part).casefold())
                 for part in Path(path).parts)


def overlaps(a, ak, b, bk):
    a, b = lease_parts(a), lease_parts(b)
    return a == b or (ak == 'tree' and b[:len(a)] == a) or (bk == 'tree' and a[:len(b)] == b)


def page(rows):
    items, size = [], 64
    for row in rows[:100]:
        length = len(json.dumps(row, separators=(',', ':'), ensure_ascii=False).encode()) + 1
        if items and size + length > 256 * 1024:
            break
        size += length
        items.append(row)
    return {'items': items, 'next': items[-1]['id'] if len(rows) > len(items) else None}


def main(database, workspace, operation, data):
    if sqlite3.sqlite_version_info < (3, 51, 3):
        raise RuntimeError('SQLite >=3.51.3 required for concurrent WAL')
    workspace = str(Path(workspace).resolve(strict=True))
    # mode=rw prevents a typo from creating a second, disconnected store.
    db = sqlite3.connect(Path(database).resolve().as_uri() + '?mode=rw', uri=True, timeout=5, isolation_level=None)
    db.row_factory = sqlite3.Row
    try:
        rows = [dict(row) for row in db.execute("SELECT type,name,tbl_name,sql FROM sqlite_schema WHERE name NOT LIKE 'sqlite_%' ORDER BY type,name")]
        for row in rows:
            row['sql'] = re.sub(r'\s+', ' ', row['sql']).strip()
        fingerprint = hashlib.sha256(json.dumps(rows, separators=(',', ':'), ensure_ascii=False).encode()).hexdigest()
        if (db.execute('PRAGMA application_id').fetchone()[0] != APPLICATION_ID
                or db.execute('PRAGMA user_version').fetchone()[0] != SCHEMA_VERSION
                or fingerprint != SCHEMA_SHA256):
            raise RuntimeError('Incompatible database schema')
        db.execute('PRAGMA foreign_keys=ON')
        db.execute('PRAGMA synchronous=FULL')
        if db.execute('PRAGMA journal_mode').fetchone()[0] != 'wal':
            raise RuntimeError('Expected a CLI-initialized WAL database')
        db.execute('BEGIN IMMEDIATE' if operation != 'inbox' else 'BEGIN')
        stamp = now()
        session = data.get('session')
        if operation not in ('join', 'resume'):
            if not db.execute('SELECT 1 FROM sessions WHERE id=? AND workspace=? AND expiresAt>?', (session, workspace, stamp)).fetchone():
                raise ValueError('Unknown or expired session')
        if operation == 'join':
            session = str(uuid.uuid4())
            db.execute('INSERT INTO sessions(id,workspace,name,vendor,vendorSession,expiresAt) VALUES(?,?,?,?,?,?)',
                       (session, workspace, text(data['name']), text(data['vendor']), None, stamp + 60000))
            result = {'id': session}
        elif operation == 'resume':
            row = db.execute('SELECT * FROM sessions WHERE id=? AND workspace=? AND vendor=?', (session, workspace, data['vendor'])).fetchone()
            if row is None or row['expiresAt'] > stamp:
                raise ValueError('Session missing, vendor mismatch, or still active')
            db.execute('DELETE FROM leases WHERE owner=?', (session,))
            db.execute('UPDATE deliveries SET claimUntil=0,claimedBy=NULL WHERE recipient=? AND acknowledgedAt IS NULL', (session,))
            db.execute('UPDATE sessions SET expiresAt=? WHERE id=?', (stamp + 60000, session))
            result = {'id': session}
        elif operation == 'heartbeat':
            db.execute('UPDATE sessions SET expiresAt=? WHERE id=?', (stamp + 60000, session))
            result = {'alive': True}
        elif operation == 'leave':
            db.execute('DELETE FROM leases WHERE owner=?', (session,))
            db.execute('UPDATE sessions SET expiresAt=? WHERE id=?', (stamp, session))
            result = {'left': True}
        elif operation == 'lock':
            intent = reasoning(data)
            raw = str(Path(workspace) / text(data['path'], 4096))
            lease_parts(raw)
            path = os.path.realpath(raw, strict=os.path.ALLOW_MISSING)
            kind = data.get('kind', 'file')
            if kind not in ('file', 'tree') or not contains(workspace, path):
                raise ValueError('Invalid kind or path outside workspace')
            expiry = stamp + ttl(data.get('ttlMs'), 60000)
            conflict = None
            for row in db.execute('SELECT l.* FROM leases l JOIN sessions s ON s.id=l.owner WHERE l.workspace=? AND l.expiresAt>? AND s.expiresAt>?', (workspace, stamp, stamp)):
                if overlaps(row['path'], row['kind'], path, kind):
                    conflict = dict(row)
                    break
            if conflict:
                result = {'ok': False, 'conflict': conflict}
            else:
                row = db.execute('INSERT INTO leases(workspace,path,kind,owner,expiresAt,reasoning) VALUES(?,?,?,?,?,?)', (workspace, path, kind, session, expiry, intent))
                result = {'ok': True, 'lease': {'id': row.lastrowid, 'path': path, 'kind': kind, 'owner': session, 'expiresAt': expiry, 'reasoning': intent}}
        elif operation in ('renew', 'unlock'):
            if operation == 'renew':
                cursor = db.execute('UPDATE leases SET expiresAt=? WHERE id=? AND owner=? AND expiresAt>?', (stamp + ttl(data.get('ttlMs'), 60000), data['lease'], session, stamp))
            else:
                cursor = db.execute('DELETE FROM leases WHERE id=? AND owner=? AND expiresAt>?', (data['lease'], session, stamp))
            result = {('renewed' if operation == 'renew' else 'released'): cursor.rowcount == 1}
        elif operation in ('send_message', 'notify_all'):
            intent = reasoning(data)
            broadcast = operation == 'notify_all'
            ack_reply = data.get('ackReply', False)
            if type(ack_reply) is not bool or (broadcast and 'ackReply' in data) or (ack_reply and ('topic' in data or 'replyTo' not in data)):
                raise ValueError('ackReply requires a direct replyTo to an incoming message')
            wake = data.get('wake', 'passive' if broadcast else 'action')
            if wake not in ('action', 'passive'): raise ValueError('Invalid wake intent')
            if 'topic' in data or (broadcast and 'to' in data):
                raise ValueError('Use a direct target for send_message, or no target for notify_all')
            target = '*' if broadcast else (text(data['to']) if 'to' in data else None)
            body, key = text(data['body'], 16384), text(data.get('key', str(uuid.uuid4())))
            expiry = stamp + ttl(data.get('ttlMs'), 3600000)
            conversation, reply = data.get('conversationId'), data.get('replyTo')
            if 'conversationId' in data and (not isinstance(conversation, str) or not re.fullmatch(r'[A-Za-z0-9._:-]{1,128}', conversation)):
                raise ValueError('Invalid conversationId')
            if 'replyTo' in data:
                if type(reply) is not int or not 1 <= reply <= 9007199254740991:
                    raise ValueError('Invalid replyTo')
                parent = db.execute('SELECT m.conversationId,m.sender FROM messages m JOIN sessions s ON s.id=m.sender WHERE m.id=? AND s.workspace=? AND (m.sender=? OR EXISTS(SELECT 1 FROM deliveries d WHERE d.message=m.id AND d.recipient=?))', (reply, workspace, session, session)).fetchone()
                if parent is None:
                    raise ValueError('Reply requires a visible parent in this workspace')
                if 'conversationId' in data and conversation != parent['conversationId']:
                    raise ValueError('Reply conversationId must match its parent')
                conversation = parent['conversationId']
                if target is None:
                    target = parent['sender']
            if target is None:
                raise ValueError('Supply to or replyTo for a direct message')
            if ack_reply and (target != parent['sender'] or not db.execute(
                    'SELECT 1 FROM deliveries WHERE message=? AND recipient=?', (reply, session)).fetchone()):
                raise ValueError('ackReply requires replying to the sender of a message received by this session')
            previous = db.execute('SELECT * FROM messages WHERE sender=? AND key=?', (session, key)).fetchone()
            if previous:
                if previous['target'] != target or previous['body'] != body or previous['topic'] is not None or previous['reasoning'] != intent or previous['wake'] != wake or previous['conversationId'] != conversation or previous['replyTo'] != reply:
                    raise ValueError('Message key reused with different content')
                count = db.execute('SELECT count(*) FROM deliveries WHERE message=?', (previous['id'],)).fetchone()[0]
                result = {'id': previous['id'], 'recipients': count}
            else:
                if broadcast:
                    recipients = [r[0] for r in db.execute('SELECT id FROM sessions WHERE workspace=? AND expiresAt>? AND id<>?', (workspace, stamp, session))]
                else:
                    if not db.execute('SELECT 1 FROM sessions WHERE id=? AND workspace=?', (target, workspace)).fetchone():
                        raise ValueError('Unknown recipient in this workspace')
                    recipients = [target]
                message = db.execute('INSERT INTO messages(sender,target,topic,body,key,expiresAt,reasoning,wake,conversationId,replyTo) VALUES(?,?,NULL,?,?,?,?,?,?,?)', (session, target, body, key, expiry, intent, wake, conversation, reply)).lastrowid
                db.executemany('INSERT INTO deliveries(message,recipient) VALUES(?,?)', [(message, recipient) for recipient in recipients])
                result = {'id': message, 'recipients': len(recipients)}
            if ack_reply:
                db.execute('UPDATE deliveries SET acknowledgedAt=? WHERE message=? AND recipient=? AND acknowledgedAt IS NULL', (stamp, reply, session))
                result['acknowledged'] = True
        elif operation == 'inbox':
            rows = [dict(row) for row in db.execute('SELECT m.id,m.sender,m.body,m.reasoning,m.topic,m.expiresAt,m.wake,m.conversationId,m.replyTo FROM messages m JOIN deliveries d ON d.message=m.id WHERE d.recipient=? AND d.acknowledgedAt IS NULL AND m.expiresAt>? AND m.id>? ORDER BY m.id LIMIT 101', (session, stamp, data.get('after', 0)))]
            result = page(rows)
        elif operation == 'ack':
            row = db.execute('UPDATE deliveries SET acknowledgedAt=coalesce(acknowledgedAt,?) WHERE message=? AND recipient=?', (stamp, data['message'], session))
            result = {'acknowledged': row.rowcount == 1}
        else:
            raise ValueError('Unknown operation')
        db.execute('COMMIT')
        return result
    finally:
        # Closing also rolls back any failed transaction.
        db.close()


if __name__ == '__main__':
    print(json.dumps(main(sys.argv[1], sys.argv[2], sys.argv[3], json.loads(sys.argv[4]))))
