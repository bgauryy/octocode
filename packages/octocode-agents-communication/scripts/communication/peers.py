"""Bounded changes derived from sessions, never a second agent registry."""
import json
from .database import query, execute, transaction


def peer_snapshot(store, session):
    from .store import now
    scopes = json.dumps(store.shared_workspaces())
    rows = query(store.db, "SELECT id,workspace,branch,name,vendor,task,status FROM sessions WHERE workspace IN (SELECT value FROM json_each(?)) AND id<>? AND expiresAt>? ORDER BY id LIMIT 17", (scopes, session, now()))
    items, size, more = [], 0, False
    for row in rows:
        length = len(json.dumps(row, ensure_ascii=False, separators=(',', ':')).encode())
        if len(items) == 16 or (items and size + length > 3000):
            more = True
            break
        size += length
        items.append(row)
    summary = query(store.db, "SELECT coalesce((SELECT revision FROM peer_revisions WHERE workspace=?2),0) AS revision,count(*) AS active FROM sessions WHERE expiresAt>?1 AND workspace IN (SELECT value FROM json_each(?4)) AND id<>?3", (now(), store.coordination_scope, session, scopes))[0]
    return {'items': items, 'next': items[-1]['id'] if more and items else None, 'summary': summary}


def saved_peers(store, session, consumer, generation):
    rows = query(store.db, "SELECT snapshot FROM peer_views WHERE session=? AND consumer=? AND generation=?", (session, consumer, generation))
    return json.loads(rows[0]['snapshot']) if rows else None


def peers_changed(store, session, consumer, generation):
    return peer_snapshot(store, session) != saved_peers(store, session, consumer, generation)


def peer_context(store, session, consumer, generation):
    if not peers_changed(store, session, consumer, generation):
        return ''
    with transaction(store.db):
        snapshot = peer_snapshot(store, session)
        previous = saved_peers(store, session, consumer, generation)
        if snapshot == previous:
            return ''
        execute(store.db, "INSERT INTO peer_views(session,consumer,generation,snapshot) VALUES(?,?,?,?) ON CONFLICT(session,consumer) DO UPDATE SET generation=excluded.generation,snapshot=excluded.snapshot", (session, consumer, generation, json.dumps(snapshot, ensure_ascii=False, separators=(',', ':'))))
        previous = previous or {}
        before, current = previous.get('items', []), snapshot['items']
        changed = [row for row in current if row not in before]
        removed = [row['id'] for row in before if not any(new['id'] == row['id'] for new in current)]
        if not changed and not removed and not snapshot.get('next') and not previous.get('next'):
            return ''
        delta = {'upsert': changed, 'removedFromView': removed}
        if snapshot.get('next'):
            delta['next'] = {'command': 'peers', 'input': {'after': snapshot['next']}}
        if previous.get('next'):
            delta['refresh'] = {'command': 'peers', 'input': {}}
        return 'Peer directory (declared data; use exact IDs; removedFromView is not proof of departure; follow next): ' + json.dumps(delta, ensure_ascii=False, separators=(',', ':'))


class PeersMixin:
    peers_changed = peers_changed
    peer_context = peer_context
