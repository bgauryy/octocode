/**
 * Unit-level harness structure tests (S6, S8 acceptance).
 * Does not require compiled servers or NAPI addon — only verifies the
 * harness module structure and the static corpus registry.
 */
import { describe, it, expect } from 'vitest';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const here = dirname(fileURLToPath(import.meta.url));

// Lazily import the harness ESM — vitest handles ESM natively.
async function importHarness() {
  const mod = await import(join(here, 'harness.mjs'));
  return mod as {
    Harness: { connect: (opts: unknown) => Promise<unknown> };
    CORPUS: Record<string, { status: string; note: string; script?: string }>;
    assertCorpusComplete: () => void;
  };
}

describe('Parity harness (S6)', () => {
  it('exports Harness, CORPUS, assertCorpusComplete', async () => {
    const { Harness, CORPUS, assertCorpusComplete } = await importHarness();
    expect(typeof Harness).toBe('function');
    expect(typeof CORPUS).toBe('object');
    expect(typeof assertCorpusComplete).toBe('function');
  });

  it('CORPUS covers all 12 tools', async () => {
    const { CORPUS } = await importHarness();
    const expectedTools = [
      'localSearch',
      'localFetch',
      'astSearch',
      'astRewrite',
      'astGraph',
      'lspSearch',
      'ghSearch',
      'ghGetFileContent',
      'ghSearchHistory',
      'ghGetHistoryItem',
      'ghCloneRepo',
      'artifactSearch',
    ];
    for (const tool of expectedTools) {
      expect(CORPUS).toHaveProperty(tool);
      expect(CORPUS[tool]).toHaveProperty('status');
      expect(CORPUS[tool]).toHaveProperty('note');
    }
  });

  it('assertCorpusComplete throws when any tool is not covered', async () => {
    const { assertCorpusComplete } = await importHarness();
    expect(() => assertCorpusComplete()).toThrow(/Parity corpus is incomplete/);
  });

  it('CORPUS.localFetch is already covered by response-pagination.mjs', async () => {
    const { CORPUS } = await importHarness();
    expect(CORPUS.localFetch.status).toBe('covered');
  });

  it('each CORPUS entry has a valid status value', async () => {
    const { CORPUS } = await importHarness();
    const validStatuses = new Set(['pending', 'partial', 'blocked', 'covered']);
    for (const [tool, entry] of Object.entries(CORPUS)) {
      expect(validStatuses).toContain(
        entry.status,
        `${tool}.status must be pending|partial|blocked|covered`
      );
      if (entry.status !== 'pending') {
        expect(entry.script).toBeTruthy();
      }
    }
  });
});

describe('Schema single-source guard (S8)', () => {
  it('contract-provenance.json is accessible and has sourceDirty:false', async () => {
    const { readFileSync } = await import('node:fs');
    // here = packages/octocode-mcp/tests/native/parity/
    // ../../../../ = packages/
    const provenancePath = join(
      here,
      '../../../../octocode-native/src/contracts/generated/contract-provenance.json'
    );
    let provenance: { sourceDirty: boolean; contractFingerprint: string; sourceRevision: string };
    expect(() => {
      provenance = JSON.parse(readFileSync(provenancePath, 'utf8'));
    }).not.toThrow();
    expect(provenance!.sourceDirty).toBe(false);
    expect(provenance!.contractFingerprint).toMatch(/^[0-9a-f]{64}$/);
    expect(provenance!.sourceRevision).toMatch(/^[0-9a-f]{40}$/);
  });
});
