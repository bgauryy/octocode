import { describe, expect, it } from 'vitest';
import { createRequire } from 'node:module';
import { readdirSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';

const read = (path: string) =>
  JSON.parse(readFileSync(resolve(__dirname, path), 'utf8')) as {
    version: string;
    optionalDependencies?: Record<string, string>;
    os?: string[];
    libc?: string[];
  };
const { PLATFORMS, platformPackageName } = createRequire(__filename)(
  '../../../octocode-native/bin/platform.cjs'
) as {
  PLATFORMS: Record<string, unknown>;
  platformPackageName: (suffix: string) => string;
};

describe('per-platform binary packages', () => {
  it('both launchers pin every platform package, at the launcher version', () => {
    for (const path of [
      '../../package.json',
      '../../../octocode-mcp/package.json',
    ]) {
      const launcher = read(path);
      expect(launcher.optionalDependencies, path).toEqual(
        Object.fromEntries(
          Object.keys(PLATFORMS).map(suffix => [
            platformPackageName(suffix),
            launcher.version,
          ])
        )
      );
    }
  });

  // Without `libc`, npm installs the gnu and the musl package on any Linux.
  it('name their libc on Linux', () => {
    const dir = '../../../octocode-native/npm';
    for (const name of readdirSync(resolve(__dirname, dir))) {
      const platform = read(`${dir}/${name}/package.json`);
      if (!platform.os?.includes('linux')) continue;
      expect(platform.libc, name).toEqual([
        name.endsWith('-musl') ? 'musl' : 'glibc',
      ]);
    }
  });
});
