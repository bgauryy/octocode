import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { openAwarenessStore } from '../../src/coordination/open.js';
import type { AwarenessStore } from '../../src/coordination/coordination-continuity.js';
// Deterministic bag-of-words embedder: text on stdin → JSON { embedding, model }.
// Shared words yield higher cosine, so semantic ranking is testable without a model.
const EMBED_SCRIPT = `
let d='';process.stdin.on('data',c=>d+=c).on('end',()=>{
  const N=64;const v=new Array(N).fill(0);
  for(const w of d.toLowerCase().split(/[^a-z0-9]+/).filter(Boolean)){
    let h=0;for(const ch of w)h=(h*31+ch.charCodeAt(0))>>>0;
    v[h%N]+=1;
  }
  process.stdout.write(JSON.stringify({embedding:v,model:'test-bow'}));
});
`;

describe('semantic memory recall', () => {
  let workspace: string;
  let scriptPath: string;
  let aw: AwarenessStore;
  const prevCmd = process.env['OCTOCODE_EMBED_CMD'];

  beforeEach(async () => {
    workspace = await mkdtemp(join(tmpdir(), 'aw-lite-embed-'));
    scriptPath = join(workspace, 'embed.mjs');
    await writeFile(scriptPath, EMBED_SCRIPT, 'utf8');
    process.env['OCTOCODE_EMBED_CMD'] = `"${process.execPath}" "${scriptPath}"`;
    aw = openAwarenessStore({ workspace });
  });

  afterEach(async () => {
    if (prevCmd === undefined) delete process.env['OCTOCODE_EMBED_CMD'];
    else process.env['OCTOCODE_EMBED_CMD'] = prevCmd;
    await rm(workspace, { recursive: true, force: true });
  });

  it('ranks the semantically closest memory first through canonical recall', () => {
    aw.storeMemory({ label: 'BUILD', text: 'how to run a database migration with drizzle' });
    aw.storeMemory({ label: 'OTHER', text: 'classic apple pie recipe with cinnamon' });
    const hits = aw.recallMemory({ query: 'database migration steps', semantic: true, limit: 5 });
    expect(hits.memories.length).toBeGreaterThan(0);
    expect(hits.memories[0]!.label).toBe('BUILD');
  });

  it('ranks only current verified rows through the semantic verified-memory route', () => {
    aw.storeVerifiedMemory({ label: 'BUILD', text: 'database migration transaction', sourceDigest: 'sha256:db', verifiedAt: '2026-08-26T00:00:00.000Z', validUntil: '2027-01-01T00:00:00.000Z' });
    aw.storeVerifiedMemory({ label: 'OTHER', text: 'apple pie cinnamon recipe', sourceDigest: 'sha256:food', verifiedAt: '2026-08-26T00:00:00.000Z', validUntil: '2027-01-01T00:00:00.000Z' });
    aw.storeMemory({ label: 'OTHER', text: 'database migration transaction extra' });
    const hits = aw.recallVerifiedMemory({ query: 'database migration steps', mode: 'semantic', now: '2026-08-27T00:00:00.000Z' });
    expect(hits.memories[0]?.sourceDigest).toBe('sha256:db');
    expect(hits.memories[0]?.explanation).toContain('similarity=');
    expect(hits.memories.map((item) => item.sourceDigest)).not.toContain('unverified');
  });

  it('pages semantic and hybrid unions without repeats and includes lexical-only rows in hybrid', () => {
    for (const [index, [text, sourceDigest]] of ([
      ['database migration transaction', 'sha256:one'],
      ['database migration rollback', 'sha256:two'],
      ['database migration lexical-only', 'sha256:low'],
    ] as const).entries()) {
      // Keep one lexical-only row out of the embedding pool so hybrid recall
      // proves that it unions semantic and lexical candidates.
      if (index === 2) delete process.env['OCTOCODE_EMBED_CMD'];
      aw.storeVerifiedMemory({ label: 'TEST', text, sourceDigest, verifiedAt: '2026-08-26T00:00:00Z', validUntil: '2027-01-01T00:00:00Z' });
      if (index === 2) process.env['OCTOCODE_EMBED_CMD'] = `"${process.execPath}" "${scriptPath}"`;
    }
    aw.storeVerifiedMemory({ label: 'OTHER', text: 'apple pie cinnamon recipe', sourceDigest: 'sha256:unrelated', verifiedAt: '2026-08-26T00:00:00Z', validUntil: '2027-01-01T00:00:00Z' });
    const readPages = (mode: 'semantic' | 'hybrid', minSimilarity?: number) => {
      const ids: string[] = [];
      let page = aw.recallVerifiedMemory({ query: 'database migration', mode, minSimilarity, limit: 1, now: '2026-08-27T00:00:00Z' });
      for (;;) {
        ids.push(...page.memories.map(item => item.memoryId));
        if (!page.next) return { ids, page };
        page = aw.recallVerifiedMemory(page.next.params);
      }
    };
    const semantic = readPages('semantic', 0.5);
    expect(new Set(semantic.ids).size).toBe(2);
    expect(semantic.ids).toHaveLength(2);
    const hybrid = readPages('hybrid', 0.5);
    expect(new Set(hybrid.ids).size).toBe(hybrid.ids.length);
    expect(hybrid.ids).toHaveLength(3);
  });

  it('reindex backfills embeddings for rows stored before the embedder was set', () => {
    delete process.env['OCTOCODE_EMBED_CMD'];
    aw.storeMemory({ label: 'BUILD', text: 'database migration notes' });
    process.env['OCTOCODE_EMBED_CMD'] = `"${process.execPath}" "${scriptPath}"`;
    const first = aw.reindexMemories();
    expect(first).toEqual({ enabled: true, scanned: 1, embedded: 1 });
    // Nothing left missing → second pass embeds 0.
    expect(aw.reindexMemories()).toEqual({ enabled: true, scanned: 0, embedded: 0 });
    expect(aw.recallMemory({ query: 'database migration', semantic: true }).memories[0]?.label).toBe('BUILD');
  });

  it('falls back to lexical recall when the embedder is unset', () => {
    delete process.env['OCTOCODE_EMBED_CMD'];
    aw.storeMemory({ label: 'BUILD', text: 'database migration notes' });
    const hits = aw.recallMemory({ query: 'migration', semantic: true });
    expect(hits.memories.map((h) => h.label)).toContain('BUILD');
    expect(hits.memories[0]!.similarity).toBeUndefined();
    expect(aw.reindexMemories()).toEqual({ enabled: false, scanned: 0, embedded: 0 });
  });

  it('reindexes vectors after an embedding model change before semantic recall', async () => {
    // Store embedded under the default model ('test-bow').
    aw.storeMemory({ label: 'BUILD', text: 'database migration notes' });
    // Swap the embedder to a DIFFERENT model of the SAME dimension. The stored
    // vector is now cross-model and must NOT be treated as a semantic candidate.
    const v2 = join(workspace, 'embed-v2.mjs');
    await writeFile(v2, EMBED_SCRIPT.replace("model:'test-bow'", "model:'test-bow-v2'"), 'utf8');
    process.env['OCTOCODE_EMBED_CMD'] = `"${process.execPath}" "${v2}"`;
    const hits = aw.recallMemory({ query: 'database migration notes', semantic: true });
    expect(hits.memories.map((h) => h.label)).toContain('BUILD');
    expect(aw.reindexMemories()).toEqual({ enabled: true, scanned: 1, embedded: 1 });
  });

  it('reindexes same-model vectors after an embedding dimension change', async () => {
    aw.storeMemory({ label: 'BUILD', text: 'database migration notes' });
    const resized = join(workspace, 'embed-resized.mjs');
    await writeFile(resized, EMBED_SCRIPT.replace('const N=64', 'const N=32'), 'utf8');
    process.env['OCTOCODE_EMBED_CMD'] = `"${process.execPath}" "${resized}"`;
    const hits = aw.recallMemory({ query: 'database migration notes', semantic: true });
    expect(hits.memories[0]?.label).toBe('BUILD');
  });

  it('keeps canonical lexical fallback explicit when a host-only similarity floor is unavailable', () => {
    aw.storeMemory({ label: 'BUILD', text: 'how to run a database migration with drizzle' });
    // Default floor (0): the weakly-related row is a semantic hit with a score.
    const withScore = aw.recallMemory({ query: 'database migration steps', semantic: true });
    expect(withScore.memories[0]?.label).toBe('BUILD');
    // Canonical recall does not expose the removed host scorer's similarity
    // threshold; it remains a useful lexical result rather than a fabricated score.
    const floored = aw.recallMemory({ query: 'database migration steps', semantic: true, minSimilarity: 0.95 });
    expect(floored.memories.map((hit) => hit.label)).toContain('BUILD');
  });
});
