import { afterEach, describe, expect, it } from 'vitest';
import { mkdtempSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';
import * as esbuild from 'esbuild';
import { coreBundleOptions } from '../build-options.mjs';

const roots: string[] = [];
afterEach(() => { for (const root of roots.splice(0)) rmSync(root, { recursive: true, force: true }); });

describe('live build generation coherence', () => {
  it('keeps lazy modules inside stable standalone entries across a rebuild', async () => {
    const root = mkdtempSync(join(tmpdir(), 'awareness-stable-build-'));
    roots.push(root);
    const entry = join(root, 'entry.ts');
    const lazy = join(root, 'lazy.ts');
    const outDir = join(root, 'out');
    writeFileSync(entry, "export async function load() { return (await import('./lazy.js')).generation; }");
    writeFileSync(lazy, "export const generation = 'old';");
    const build = () => esbuild.build({
      ...coreBundleOptions,
      entryPoints: { entry },
      outdir: outDir,
      logLevel: 'silent',
    });

    await build();
    const entryUrl = pathToFileURL(join(outDir, 'entry.js')).href;
    const oldGeneration = await import(`${entryUrl}?generation=old`);
    writeFileSync(lazy, "export const generation = 'new';");
    await build();

    expect(await oldGeneration.load()).toBe('old');
    expect(await (await import(`${entryUrl}?generation=new`)).load()).toBe('new');
    expect(readdirSync(outDir)).toEqual(['entry.js']);
  });
});
