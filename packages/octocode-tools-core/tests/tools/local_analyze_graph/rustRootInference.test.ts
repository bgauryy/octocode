import { mkdir, mkdtemp, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { buildFileGraph } from '../../../src/graph/buildFileGraph.js';
import { analyzeGraph } from '../../../src/tools/local_analyze_graph/analyzeGraph.js';
import { inferRootFromAbsoluteFile } from '../../../src/tools/local_analyze_graph/rootInference.js';
import { executeAnalyzeGraph } from '../../../src/tools/local_analyze_graph/execution.js';

vi.mock('../../../src/graph/buildFileGraph.js', async importOriginal => ({
  ...(await importOriginal<
    typeof import('../../../src/graph/buildFileGraph.js')
  >()),
  buildFileGraph: vi.fn(async () => ({
    facts: new Map(),
    fileGraph: new Map(),
    filesScanned: 0,
    filesSkipped: 0,
    truncated: false,
    starReexportTargets: new Set(),
    namespaceImportTargets: new Set(),
    starReexporters: new Map(),
  })),
}));

const directories: string[] = [];
afterEach(async () => {
  vi.mocked(buildFileGraph).mockClear();
  await Promise.all(
    directories
      .splice(0)
      .map(path => rm(path, { recursive: true, force: true }))
  );
});

async function fixture() {
  const root = await mkdtemp(join(process.cwd(), '.tmp-rust-root-'));
  directories.push(root);
  const member = join(root, 'crates', 'member');
  await mkdir(join(member, 'src'), { recursive: true });
  await writeFile(join(root, 'package.json'), '{}');
  await writeFile(join(root, 'Cargo.toml'), '[workspace]');
  await writeFile(join(member, 'Cargo.toml'), '[package]');
  return { root, member, file: join(member, 'src', 'lib.rs') };
}

describe('Cargo-aware graph root inference', () => {
  it('separates syntax and Cargo graph cache entries while sharing identical bulk queries', async () => {
    const { root } = await fixture();
    await executeAnalyzeGraph({
      queries: [
        { operation: 'cycles', path: root, rustWorkspace: 'syntax' },
        { operation: 'cycles', path: root, rustWorkspace: 'cargo' },
        { operation: 'cycles', path: root, rustWorkspace: 'cargo' },
      ],
    });
    expect(buildFileGraph).toHaveBeenCalledTimes(2);
    expect(vi.mocked(buildFileGraph).mock.calls.map(call => call[3])).toEqual([
      'syntax',
      'cargo',
    ]);
  });

  it('uses Cargo mode for non-Rust input and falls back to package markers when no Cargo manifest exists', async () => {
    const { root, member } = await fixture();
    const source = join(member, 'src', 'index.ts');
    expect(inferRootFromAbsoluteFile(source, 'cargo')).toBe(member);
    await rm(join(member, 'Cargo.toml'));
    await rm(join(root, 'Cargo.toml'));
    expect(inferRootFromAbsoluteFile(source, 'cargo')).toBe(root);
  });

  it('uses the nearest Cargo manifest for Rust sources without executing Cargo', async () => {
    const { member, file } = await fixture();
    expect(inferRootFromAbsoluteFile(file)).toBe(member);
    expect(buildFileGraph).not.toHaveBeenCalled();
  });

  it('retains JavaScript package inference in a mixed workspace', async () => {
    const { root, member } = await fixture();
    expect(inferRootFromAbsoluteFile(join(member, 'src', 'index.ts'))).toBe(
      root
    );
  });

  it.each(['direct', 'public'] as const)(
    'passes the inferred Cargo boundary through the %s path',
    async path => {
      const { member, file } = await fixture();
      const query = {
        operation: 'dependencies' as const,
        file,
        rustWorkspace: 'cargo' as const,
      };
      if (path === 'direct') await analyzeGraph(query);
      else await executeAnalyzeGraph({ queries: [query] });
      expect(buildFileGraph).toHaveBeenCalledWith(
        member,
        expect.any(Array),
        20_000,
        'cargo'
      );
    }
  );

  it('honors an explicit wider root in Cargo mode', async () => {
    const { root, file } = await fixture();
    await executeAnalyzeGraph({
      queries: [
        { operation: 'dependencies', path: root, file, rustWorkspace: 'cargo' },
      ],
    });
    expect(buildFileGraph).toHaveBeenCalledWith(
      root,
      expect.any(Array),
      20_000,
      'cargo'
    );
  });
});
