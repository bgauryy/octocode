/**
 * Golden parity: raw tool output vs OQL output for the local targets, proving
 * the OQL mapping preserves the information the raw tool returns (no rows
 * dropped/added, identity + content preserved). No network — local tools only.
 */
import { describe, expect, it, beforeAll } from 'vitest';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { runOqlSearch } from '../../src/oql/run.js';
import { runDirect } from '../../src/oql/adapters/runner.js';
import { _resetConfigCache } from '../../src/shared/config/resolverCache.js';

// The raw-tool path (executeDirectTool) gates local tools behind ENABLE_LOCAL;
// OQL's local adapters call the runners directly, so only the raw side needs it.
// Reset the cached config so the env override is picked up.
beforeAll(() => {
  process.env.ENABLE_LOCAL = 'true';
  _resetConfigCache();
});
import {
  isBatchEnvelope,
  type OqlCodeResultRow,
  type OqlContentResultRow,
  type OqlFileResultRow,
  type OqlResultEnvelope,
} from '../../src/oql/types.js';

const here = path.dirname(fileURLToPath(import.meta.url));
const OQL_SRC = path.resolve(here, '../../src/oql');
const TYPES = path.join(OQL_SRC, 'types.ts');
const base = (p: string) => p.split('/').pop() ?? p;

function single(
  r: Awaited<ReturnType<typeof runOqlSearch>>
): OqlResultEnvelope {
  if (isBatchEnvelope(r)) throw new Error('expected single envelope');
  return r;
}
function rawData<T = Record<string, unknown>>(result: unknown): T | undefined {
  const sc = (
    result as { structuredContent?: { results?: Array<{ data?: unknown }> } }
  ).structuredContent;
  return sc?.results?.[0]?.data as T | undefined;
}

describe('#12 golden parity: local code search (localSearchCode vs target:code)', () => {
  it('same path:line match set', async () => {
    const raw = await runDirect('localSearchCode', {
      keywords: 'runOqlSearch',
      path: OQL_SRC,
      mode: 'paginated',
    });
    const rd = rawData<{
      files?: Array<{ path: string; matches?: Array<{ line: number }> }>;
    }>(raw);
    const rawPairs = (rd?.files ?? [])
      .flatMap(f => (f.matches ?? []).map(m => `${base(f.path)}:${m.line}`))
      .sort();

    const env = single(
      await runOqlSearch({
        target: 'code',
        from: { kind: 'local', path: OQL_SRC },
        where: { kind: 'text', value: 'runOqlSearch' },
        view: 'paginated',
      })
    );
    const oqlPairs = env.results
      .filter((r): r is OqlCodeResultRow => r.kind === 'code')
      .map(r => `${base(r.path)}:${r.line}`)
      .sort();

    expect(oqlPairs.length).toBeGreaterThan(0);
    expect(oqlPairs).toEqual(rawPairs);
  });
});

describe('#12 golden parity: file content (localGetFileContent vs target:content)', () => {
  it('exact content is byte-identical', async () => {
    const raw = await runDirect('localGetFileContent', {
      path: TYPES,
      minify: 'none',
      fullContent: true,
    });
    const rawContent = rawData<{ content?: string }>(raw)?.content;

    const env = single(
      await runOqlSearch({
        target: 'content',
        from: { kind: 'local', path: TYPES },
        fetch: { content: { contentView: 'exact', fullContent: true } },
      })
    );
    const oqlContent = (env.results[0] as OqlContentResultRow).content;

    expect(oqlContent.length).toBeGreaterThan(0);
    expect(oqlContent).toBe(rawContent);
  });
});

describe('#12 golden parity: file discovery (localFindFiles vs target:files)', () => {
  it('same file basename set', async () => {
    const raw = await runDirect('localFindFiles', { path: OQL_SRC });
    const rd = rawData<{ files?: Array<{ path: string }> }>(raw);
    const rawNames = (rd?.files ?? []).map(f => base(f.path)).sort();

    const env = single(
      await runOqlSearch({
        target: 'files',
        from: { kind: 'local', path: OQL_SRC },
      })
    );
    const oqlNames = env.results
      .filter((r): r is OqlFileResultRow => r.kind === 'file')
      .map(r => base(r.path))
      .sort();

    expect(oqlNames.length).toBeGreaterThan(0);
    expect(oqlNames).toEqual(rawNames);
  });
});
