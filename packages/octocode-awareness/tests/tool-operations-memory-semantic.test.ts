import { describe, expect, it } from 'vitest';
import { DatabaseSync } from 'node:sqlite';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { initDb } from '../src/db-init.js';
import { runAwarenessToolOperation } from '../src/tool-operations.js';

function freshDb(): DatabaseSync {
  const db = new DatabaseSync(':memory:');
  db.exec('PRAGMA foreign_keys = ON');
  initDb(db);
  return db;
}

function run(
  db: DatabaseSync,
  operation: Parameters<typeof runAwarenessToolOperation>[1],
  request: Record<string, unknown>,
  cwd: string,
  agentId = 'agent-a',
) {
  return runAwarenessToolOperation(db, operation, request, { cwd, agentId, sessionId: `sess-test-${agentId}` });
}

describe('runAwarenessToolOperation memory recall semantic mode', () => {
  it('recall semantic:true falls back to lexical with a warning when no embedder is configured', () => {
    const dir = mkdtempSync(join(tmpdir(), 'oc-tool-memory-semantic-off-'));
    const previousEmbedCmd = process.env['OCTOCODE_EMBED_CMD'];
    delete process.env['OCTOCODE_EMBED_CMD'];
    try {
      const db = freshDb();
      run(db, 'record', {
        task_context: 'semantic fallback memory',
        observation: 'no embedder configured for this recall',
        label: 'GOTCHA',
        workspace_path: dir,
      }, dir);

      const recalled = run(db, 'recall', { query: 'semantic', semantic: true, workspace_path: dir }, dir);
      const payload = recalled.payload as { mode?: string; warnings?: string[]; count: number };
      expect(payload.mode).not.toBe('semantic');
      expect(payload.warnings?.some((w) => w.includes('semantic ranking is unavailable'))).toBe(true);
      expect(payload.count).toBeGreaterThanOrEqual(1);
    } finally {
      if (previousEmbedCmd === undefined) delete process.env['OCTOCODE_EMBED_CMD'];
      else process.env['OCTOCODE_EMBED_CMD'] = previousEmbedCmd;
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it('recall semantic:true ranks by embedding cosine similarity when OCTOCODE_EMBED_CMD is configured', () => {
    const dir = mkdtempSync(join(tmpdir(), 'oc-tool-memory-semantic-on-'));
    const embedScript = join(dir, 'embed.mjs');
    writeFileSync(embedScript, `#!/usr/bin/env node
import { readFileSync } from 'node:fs';
const text = readFileSync(0, 'utf8');
const embedding = text.includes('alpha') ? [1, 0, 0] : text.includes('beta') ? [0, 1, 0] : [0.2, 0.8, 0];
process.stdout.write(JSON.stringify({ embedding, model: 'test-embed' }));
`, 'utf8');
    const previousEmbedCmd = process.env['OCTOCODE_EMBED_CMD'];
    process.env['OCTOCODE_EMBED_CMD'] = `${process.execPath} ${embedScript}`;
    try {
      const db = freshDb();
      const alpha = run(db, 'record', {
        task_context: 'alpha vector memory',
        observation: 'alpha content for semantic ranking',
        label: 'DECISION',
        workspace_path: dir,
      }, dir);
      run(db, 'record', {
        task_context: 'beta vector memory',
        observation: 'beta content for semantic ranking',
        label: 'DECISION',
        workspace_path: dir,
      }, dir);
      const alphaId = (alpha.payload as { memory_id: string }).memory_id;

      const recalled = run(db, 'recall', {
        query: 'alpha', semantic: true, workspace_path: dir, limit: 2,
      }, dir);
      const payload = recalled.payload as { mode?: string; embedding_model?: string; memories: Array<{ memory_id: string }> };
      expect(payload.mode).toBe('semantic');
      expect(payload.embedding_model).toBe('test-embed');
      expect(payload.memories[0]?.memory_id).toBe(alphaId);
    } finally {
      if (previousEmbedCmd === undefined) delete process.env['OCTOCODE_EMBED_CMD'];
      else process.env['OCTOCODE_EMBED_CMD'] = previousEmbedCmd;
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it('recall without semantic never surfaces mode/embedding_model/warnings (unchanged default shape)', () => {
    const dir = mkdtempSync(join(tmpdir(), 'oc-tool-memory-semantic-default-'));
    try {
      const db = freshDb();
      run(db, 'record', {
        task_context: 'default shape memory',
        observation: 'plain lexical recall, no semantic flag',
        label: 'GOTCHA',
        workspace_path: dir,
      }, dir);
      const recalled = run(db, 'recall', { query: 'default', workspace_path: dir }, dir);
      const payload = recalled.payload as Record<string, unknown>;
      expect(payload['mode']).toBeUndefined();
      expect(payload['embedding_model']).toBeUndefined();
      expect(payload['warnings']).toBeUndefined();
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });
});
