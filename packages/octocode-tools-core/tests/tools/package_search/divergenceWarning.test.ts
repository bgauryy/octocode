import { describe, expect, it } from 'vitest';

import { formatPackageData } from '../../../src/tools/package_search/execution.js';

type AnyPkg = Parameters<typeof formatPackageData>[0];

const npmPkg = (overrides: Record<string, unknown>): AnyPkg =>
  ({
    // `npmUrl` is the isNpm() discriminator; npm packages carry `repoUrl`.
    npmUrl: 'https://www.npmjs.com/package/typescript',
    name: 'typescript',
    version: '7.0.2',
    repoUrl: 'https://github.com/microsoft/TypeScript',
    ...overrides,
  }) as unknown as AnyPkg;

describe('npmSearch — npm-version vs repo-release divergence warning', () => {
  it('warns when a version AND a GitHub repository pointer are both present', () => {
    const data = formatPackageData(npmPkg({}));
    expect(data.warnings).toBeDefined();
    expect(data.warnings![0]).toContain('7.0.2');
    expect(data.warnings![0]).toContain('may not correspond to a release/tag');
  });

  it('does not warn without a repository pointer', () => {
    const data = formatPackageData(npmPkg({ repoUrl: undefined }));
    expect(data.warnings).toBeUndefined();
  });

  it('does not warn for non-GitHub repositories', () => {
    const data = formatPackageData(
      npmPkg({ repoUrl: 'https://gitlab.com/foo/bar' })
    );
    expect(data.warnings).toBeUndefined();
  });

  it('does not warn when the version is unknown', () => {
    const data = formatPackageData(npmPkg({ version: 'unknown' }));
    expect(data.warnings).toBeUndefined();
  });
});
