import { afterEach, describe, expect, it, vi } from 'vitest';
import { readFileSync } from 'node:fs';
import { buildFileGraph } from '../../../src/graph/buildFileGraph.js';
import { contextUtils } from '../../../src/utils/contextUtils.js';

vi.mock('node:fs', async importOriginal => {
  const original = await importOriginal<typeof import('node:fs')>();
  return { ...original, readFileSync: vi.fn(original.readFileSync) };
});

afterEach(() => {
  vi.restoreAllMocks();
  vi.mocked(readFileSync).mockClear();
});

function scan(
  entries: Array<{ file: string; facts: Record<string, unknown> }>
) {
  vi.spyOn(contextUtils, 'scanGraphFacts').mockResolvedValue({
    candidatePaths: entries.map(entry => entry.file),
    filesSkipped: 0,
    truncated: false,
    entries: entries.map(entry => ({
      relativePath: entry.file,
      factsJson: JSON.stringify(entry.facts),
      referenceCounts: [],
    })),
  });
}

describe('workspace export discovery demand', () => {
  it.each(['py', 'rs', 'c', 'ts'])(
    'does no manifest I/O for 1000 %s files without bare JS imports',
    async extension => {
      scan(
        Array.from({ length: 1000 }, (_, index) => ({
          file: `src/folder_${index}/file.${extension}`,
          facts: {},
        }))
      );
      vi.mocked(readFileSync).mockClear();
      const result = await buildFileGraph(
        '/nonexistent-lazy-graph-fixture',
        [],
        2000
      );
      expect(result.filesScanned).toBe(1000);
      expect(readFileSync).not.toHaveBeenCalled();
    }
  );

  it('retains relative JS imports without discovering workspace packages', async () => {
    scan([
      { file: 'a.ts', facts: { imports: [{ specifier: './b.js', line: 1 }] } },
      { file: 'b.ts', facts: {} },
    ]);
    vi.mocked(readFileSync).mockClear();
    const result = await buildFileGraph(
      '/nonexistent-lazy-graph-fixture',
      [],
      20
    );
    expect([...result.fileGraph.get('a.ts')!.importsFiles]).toEqual(['b.ts']);
    expect(readFileSync).not.toHaveBeenCalled();
  });

  it('discovers exports once when bare imports need them and refreshes on the next scan', async () => {
    scan([
      { file: 'a.ts', facts: { imports: [{ specifier: 'fixture', line: 1 }] } },
      { file: 'b.ts', facts: { imports: [{ specifier: 'fixture', line: 1 }] } },
      { file: 'target.ts', facts: {} },
      { file: 'updated.ts', facts: {} },
    ]);
    vi.mocked(readFileSync).mockReturnValueOnce(
      JSON.stringify({ name: 'fixture', exports: './target.ts' })
    );
    const first = await buildFileGraph(
      '/nonexistent-lazy-graph-fixture',
      [],
      20
    );
    expect(readFileSync).toHaveBeenCalledTimes(1);
    expect([...first.fileGraph.get('a.ts')!.importsFiles]).toEqual([
      'target.ts',
    ]);
    expect([...first.fileGraph.get('b.ts')!.importsFiles]).toEqual([
      'target.ts',
    ]);
    vi.mocked(readFileSync).mockReturnValueOnce(
      JSON.stringify({ name: 'fixture', exports: './updated.ts' })
    );
    const second = await buildFileGraph(
      '/nonexistent-lazy-graph-fixture',
      [],
      20
    );
    expect(readFileSync).toHaveBeenCalledTimes(2);
    expect([...second.fileGraph.get('a.ts')!.importsFiles]).toEqual([
      'updated.ts',
    ]);
  });
});
