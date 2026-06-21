/**
 * Execution-path tests for the OQL open-gap closures that need a clone/inspect/
 * content backend (gaps 7, 8 direct-file lane, 9). The backing tool runner is
 * mocked so these assert OQL's mapping + continuation behavior, not the tools.
 */
import { describe, expect, it, vi, beforeEach } from 'vitest';

const { runDirect } = vi.hoisted(() => ({ runDirect: vi.fn() }));
vi.mock('../../src/oql/adapters/runner.js', () => ({ runDirect }));

import { runOqlSearch } from '../../src/oql/run.js';
import {
  isBatchEnvelope,
  type OqlRecordResultRow,
  type OqlResultEnvelope,
} from '../../src/oql/types.js';

function single(
  r: Awaited<ReturnType<typeof runOqlSearch>>
): OqlResultEnvelope {
  if (isBatchEnvelope(r)) throw new Error('expected single envelope');
  return r;
}
function toolResult(data: Record<string, unknown>, status = 'success') {
  return {
    content: [],
    structuredContent: { results: [{ status, data }] },
  };
}

beforeEach(() => runDirect.mockReset());

/* ----------------- gap 7: materialize checkpoint row -------------------- */

describe('gap 7: target:"materialize" returns a checkpoint row + continuations', () => {
  it('clones once and returns a materialized record row', async () => {
    runDirect.mockResolvedValue(
      toolResult({ localPath: '/cache/facebook/react', cached: false })
    );
    const env = single(
      await runOqlSearch({
        target: 'materialize',
        repo: 'facebook/react',
        path: 'packages/react',
      })
    );
    expect(runDirect).toHaveBeenCalledWith(
      'ghCloneRepo',
      expect.objectContaining({ owner: 'facebook', repo: 'react' })
    );
    const row = env.results[0] as OqlRecordResultRow;
    expect(row.kind).toBe('record');
    expect(row.recordType).toBe('materialized');
    expect(row.id).toBe('/cache/facebook/react');
    expect(row.data.localPath).toBe('/cache/facebook/react');
    expect(row.data.repoRoot).toBe('/cache/facebook/react');
    expect(row.data.complete).toBe(false); // bounded sparse subtree
    expect(env.provenance[0]?.backend).toBe('ghCloneRepo');
    expect(env.provenance[0]?.materializedPath).toBe('/cache/facebook/react');
  });

  it('the checkpoint row carries next.structure / next.files', async () => {
    runDirect.mockResolvedValue(
      toolResult({ localPath: '/cache/react', cached: true })
    );
    const env = single(
      await runOqlSearch({
        target: 'materialize',
        repo: 'facebook/react',
        path: 'packages/react',
      })
    );
    const row = env.results[0] as OqlRecordResultRow;
    expect(row.next?.['next.structure']?.query).toMatchObject({
      target: 'structure',
      from: { kind: 'local', path: '/cache/react' },
    });
    expect(row.next?.['next.files']?.query).toMatchObject({
      target: 'files',
      from: { kind: 'local', path: '/cache/react' },
    });
    // cached clone -> staleCache info diagnostic
    expect(env.diagnostics.map(d => d.code)).toContain('staleCache');
  });
});

/* --------------- gap 9: artifact extract -> tree continuations ---------- */

describe('gap 9: extracted artifact rows carry next.structure / next.files', () => {
  it('emits continuations rooted at the extracted localPath', async () => {
    runDirect.mockResolvedValue(
      toolResult({ localPath: '/extracted/pkg', mode: 'extract' })
    );
    const env = single(
      await runOqlSearch({
        target: 'artifacts',
        from: { kind: 'local', path: '/tmp/pkg.tgz' },
        params: { mode: 'extract' },
      })
    );
    const row = env.results[0] as OqlRecordResultRow;
    expect(row.recordType).toBe('artifact');
    expect(row.next?.['next.structure']?.query).toMatchObject({
      target: 'structure',
      from: { kind: 'local', path: '/extracted/pkg' },
    });
    expect(row.next?.['next.files']?.query).toMatchObject({
      target: 'files',
      from: { kind: 'local', path: '/extracted/pkg' },
    });
  });
});

/* ---------------- #4: typed record-row data contracts ------------------ */

describe('#4 typed record rows: data carries the documented fields', () => {
  it('repository row data exposes typed fields', async () => {
    runDirect.mockResolvedValue(
      toolResult({
        repositories: [
          {
            fullName: 'facebook/react',
            stars: 1000,
            language: 'JavaScript',
            topics: ['ui'],
          },
        ],
      })
    );
    const env = single(
      await runOqlSearch({
        target: 'repositories',
        params: { keywords: ['react'] },
      })
    );
    const row = env.results[0] as OqlRecordResultRow;
    expect(row.recordType).toBe('repository');
    expect(row.id).toBe('facebook/react');
    expect(row.data.stars).toBe(1000);
    expect(row.data.language).toBe('JavaScript');
  });
});

/* ---------------- #6: binary strings scan-offset cursor ----------------- */

describe('#6 per-domain continuation: binary strings nextScanOffset', () => {
  it('emits next.artifactStrings carrying the next scanOffset', async () => {
    runDirect.mockResolvedValue(
      toolResult({ strings: ['libcurl', 'GET'], nextScanOffset: 4096 })
    );
    const env = single(
      await runOqlSearch({
        target: 'artifacts',
        from: { kind: 'local', path: '/tmp/bin' },
        params: { mode: 'strings' },
      })
    );
    const row = env.results[0] as OqlRecordResultRow;
    const cont = row.next?.['next.artifactStrings'];
    expect(cont).toBeDefined();
    expect(
      (cont?.query as { params?: { scanOffset?: number } }).params?.scanOffset
    ).toBe(4096);
  });
});

/* ---------------- gap 8: direct two-ref file diff lane ------------------ */

describe('gap 8: direct file diff lane (baseRef/headRef/path)', () => {
  it('reads both refs and returns a computed line diff', async () => {
    runDirect.mockImplementation((tool: string, q: { branch?: string }) => {
      if (tool === 'ghGetFileContent') {
        return Promise.resolve(
          toolResult({ content: q.branch === 'main' ? 'a\nb\nc' : 'a\nB\nc' })
        );
      }
      return Promise.resolve(toolResult({}));
    });
    const env = single(
      await runOqlSearch({
        target: 'diff',
        repo: 'facebook/react',
        params: { baseRef: 'main', headRef: 'next', path: 'x.ts' },
      })
    );
    const row = env.results[0] as OqlRecordResultRow;
    expect(row.recordType).toBe('diff');
    expect(row.data.baseRef).toBe('main');
    expect(row.data.headRef).toBe('next');
    expect(row.data.additions).toBe(1);
    expect(row.data.deletions).toBe(1);
    expect(row.data.unchanged).toBe(2);
    // direct-file lane uses ghGetFileContent, not ghHistoryResearch
    expect(runDirect).toHaveBeenCalledWith(
      'ghGetFileContent',
      expect.objectContaining({ branch: 'main' })
    );
  });
});
