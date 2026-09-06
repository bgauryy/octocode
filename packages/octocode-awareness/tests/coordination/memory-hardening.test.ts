import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterEach, describe, expect, it } from 'vitest';
import { evaluateMemoryRecall, MEMORY_EVALUATION_CORPUS_V1 } from '../../src/memory-hardening.js';
import { openAwarenessStore } from '../../src/coordination/open.js';

const roots: string[] = [];
afterEach(() => roots.splice(0).forEach((root) => rmSync(root, { recursive: true, force: true })));

describe('verified memory hardening', () => {
  it('requires provenance, filters expired entries, and explains recall', () => {
    const workspace = mkdtempSync(join(tmpdir(), 'memory-hardening-'));
    roots.push(workspace);
    const aw = openAwarenessStore({ workspace, dbPath: join(workspace, 'awareness.sqlite3') });
    try {
      const active = aw.storeVerifiedMemory({ label: 'build', text: 'Run workspace verify', sourceDigest: 'sha256:docs', verifiedAt: '2026-08-26T00:00:00.000Z', validUntil: '2026-09-01T00:00:00.000Z', importance: 8 });
      aw.storeVerifiedMemory({ label: 'BUILD', text: 'Old command', sourceDigest: 'sha256:old', verifiedAt: '2026-08-01T00:00:00.000Z', validUntil: '2026-08-20T00:00:00.000Z' });
      const recalled = aw.recallVerifiedMemory({ now: '2026-08-26T00:00:00.000Z' });
      expect(recalled.map((memory) => memory.memoryId)).toEqual([active.memoryId]);
      expect(recalled[0]?.explanation).toContain('source=sha256:docs');
    } finally { aw.close(); }
  });

  it('blocks secret-like content before persistence', () => {
    const workspace = mkdtempSync(join(tmpdir(), 'memory-secret-'));
    roots.push(workspace);
    const aw = openAwarenessStore({ workspace, dbPath: join(workspace, 'awareness.sqlite3') });
    try {
      expect(() => aw.storeVerifiedMemory({ label: 'credential', text: 'api_key=supersecretvalue', sourceDigest: 'sha256:x' })).toThrow(/secret-like/);
      expect(() => aw.storeMemory({ label: 'credential', text: 'access_token=supersecretvalue' })).toThrow(/secret-like/);
      expect(aw.recallVerifiedMemory()).toEqual([]);
    } finally { aw.close(); }
  });

  it('never promotes an ordinary row to verified recall through labels or tags', () => {
    const workspace = mkdtempSync(join(tmpdir(), 'memory-unverified-'));
    roots.push(workspace);
    const aw = openAwarenessStore({ workspace, dbPath: join(workspace, 'awareness.sqlite3') });
    try {
      aw.storeMemory({ label: 'OTHER', text: 'unverified assertion', tags: ['verified', 'source:fake'] });
      expect(aw.recallMemory({ query: 'assertion' })).toHaveLength(1);
      expect(aw.recallVerifiedMemory({ query: 'assertion' })).toEqual([]);
    } finally { aw.close(); }
  });

  it('measures precision, recall, stale recall, and false-recall cost', () => {
    expect(evaluateMemoryRecall([{ expectedIds: ['a', 'b'], returnedIds: ['a', 'wrong', 'stale'], staleIds: ['stale'], falseRecallWeight: 2 }]))
      .toEqual({ version: 1, precision: 1 / 3, recall: 1 / 2, staleRecallRate: 1 / 3, falseRecallCost: 4 });
  });

  it('runs the maintained lexical, semantic, hybrid, stale, scope, and secret corpus through the real store', () => {
    const workspace = mkdtempSync(join(tmpdir(), 'memory-evaluate-'));
    roots.push(workspace);
    const previous = process.env['OCTOCODE_EMBED_CMD'];
    delete process.env['OCTOCODE_EMBED_CMD'];
    const aw = openAwarenessStore({ workspace, dbPath: join(workspace, 'awareness.sqlite3') });
    try {
      const common = { verifiedAt: '2026-08-26T00:00:00.000Z', validUntil: '2026-09-01T00:00:00.000Z' };
      aw.storeVerifiedMemory({ label: 'BUILD', text: 'sqlite migration transaction', sourceDigest: 'eval:fresh:migration', ...common });
      aw.storeVerifiedMemory({ label: 'SECURITY', text: 'single use permission race', sourceDigest: 'eval:fresh:authorization', ...common });
      aw.storeVerifiedMemory({ label: 'WORKFLOW', text: 'resume after compact', sourceDigest: 'eval:fresh:recovery', ...common });
      aw.storeVerifiedMemory({ label: 'RELEASE', text: 'release command current', sourceDigest: 'eval:fresh:release', ...common });
      aw.storeVerifiedMemory({ label: 'RELEASE', text: 'release command obsolete', sourceDigest: 'eval:stale:release', verifiedAt: '2026-07-01T00:00:00.000Z', validUntil: '2026-08-01T00:00:00.000Z' });
      aw.storeVerifiedMemory({ label: 'DECISION', text: 'artifact decision', sourceDigest: 'eval:artifact:decision', scope: 'artifact', ...common });
      aw.storeVerifiedMemory({ label: 'DECISION', text: 'artifact decision', sourceDigest: 'eval:project:decision', scope: 'project', ...common });

      const report = aw.evaluateVerifiedMemory({ corpus: MEMORY_EVALUATION_CORPUS_V1, now: '2026-08-26T00:00:00.000Z' });
      expect(report.corpusId).toBe('octocode-memory-hardening-v1');
      expect(report.cases.map((item) => item.mode)).toEqual(expect.arrayContaining(['lexical', 'semantic', 'hybrid']));
      expect(report.aggregate).toMatchObject({ precision: 1, recall: 1, staleRecallRate: 0, falseRecallCost: 0 });
      expect(report).toMatchObject({ staleRecallCost: 0, crossScopeRecallCost: 0, secretRecallCost: 0 });
    } finally {
      aw.close();
      if (previous === undefined) delete process.env['OCTOCODE_EMBED_CMD']; else process.env['OCTOCODE_EMBED_CMD'] = previous;
    }
  });
});
