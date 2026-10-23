/** Bounded changes derived from sessions, never a second agent registry. */
import { execute, query, rowText, transaction, type Row } from './database.js';
import type { StoreHost } from './store-types.js';
import { now } from './timing.js';
import { asObjects, jsonEqual, parseObject, type JsonObject } from './validation.js';
import { compact, dumps } from './json-text.js';

/** The other live peers of the repository, bounded, and how many there are. */
interface PeerSnapshot {
  items: Row[];
  next: string | null;
  summary: Row;
}

export function peerSnapshot(host: StoreHost, session: string): PeerSnapshot {
  const at = now();
  const scopes = dumps(host.sharedWorkspaces());
  const rows = query(host.db, 'SELECT id,workspace,branch,name,vendor,task,status FROM sessions WHERE workspace IN (SELECT value FROM json_each(?)) AND id<>? AND expiresAt>? ORDER BY id LIMIT 17', [scopes, session, at]);
  const items: Row[] = [];
  let size = 0;
  let more = false;
  for (const row of rows) {
    const length = Buffer.byteLength(compact(row));
    if (items.length === 16 || (items.length && size + length > 3000)) {
      more = true;
      break;
    }
    size += length;
    items.push(row);
  }
  const summary = query(host.db, 'SELECT coalesce((SELECT revision FROM peer_revisions WHERE workspace=?2),0) AS revision,count(*) AS active FROM sessions WHERE expiresAt>?1 AND workspace IN (SELECT value FROM json_each(?4)) AND id<>?3', [at, host.coordinationScope, session, scopes])[0]!;
  return { items, next: more && items.length ? rowText(items.at(-1)!, 'id') : null, summary };
}

function savedPeers(host: StoreHost, session: string, consumer: string, generation: string): JsonObject | null {
  const rows = query(host.db, 'SELECT snapshot FROM peer_views WHERE session=? AND consumer=? AND generation=?', [session, consumer, generation]);
  return rows.length ? parseObject(rowText(rows[0]!, 'snapshot'), 'peer snapshot') : null;
}

export function peersChanged(host: StoreHost, session: string, consumer: string, generation: string): boolean {
  return !jsonEqual(peerSnapshot(host, session), savedPeers(host, session, consumer, generation));
}

export function peerContext(host: StoreHost, session: string, consumer: string, generation: string): string {
  if (!peersChanged(host, session, consumer, generation)) return '';
  return transaction(host.db, () => {
    const snapshot = peerSnapshot(host, session);
    const saved = savedPeers(host, session, consumer, generation);
    if (jsonEqual(snapshot, saved)) return '';
    execute(host.db, 'INSERT INTO peer_views(session,consumer,generation,snapshot) VALUES(?,?,?,?) ON CONFLICT(session,consumer) DO UPDATE SET generation=excluded.generation,snapshot=excluded.snapshot', [session, consumer, generation, compact(snapshot)]);
    const previous: JsonObject = saved ?? {};
    const before = asObjects(previous.items ?? [], 'peer items');
    const current = snapshot.items;
    const changed = current.filter(row => !before.some(old => jsonEqual(old, row)));
    const removed = before.filter(row => !current.some(entry => entry.id === row.id)).map(row => row.id);
    if (!changed.length && !removed.length && !snapshot.next && !previous.next) return '';
    const delta = {
      upsert: changed, removedFromView: removed,
      ...(snapshot.next ? { next: { command: 'peers', input: { after: snapshot.next } } } : {}),
      ...(previous.next ? { refresh: { command: 'peers', input: {} } } : {}),
    };
    return 'Peer directory (declared data; use exact IDs; removedFromView is not proof of departure; follow next): ' + compact(delta);
  });
}
