import { afterEach, describe, expect, it } from 'vitest';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { decodeMemoryContent, renderMemoryContent } from '../src/memory-content.js';
import { openAwarenessStore } from '../src/coordination/open.js';

const roots: string[] = [];
const stores: Array<ReturnType<typeof openAwarenessStore>> = [];
afterEach(() => {
  stores.splice(0).forEach(store => store.close());
  roots.splice(0).forEach(root => rmSync(root, { recursive: true, force: true }));
});

describe('selected file reasoning', () => {
  it('preserves reasons without collapsing evidence about different files or artifacts', async () => {
    const workspace = mkdtempSync(join(tmpdir(), 'awareness-reasons-'));
    roots.push(workspace);
    const awareness = openAwarenessStore({ workspace, dbPath: join(workspace, 'ledger.sqlite3') });
    stores.push(awareness);
    const params = { label: 'DECISION', text: 'Keep the current config on validation failure.',
      sourceDigest: 'config-spec-v1', file: ['src/config.ts'], area: 'configuration',
      why: 'Subscribers require a valid config.', constraint: 'Validate before swapping.',
      scope: 'artifact' as const, artifact: 'config-loader' };
    const store = async (changes = {}) => {
      return awareness.storeVerifiedMemory({ ...params, ...changes });
    };
    const first = await store();
    expect(await store()).toEqual(first);
    expect((await store({ file: ['src/other.ts'] })).memoryId).not.toBe(first.memoryId);
    expect((await store({ artifact: 'another-loader' })).memoryId).not.toBe(first.memoryId);
    const recalled = awareness.recallVerifiedMemory({
      file: ['src/config.ts'], area: 'configuration', scope: 'artifact', artifact: 'config-loader',
    });
    expect(recalled).toMatchObject({ partial: false, memories: [{ memoryId: first.memoryId,
      text: params.text, why: params.why, constraint: params.constraint, file: params.file }] });
    expect(recalled.memories).toHaveLength(1);
  });

  it('treats malformed envelopes as ordinary text and renders selected reasons readably', () => {
    const malformed = '{"$awareness":"awareness-file-context/v1","text":3}';
    expect(decodeMemoryContent(malformed)).toEqual({ text: malformed });
    const incomplete = '{"$awareness":"awareness-file-context/v1"';
    expect(renderMemoryContent(incomplete)).toBe(incomplete);
    expect(renderMemoryContent('{"$awareness":"awareness-file-context/v1","text":"Keep config","why":"Avoid invalid state"}'))
      .toBe('Keep config\nWhy: Avoid invalid state');
  });
});
