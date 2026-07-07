/**
 * doc-staleness.test.ts — Behavioural tests for mineDocStaleness / proposeDocRefresh.
 *
 * edit_log.created_at has second-level precision (utcNow() strips milliseconds),
 * so tests that depend on ordering between a doc edit and later source edits seed
 * edit_log directly with explicit, well-separated ISO timestamps rather than relying
 * on wall-clock insertion order.
 *
 * Verifies:
 *  1. A doc with no source edits since its last sync is fresh.
 *  2. Source edits below both thresholds keep a doc fresh.
 *  3. Edit-count threshold alone flags a doc stale.
 *  4. Line-count threshold alone flags a doc stale.
 *  5. A doc never seen in edit_log treats all historical source activity as drift.
 *  6. Only edits under the configured sourceDirs prefixes count (unrelated files ignored).
 *  7. workspacePath scoping isolates unrelated workspaces.
 *  8. Multiple targets are evaluated independently in one call.
 *  9. proposeDocRefresh inserts a harness_log 'propose' row with the expected payload shape.
 * 10. Custom thresholds (minEditsSinceSync / minLinesSinceSync) are honored.
 */

import { describe, it, expect } from 'vitest';
import { DatabaseSync } from 'node:sqlite';
import { randomUUID } from 'node:crypto';
import { initDb } from '../src/db.js';
import { mineDocStaleness, proposeDocRefresh } from '../src/docs.js';
import type { HarnessLogRow } from '../src/types.js';

// ─── Helpers ──────────────────────────────────────────────────────────────────

function freshDb(): DatabaseSync {
  const db = new DatabaseSync(':memory:');
  db.exec('PRAGMA foreign_keys = ON');
  initDb(db);
  return db;
}

/** Insert an edit_log row at an explicit timestamp — avoids same-second ordering flakiness. */
function seedEdit(db: DatabaseSync, opts: {
  filePath: string;
  createdAt: string;
  linesAdded?: number | null;
  linesRemoved?: number | null;
  workspacePath?: string | null;
}): void {
  db.prepare(`
    INSERT INTO edit_log (edit_id, agent_id, file_path, operation, lines_added, lines_removed, workspace_path, created_at)
    VALUES (?, 'agent-test', ?, 'update', ?, ?, ?, ?)
  `).run(
    'edit_' + randomUUID(),
    opts.filePath,
    opts.linesAdded ?? null,
    opts.linesRemoved ?? null,
    opts.workspacePath ?? null,
    opts.createdAt,
  );
}

const DOC = '/repo/packages/foo/ARCHITECTURE.md';
const SRC = '/repo/packages/foo/src';

const T0 = '2026-01-01T00:00:00Z'; // doc last synced
const T1 = '2026-02-01T00:00:00Z'; // after sync
const T2 = '2026-02-02T00:00:00Z';

// ─── 1. No source edits since last sync ──────────────────────────────────────

describe('mineDocStaleness — no drift', () => {
  it('is fresh when all source edits happened before the doc was last synced', () => {
    const db = freshDb();
    seedEdit(db, { filePath: `${SRC}/index.ts`, createdAt: T0, linesAdded: 100, linesRemoved: 50 });
    seedEdit(db, { filePath: DOC, createdAt: T1 }); // doc synced after that source edit

    const result = mineDocStaleness(db, { targets: [{ docFile: DOC, sourceDirs: [SRC] }] });

    expect(result.ok).toBe(true);
    expect(result.checked).toBe(1);
    expect(result.stale_count).toBe(0);
    expect(result.entries[0]!.stale).toBe(false);
    expect(result.entries[0]!.edits_since_sync).toBe(0);
    expect(result.entries[0]!.doc_last_synced_at).toBe(T1);
  });

  it('is fresh when there is no source activity at all', () => {
    const db = freshDb();
    seedEdit(db, { filePath: DOC, createdAt: T0 });

    const result = mineDocStaleness(db, { targets: [{ docFile: DOC, sourceDirs: [SRC] }] });

    expect(result.stale_count).toBe(0);
    expect(result.entries[0]!.edits_since_sync).toBe(0);
    expect(result.entries[0]!.files_touched).toEqual([]);
  });
});

// ─── 2. Edits below both thresholds ───────────────────────────────────────────

describe('mineDocStaleness — activity below thresholds', () => {
  it('stays fresh when edits and lines are both below default thresholds', () => {
    const db = freshDb();
    seedEdit(db, { filePath: DOC, createdAt: T0 });
    // Default thresholds: 5 edits / 50 lines. This is 2 edits, 10 lines.
    seedEdit(db, { filePath: `${SRC}/a.ts`, createdAt: T1, linesAdded: 5 });
    seedEdit(db, { filePath: `${SRC}/b.ts`, createdAt: T2, linesAdded: 5 });

    const result = mineDocStaleness(db, { targets: [{ docFile: DOC, sourceDirs: [SRC] }] });

    expect(result.entries[0]!.stale).toBe(false);
    expect(result.entries[0]!.edits_since_sync).toBe(2);
    expect(result.entries[0]!.lines_changed_since_sync).toBe(10);
  });
});

// ─── 3. Edit-count threshold flags stale ─────────────────────────────────────

describe('mineDocStaleness — edit-count threshold', () => {
  it('flags stale once edit count meets the default threshold (5)', () => {
    const db = freshDb();
    seedEdit(db, { filePath: DOC, createdAt: T0 });
    for (let i = 0; i < 5; i++) {
      seedEdit(db, { filePath: `${SRC}/f${i}.ts`, createdAt: T1, linesAdded: 1 });
    }

    const result = mineDocStaleness(db, { targets: [{ docFile: DOC, sourceDirs: [SRC] }] });

    expect(result.entries[0]!.stale).toBe(true);
    expect(result.entries[0]!.edits_since_sync).toBe(5);
    expect(result.stale_count).toBe(1);
  });

  it('respects a custom minEditsSinceSync threshold', () => {
    const db = freshDb();
    seedEdit(db, { filePath: DOC, createdAt: T0 });
    seedEdit(db, { filePath: `${SRC}/a.ts`, createdAt: T1, linesAdded: 1 });
    seedEdit(db, { filePath: `${SRC}/b.ts`, createdAt: T2, linesAdded: 1 });

    const strict = mineDocStaleness(db, {
      targets: [{ docFile: DOC, sourceDirs: [SRC] }],
      minEditsSinceSync: 2,
      minLinesSinceSync: 1000, // disable the line-based trigger for this assertion
    });
    expect(strict.entries[0]!.stale).toBe(true);

    const lenient = mineDocStaleness(db, {
      targets: [{ docFile: DOC, sourceDirs: [SRC] }],
      minEditsSinceSync: 10,
      minLinesSinceSync: 1000,
    });
    expect(lenient.entries[0]!.stale).toBe(false);
  });
});

// ─── 4. Line-count threshold flags stale ─────────────────────────────────────

describe('mineDocStaleness — line-count threshold', () => {
  it('flags stale once cumulative lines changed meets the default threshold (50) even with few edits', () => {
    const db = freshDb();
    seedEdit(db, { filePath: DOC, createdAt: T0 });
    seedEdit(db, { filePath: `${SRC}/big.ts`, createdAt: T1, linesAdded: 40, linesRemoved: 20 });

    const result = mineDocStaleness(db, { targets: [{ docFile: DOC, sourceDirs: [SRC] }] });

    expect(result.entries[0]!.edits_since_sync).toBe(1);
    expect(result.entries[0]!.lines_changed_since_sync).toBe(60);
    expect(result.entries[0]!.stale).toBe(true);
  });
});

// ─── 5. Doc never tracked — all history counts as drift ──────────────────────

describe('mineDocStaleness — doc never recorded in edit_log', () => {
  it('treats all historical source activity as drift when the doc has no edit_log row', () => {
    const db = freshDb();
    for (let i = 0; i < 6; i++) {
      seedEdit(db, { filePath: `${SRC}/f${i}.ts`, createdAt: T1, linesAdded: 1 });
    }

    const result = mineDocStaleness(db, { targets: [{ docFile: DOC, sourceDirs: [SRC] }] });

    expect(result.entries[0]!.doc_last_synced_at).toBeNull();
    expect(result.entries[0]!.edits_since_sync).toBe(6);
    expect(result.entries[0]!.stale).toBe(true);
  });

  it('is fresh (not stale) when the doc was never tracked and there is no source activity either', () => {
    const db = freshDb();
    const result = mineDocStaleness(db, { targets: [{ docFile: DOC, sourceDirs: [SRC] }] });

    expect(result.entries[0]!.doc_last_synced_at).toBeNull();
    expect(result.entries[0]!.edits_since_sync).toBe(0);
    expect(result.entries[0]!.stale).toBe(false);
  });
});

// ─── 6. Only sourceDirs-prefixed files count ─────────────────────────────────

describe('mineDocStaleness — sourceDirs scoping', () => {
  it('ignores edits outside the configured sourceDirs', () => {
    const db = freshDb();
    seedEdit(db, { filePath: DOC, createdAt: T0 });
    for (let i = 0; i < 10; i++) {
      seedEdit(db, { filePath: `/repo/packages/other/src/f${i}.ts`, createdAt: T1, linesAdded: 100 });
    }

    const result = mineDocStaleness(db, { targets: [{ docFile: DOC, sourceDirs: [SRC] }] });

    expect(result.entries[0]!.edits_since_sync).toBe(0);
    expect(result.entries[0]!.stale).toBe(false);
  });

  it('does not match a sibling directory that merely shares a prefix (foo vs foo-bar)', () => {
    const db = freshDb();
    seedEdit(db, { filePath: DOC, createdAt: T0 });
    for (let i = 0; i < 10; i++) {
      seedEdit(db, { filePath: `/repo/packages/foo-bar/src/f${i}.ts`, createdAt: T1, linesAdded: 100 });
    }

    const result = mineDocStaleness(db, { targets: [{ docFile: DOC, sourceDirs: [SRC] }] });

    expect(result.entries[0]!.edits_since_sync).toBe(0);
  });
});

// ─── 7. workspacePath scoping ─────────────────────────────────────────────────

describe('mineDocStaleness — workspacePath scoping', () => {
  it('only counts edits from the given workspacePath', () => {
    const db = freshDb();
    seedEdit(db, { filePath: DOC, createdAt: T0, workspacePath: '/ws-a' });
    for (let i = 0; i < 5; i++) {
      seedEdit(db, { filePath: `${SRC}/f${i}.ts`, createdAt: T1, linesAdded: 1, workspacePath: '/ws-b' });
    }

    const scoped = mineDocStaleness(db, {
      targets: [{ docFile: DOC, sourceDirs: [SRC] }],
      workspacePath: '/ws-a',
    });
    // Edits tagged to a different workspace are excluded from this scoped view.
    expect(scoped.entries[0]!.edits_since_sync).toBe(0);

    const unscoped = mineDocStaleness(db, { targets: [{ docFile: DOC, sourceDirs: [SRC] }] });
    expect(unscoped.entries[0]!.edits_since_sync).toBe(5);
  });
});

// ─── 8. Multiple targets evaluated independently ─────────────────────────────

describe('mineDocStaleness — multiple targets', () => {
  it('evaluates each target independently in one call', () => {
    const db = freshDb();
    const DOC2 = '/repo/packages/bar/ARCHITECTURE.md';
    const SRC2 = '/repo/packages/bar/src';

    seedEdit(db, { filePath: DOC, createdAt: T0 });
    seedEdit(db, { filePath: DOC2, createdAt: T0 });
    // foo: fresh (no source activity)
    // bar: stale (line-threshold breach)
    seedEdit(db, { filePath: `${SRC2}/big.ts`, createdAt: T1, linesAdded: 200 });

    const result = mineDocStaleness(db, {
      targets: [
        { docFile: DOC, sourceDirs: [SRC] },
        { docFile: DOC2, sourceDirs: [SRC2] },
      ],
    });

    expect(result.checked).toBe(2);
    expect(result.stale_count).toBe(1);
    const fooEntry = result.entries.find((e) => e.doc_file === DOC)!;
    const barEntry = result.entries.find((e) => e.doc_file === DOC2)!;
    expect(fooEntry.stale).toBe(false);
    expect(barEntry.stale).toBe(true);
  });
});

// ─── 9. proposeDocRefresh inserts a harness_log 'propose' row ────────────────

describe('proposeDocRefresh', () => {
  it('inserts a harness_log propose event with the doc-staleness payload shape', () => {
    const db = freshDb();
    seedEdit(db, { filePath: DOC, createdAt: T0 });
    for (let i = 0; i < 6; i++) {
      seedEdit(db, { filePath: `${SRC}/f${i}.ts`, createdAt: T1, linesAdded: 3 });
    }

    const result = mineDocStaleness(db, { targets: [{ docFile: DOC, sourceDirs: [SRC] }] });
    const entry = result.entries[0]!;
    expect(entry.stale).toBe(true);

    const harnessId = proposeDocRefresh(db, entry, { agentId: 'agent-doc', workspacePath: '/repo' });
    expect(harnessId).toMatch(/^harness_/);

    const row = db.prepare('SELECT * FROM harness_log WHERE harness_id = ?').get(harnessId) as unknown as HarnessLogRow;
    expect(row.event_type).toBe('propose');
    expect(row.agent_id).toBe('agent-doc');
    expect(row.workspace_path).toBe('/repo');

    const payload = JSON.parse(row.payload_json ?? 'null');
    expect(payload.failure_signature).toBe('doc-staleness');
    expect(payload.target_file).toBe(DOC);
    expect(payload.proposed_change).toContain(DOC);
    expect(payload.evidence.edits_since_sync).toBe(6);
    expect(payload.evidence.lines_changed_since_sync).toBe(18);
  });

  it('records doc_last_synced_at as null in evidence when the doc was never tracked', () => {
    const db = freshDb();
    for (let i = 0; i < 6; i++) {
      seedEdit(db, { filePath: `${SRC}/f${i}.ts`, createdAt: T1, linesAdded: 1 });
    }
    const result = mineDocStaleness(db, { targets: [{ docFile: DOC, sourceDirs: [SRC] }] });
    const harnessId = proposeDocRefresh(db, result.entries[0]!, { agentId: 'agent-doc' });

    const row = db.prepare('SELECT payload_json FROM harness_log WHERE harness_id = ?').get(harnessId) as { payload_json: string };
    const payload = JSON.parse(row.payload_json);
    expect(payload.evidence.doc_last_synced_at).toBeNull();
    expect(payload.proposed_change).toContain('no prior edit_log record');
  });
});
