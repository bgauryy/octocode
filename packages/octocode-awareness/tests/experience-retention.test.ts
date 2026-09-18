import { mkdtempSync, realpathSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { expect, it } from 'vitest';
import { openAwarenessStore } from '../src/coordination/open.js';

it('does not erase canonical experience records when pruning delivered events', () => {
  const workspace = realpathSync(mkdtempSync(join(tmpdir(), 'experience-retention-')));
  const store = openAwarenessStore({ workspace, dbPath: join(workspace, 'ledger.sqlite3') });
  try {
    for (const kind of ['experience', 'task']) {
      store.appendEvent({
        version: 1, eventId: `event-${kind}`, workspace, type: `${kind}.record`,
        aggregate: { kind, id: 'example' }, retentionClass: kind === 'experience' ? 'audit' : 'delivery',
        actor: { kind: 'agent', id: 'writer' }, provenance: { source: 'tool', trust: 'attributed-data' },
        createdAt: '2026-09-13T00:00:00Z', payload: { title: 'Preserve experience' },
      });
    }
    const events = store.listEvents({ consumerId: 'host' });
    for (const event of events) store.acknowledgeEvent({ consumerId: 'host', eventId: event.eventId, decision: 'accept' });
    const throughSequence = events.at(-1)!.sequence;
    expect(store.pruneEvents({ throughSequence })).toMatchObject({ matched: 1, deleted: 0 });
    expect(store.pruneEvents({ throughSequence, dryRun: false })).toMatchObject({ matched: 1, deleted: 1 });
    expect(store.listEvents({ consumerId: 'audit' }).map(event => event.eventId)).toEqual(['event-experience']);
  } finally { store.close(); rmSync(workspace, { recursive: true, force: true }); }
});
