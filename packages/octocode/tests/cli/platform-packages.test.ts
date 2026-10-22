import { describe, expect, it } from 'vitest';
import { readdirSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';

const read = (path: string) =>
  JSON.parse(readFileSync(resolve(__dirname, path), 'utf8')) as {
    version: string;
    optionalDependencies?: Record<string, string>;
    os?: string[];
    libc?: string[];
  };

describe('per-platform binary packages', () => {
  it('match the native package one for one, at the launcher version', () => {
    const cli = read('../../package.json');
    const native = read('../../../octocode-native/package.json');
    expect(cli.optionalDependencies).toEqual(native.optionalDependencies);
    for (const version of Object.values(cli.optionalDependencies ?? {}))
      expect(version).toBe(cli.version);
  });

  // Without `libc`, npm installs the gnu and the musl package on any Linux.
  it('name their libc on Linux', () => {
    const dir = '../../../octocode-native/npm';
    for (const name of readdirSync(resolve(__dirname, dir))) {
      const platform = read(`${dir}/${name}/package.json`);
      if (!platform.os?.includes('linux')) continue;
      expect(platform.libc, name).toEqual([name.endsWith('-musl') ? 'musl' : 'glibc']);
    }
  });
});
