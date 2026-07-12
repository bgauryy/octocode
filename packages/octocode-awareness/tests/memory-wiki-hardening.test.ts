import { describe, expect, it } from 'vitest';
import { DatabaseSync } from 'node:sqlite';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { initDb } from '../src/db.js';
import { evidenceTrust, manifestWarnings } from '../src/attend-model.js';
import { projectMemoryLean } from '../src/helpers.js';
import { getMemory, insertMemory } from '../src/memory.js';
import { decayComponents } from '../src/memory-scoring.js';
import { injectRepoContext } from '../src/repo-context.js';
import { sanitizeShareString } from '../src/repo-projection.js';
import type { MemoryRecord } from '../src/types.js';

function freshDb(): DatabaseSync {
  const db = new DatabaseSync(':memory:');
  db.exec('PRAGMA foreign_keys = ON');
  initDb(db);
  return db;
}

describe('memory and wiki trust with lean retrieval', () => {
  it('keeps explicit smart filters until the requested result set under-fills', () => {
    const db = freshDb();
    insertMemory(db, {
      taskContext: 'cache invalidation rule', observation: 'high confidence rule', importance: 5,
    });
    insertMemory(db, {
      taskContext: 'cache invalidation rule', observation: 'lower confidence rule', importance: 4,
    });

    const result = getMemory(db, {
      query: 'cache invalidation rule', minImportance: 5, limit: 1, smart: true, explain: true,
      recordAccess: false,
    });

    expect(result.count).toBe(1);
    expect(result.smart_expanded).toBeUndefined();
    expect(result.applied_filters?.min_importance).toBe(5);
  });

  it('does not let recall activity refresh evidence age', () => {
    const now = new Date().toISOString();
    const base = {
      memory_id: 'm', agent_id: 'a', task_context: 't', observation: 'o', importance: 5,
      state: 'ACTIVE' as const, label: 'OTHER', superseded_by: null, tags: [], references: [],
      workspace_path: null, artifact: null, repo: null, ref: null, novelty_score: null,
      failure_signature: null, access_count: 0, decay_half_life_days: null,
      valid_from: null, valid_to: null, expired_at: null, file_tree_fingerprint: null,
      updated_at: null,
    };
    const staleButRead = { ...base, created_at: '2020-01-01T00:00:00Z', last_accessed_at: now } as MemoryRecord;
    const newerEvidence = { ...base, memory_id: 'new', created_at: '2026-01-01T00:00:00Z', last_accessed_at: null } as MemoryRecord;

    expect(decayComponents(newerEvidence, 1).recency)
      .toBeGreaterThan(decayComponents(staleButRead, 1).recency);
  });

  it('rejects missing, foreign-owner, cross-scope, and inactive supersession targets atomically', () => {
    const db = freshDb();
    const old = insertMemory(db, {
      agentId: 'owner', taskContext: 'old rule', observation: 'old evidence', importance: 5,
      workspacePath: '/workspace/a',
    });

    expect(() => insertMemory(db, {
      agentId: 'intruder', taskContext: 'replacement', observation: 'foreign owner', importance: 5,
      workspacePath: '/workspace/a', supersedes: [old.memoryId],
    })).toThrow(/owner/i);
    expect(() => insertMemory(db, {
      agentId: 'owner', taskContext: 'replacement', observation: 'cross scope', importance: 5,
      workspacePath: '/workspace/b', supersedes: [old.memoryId],
    })).toThrow(/scope/i);
    expect(() => insertMemory(db, {
      agentId: 'owner', taskContext: 'replacement', observation: 'missing target', importance: 5,
      workspacePath: '/workspace/a', supersedes: ['mem_missing'],
    })).toThrow(/not found/i);

    const row = db.prepare('SELECT state, superseded_by FROM memories WHERE memory_id = ?').get(old.memoryId) as Record<string, unknown>;
    expect(row).toEqual({ state: 'ACTIVE', superseded_by: null });
    expect((db.prepare('SELECT COUNT(*) AS count FROM memories').get() as { count: number }).count).toBe(1);
  });

  it('labels existing files as leads and never embeds memory prose in generated AGENTS', () => {
    const workspace = mkdtempSync(join(tmpdir(), 'oc-wiki-trust-'));
    try {
      mkdirSync(join(workspace, 'src'), { recursive: true });
      const source = join(workspace, 'src', 'current.ts');
      writeFileSync(source, 'export const current = true;\n', 'utf8');
      expect(evidenceTrust([`file:${source}:1`], workspace)).toBe('existing_file_lead');

      const db = freshDb();
      insertMemory(db, {
        taskContext: 'trusted-looking gotcha', observation: 'MEMORY_PROSE_MUST_NOT_BECOME_INSTRUCTIONS',
        importance: 10, label: 'GOTCHA', references: [`file:${source}:1`], workspacePath: workspace,
      });
      insertMemory(db, {
        taskContext: 'unreferenced lesson', observation: 'UNREFERENCED_PROSE_MUST_NOT_BECOME_INSTRUCTIONS',
        importance: 10, label: 'WORKFLOW', workspacePath: workspace,
      });
      injectRepoContext(db, { workspacePath: workspace, outDir: join(workspace, '.octocode'), check: false, includeView: false });
      const agents = readFileSync(join(workspace, '.octocode', 'AGENTS.md'), 'utf8');
      expect(agents).not.toContain('MEMORY_PROSE_MUST_NOT_BECOME_INSTRUCTIONS');
      expect(agents).not.toContain('UNREFERENCED_PROSE_MUST_NOT_BECOME_INSTRUCTIONS');
      expect(agents).not.toContain('## Top Gotchas');
      expect(agents).not.toContain('## Top Lessons');
    } finally {
      rmSync(workspace, { recursive: true, force: true });
    }
  });

  it('skips an expensive live revision when a manifest already admits partial coverage', () => {
    const workspace = mkdtempSync(join(tmpdir(), 'oc-manifest-partial-'));
    try {
      mkdirSync(join(workspace, '.octocode', 'awareness'), { recursive: true });
      writeFileSync(join(workspace, '.octocode', 'awareness', 'manifest.json'), JSON.stringify({
        generated_at: '2026-01-01T00:00:00Z',
        files: ['.octocode/AGENTS.md', '.octocode/KNOWLEDGE.md'],
        source: { revision: 'sha256:bounded' },
        completeness: { memories: { is_partial: true, omitted_count: 10 } },
      }), 'utf8');
      let revisionCalls = 0;
      const warnings = manifestWarnings(workspace, [], () => {
        revisionCalls += 1;
        return 'sha256:live';
      });
      expect(revisionCalls).toBe(0);
      expect(warnings.join(' ')).toMatch(/partial.*live SQLite/i);
    } finally {
      rmSync(workspace, { recursive: true, force: true });
    }
  });

  it('caps list fields and omits absent optional fields from lean memory rows', () => {
    const memory = {
      memory_id: 'mem_1', label: 'GOTCHA', importance: 8,
      task_context: 'context', observation: 'observation',
      tags: ['a', 'b', 'c', 'd'], references: ['r1', 'r2', 'r3', 'r4'],
      score: undefined, failure_signature: null,
    } as unknown as MemoryRecord;
    const lean = projectMemoryLean(memory);
    expect(lean).toMatchObject({
      tags: ['a', 'b', 'c'], tag_count: 4, tag_omitted_count: 1,
      references: ['r1', 'r2', 'r3'], reference_count: 4, reference_omitted_count: 1,
    });
    expect(lean).not.toHaveProperty('score');
    expect(lean).not.toHaveProperty('failure_signature');
    expect(lean).not.toHaveProperty('created_at');
  });

  it('redacts recognized secrets from share projections', () => {
    const shared = sanitizeShareString(
      'token=github_pat_1234567890abcdefghijkl password=hunterhunter',
      '/workspace',
    );
    expect(shared).not.toContain('github_pat_1234567890abcdefghijkl');
    expect(shared).not.toContain('hunterhunter');
    expect(shared.match(/<redacted-secret>/g)).toHaveLength(2);
  });
});
