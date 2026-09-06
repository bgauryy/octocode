import { afterEach, describe, expect, it, vi } from 'vitest';
import { prepareRustResolver } from '../../../src/graph/rustWorkspace.js';
import * as cargo from '../../../src/graph/rustCargoMetadata.js';
import type { GraphCoverage } from '../../../src/graph/types.js';

afterEach(() => vi.restoreAllMocks());
const coverage = (): GraphCoverage => ({
  basis: 'syntactic',
  referenceBasis: 'lexical-occurrence',
  languages: [],
  imports: { resolved: 0, external: 0, unresolvedInternal: 0, unsupported: 0 },
  diagnostics: [],
});
function target(
  id: string,
  srcPath: string,
  dependencyAliases: cargo.RustCargoTarget['dependencyAliases'] = []
): cargo.RustCargoTarget {
  return {
    id,
    packageId: id,
    packageName: id,
    crateName: id,
    kind: ['lib'],
    srcPath,
    dependencyAliases,
    edition: '2021',
  };
}

describe('Cargo mode module context integration', () => {
  it('rejects divergent duplicate dependency aliases instead of selecting the last context', async () => {
    const entries = ['src/lib.rs', 'dep.rs'].map(relativePath => ({
      relativePath,
      parsed: {},
    }));
    const dependencies = [
      {
        alias: 'dependency',
        packageName: 'conditional',
        external: false,
        conditional: true,
      },
      {
        alias: 'dependency',
        packageName: 'active',
        targetId: 'dep',
        external: false,
        conditional: false,
      },
    ];
    vi.spyOn(cargo, 'resolveRustCargoMetadata').mockResolvedValue({
      status: 'ok',
      diagnostics: [],
      targets: [
        target('app', 'src/lib.rs', dependencies),
        target('dep', 'dep.rs'),
      ],
    });
    const report = coverage();
    const resolve = await prepareRustResolver(
      '/repo',
      new Set(entries.map(entry => entry.relativePath)),
      entries,
      'cargo',
      report
    );
    expect(resolve('dependency::Thing', 'src/lib.rs').status).toBe(
      'unsupported'
    );
    expect(
      report.diagnostics.some(item => item.message.includes('dependency alias'))
    ).toBe(true);
  });

  it('does not invoke Cargo in syntax mode', async () => {
    const metadata = vi.spyOn(cargo, 'resolveRustCargoMetadata');
    await prepareRustResolver(
      '/repo',
      new Set(['src/lib.rs']),
      [{ relativePath: 'src/lib.rs', parsed: {} }],
      'syntax',
      coverage()
    );
    expect(metadata).not.toHaveBeenCalled();
  });

  it('links a renamed local dependency from a custom target root', async () => {
    const entries = ['custom/entry.rs', 'shared/api.rs'].map(relativePath => ({
      relativePath,
      parsed: {},
    }));
    vi.spyOn(cargo, 'resolveRustCargoMetadata').mockResolvedValue({
      status: 'ok',
      diagnostics: [],
      targets: [
        target('app', 'custom/entry.rs', [
          {
            alias: 'renamed',
            packageName: 'shared',
            targetId: 'shared',
            external: false,
            conditional: false,
          },
        ]),
        target('shared', 'shared/api.rs'),
      ],
    });
    const resolve = await prepareRustResolver(
      '/repo',
      new Set(entries.map(entry => entry.relativePath)),
      entries,
      'cargo',
      coverage()
    );
    expect(resolve('renamed::Thing', 'custom/entry.rs').target).toBe(
      'shared/api.rs'
    );
  });

  it('rejects conflicting target contexts that share one physical source', async () => {
    const entries = ['shared.rs', 'dep.rs'].map(relativePath => ({
      relativePath,
      parsed: {},
    }));
    vi.spyOn(cargo, 'resolveRustCargoMetadata').mockResolvedValue({
      status: 'ok',
      diagnostics: [],
      targets: [
        target('library', 'shared.rs', [
          {
            alias: 'dependency',
            packageName: 'dependency',
            targetId: 'dep',
            external: false,
            conditional: false,
          },
        ]),
        target('binary', 'shared.rs', [
          {
            alias: 'dependency',
            packageName: 'dependency',
            external: true,
            conditional: false,
          },
        ]),
        target('dep', 'dep.rs'),
      ],
    });
    const report = coverage();
    const resolve = await prepareRustResolver(
      '/repo',
      new Set(entries.map(entry => entry.relativePath)),
      entries,
      'cargo',
      report
    );
    expect(resolve('dependency::Thing', 'shared.rs').status).toBe(
      'unsupported'
    );
    expect(
      report.diagnostics.some(diagnostic =>
        diagnostic.message.includes('share a source file')
      )
    ).toBe(true);
  });
});
