import { describe, expect, it } from 'vitest';
import {
  buildArtifactContinuations,
  type ContinuationCtx,
} from '../../src/oql/run.js';
import type { OqlQuery, OqlResultRow } from '../../src/oql/types.js';

function artifactRow(data: Record<string, unknown>): OqlResultRow {
  return {
    kind: 'record',
    recordType: 'artifact',
    source: { kind: 'local', path: '/bin/x' },
    data,
  } as unknown as OqlResultRow;
}

function ctx(params: Record<string, unknown>): ContinuationCtx {
  const query: OqlQuery = {
    schema: 'oql',
    target: 'artifacts',
    from: { kind: 'local', path: '/bin/x' },
    params,
  } as OqlQuery;
  return { query };
}

describe('artifact continuations are mode-aware', () => {
  it('strings dump → next.search (search the dump), NOT next.structure/next.files', () => {
    const next = buildArtifactContinuations(
      artifactRow({
        mode: 'strings',
        localPath: '/tmp/x-strings/dump.txt',
        nextScanOffset: 67108864,
      }),
      ctx({ mode: 'strings' })
    )!;
    // a flat strings dump is grepped, not listed or file-discovered
    expect(next['next.search']).toBeTruthy();
    // lossless ripgrep over the dump: a regex predicate + match-paging controls
    expect(next['next.search']!.query).toMatchObject({
      target: 'code',
      from: { kind: 'local', path: '/tmp/x-strings/dump.txt' },
      where: { kind: 'regex' },
      controls: { search: { matchPage: 1 } },
    });
    expect(next['next.structure']).toBeUndefined();
    expect(next['next.files']).toBeUndefined();
    // and the scan cursor still advances
    expect(next['next.artifactStrings']).toBeTruthy();
    expect(next['next.artifactStrings']!.query.params).toMatchObject({
      scanOffset: 67108864,
    });
  });

  it('strings preview pagination → next.artifactContent char window', () => {
    const next = buildArtifactContinuations(
      artifactRow({
        mode: 'strings',
        localPath: '/tmp/x-strings/dump.txt',
        pagination: {
          hasMore: true,
          charLength: 4000,
          nextCharOffset: 4000,
        },
      }),
      ctx({ mode: 'strings' })
    )!;
    expect(next['next.artifactContent']).toBeTruthy();
    expect(next['next.artifactContent']!.query.params).toMatchObject({
      charOffset: 4000,
      charLength: 4000,
    });
    expect(next['next.search']).toBeTruthy();
  });

  it('unpack/extract → next.structure + next.files (a real extracted tree)', () => {
    const next = buildArtifactContinuations(
      artifactRow({ mode: 'unpack', localPath: '/tmp/unzip/pkg' }),
      ctx({ mode: 'unpack' })
    )!;
    expect(next['next.structure']).toBeTruthy();
    expect(next['next.files']).toBeTruthy();
    expect(next['next.search']).toBeUndefined();
  });

  it('inspect (no localPath) → no local continuations', () => {
    const next = buildArtifactContinuations(
      artifactRow({ mode: 'inspect', format: 'macho' }),
      ctx({ mode: 'inspect' })
    );
    expect(next).toBeUndefined();
  });
});
