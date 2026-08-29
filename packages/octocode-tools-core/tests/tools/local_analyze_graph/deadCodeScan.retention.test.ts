import { mkdtemp, mkdir, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';

import { afterEach, describe, expect, it } from 'vitest';

import { scanForDeadCode } from '../../../src/tools/local_analyze_graph/deadCodeScan.js';
import { analyzeGraph } from '../../../src/tools/local_analyze_graph/analyzeGraph.js';

const tempDirs: string[] = [];

afterEach(async () => {
  await Promise.all(
    tempDirs.splice(0).map(dir => rm(dir, { recursive: true, force: true }))
  );
});

async function createTempDir(): Promise<string> {
  const dir = await mkdtemp(join(process.cwd(), '.tmp-dead-code-retention-'));
  tempDirs.push(dir);
  return dir;
}

describe('star-barrel retention (named re-export → star re-export → entrypoint)', () => {
  it('retains an export whose named re-export flows through a star-re-exported barrel', async () => {
    // types.ts --named reexport--> barrel/index.ts --export * --> index.ts (entrypoint)
    const dir = await createTempDir();
    await writeFile(
      join(dir, 'package.json'),
      JSON.stringify({ name: 'fixture', main: 'index.js' })
    );
    await mkdir(join(dir, 'barrel'));
    await writeFile(
      join(dir, 'types.js'),
      'export const CONFIG_FILE_NAME = "x";\nexport const trulyDead = 1;\n'
    );
    await writeFile(
      join(dir, 'barrel', 'index.js'),
      "export { CONFIG_FILE_NAME } from '../types.js';\n"
    );
    await writeFile(
      join(dir, 'index.js'),
      "export * from './barrel/index.js';\n"
    );

    const result = scanForDeadCode(dir, { includeTests: false });

    const names = result.deadExports.map(d => d.name);
    // Republished through the star barrel — public API, must NOT be flagged.
    expect(names).not.toContain('CONFIG_FILE_NAME');
    // Control: an export nobody re-exports or imports must still be flagged.
    expect(names).toContain('trulyDead');
  });

  it('retains through a chain of star re-exports (star → star → entrypoint)', async () => {
    const dir = await createTempDir();
    await writeFile(
      join(dir, 'package.json'),
      JSON.stringify({ name: 'fixture', main: 'index.js' })
    );
    await writeFile(join(dir, 'leaf.js'), 'export const leafValue = 1;\n');
    await writeFile(
      join(dir, 'mid.js'),
      "export { leafValue } from './leaf.js';\n"
    );
    await writeFile(join(dir, 'outer.js'), "export * from './mid.js';\n");
    await writeFile(join(dir, 'index.js'), "export * from './outer.js';\n");

    const result = scanForDeadCode(dir, { includeTests: false });
    expect(result.deadExports.map(d => d.name)).not.toContain('leafValue');
  });

  it('does NOT retain via a star barrel that is itself unreachable', async () => {
    const dir = await createTempDir();
    await writeFile(
      join(dir, 'package.json'),
      JSON.stringify({ name: 'fixture', main: 'index.js' })
    );
    await writeFile(join(dir, 'types.js'), 'export const orphaned = 1;\n');
    // deadBarrel star-re-exports types.js but nothing reaches deadBarrel.
    await writeFile(
      join(dir, 'deadBarrel.js'),
      "export * from './types.js';\n"
    );
    // The entrypoint imports types.js so types.js is reachable, but `orphaned`
    // itself is only "re-exported" by the unreachable barrel.
    await writeFile(
      join(dir, 'index.js'),
      "import './types.js';\nexport const api = 1;\n"
    );

    const result = scanForDeadCode(dir, { includeTests: false });
    expect(result.deadExports.map(d => d.name)).toContain('orphaned');
  });
});

describe('symbol-level same-file retention (dead callers must not retain callees)', () => {
  it('flags an export whose only same-file caller is itself a dead export', async () => {
    const dir = await createTempDir();
    await writeFile(
      join(dir, 'package.json'),
      JSON.stringify({ name: 'fixture', main: 'index.js' })
    );
    await writeFile(
      join(dir, 'lib.js'),
      [
        'export function used() {',
        '  return 1;',
        '}',
        'export function deadCaller() {',
        '  return deadCallee();',
        '}',
        'export function deadCallee() {',
        '  return 2;',
        '}',
        '',
      ].join('\n')
    );
    await writeFile(
      join(dir, 'index.js'),
      "import { used } from './lib.js';\nexport const api = used();\n"
    );

    const result = scanForDeadCode(dir, { includeTests: false });
    const names = result.deadExports.map(d => d.name);
    expect(names).toContain('deadCaller');
    // deadCallee's only usage is FROM a dead export — symbol-level liveness
    // must not let the dead caller retain it.
    expect(names).toContain('deadCallee');
    expect(names).not.toContain('used');
  });

  it('retains an export called by a live export in the same file', async () => {
    const dir = await createTempDir();
    await writeFile(
      join(dir, 'package.json'),
      JSON.stringify({ name: 'fixture', main: 'index.js' })
    );
    await writeFile(
      join(dir, 'lib.js'),
      [
        'export function entryUsed() {',
        '  return helperExported();',
        '}',
        'export function helperExported() {',
        '  return 2;',
        '}',
        '',
      ].join('\n')
    );
    await writeFile(
      join(dir, 'index.js'),
      "import { entryUsed } from './lib.js';\nentryUsed();\n"
    );

    const result = scanForDeadCode(dir, { includeTests: false });
    const names = result.deadExports.map(d => d.name);
    expect(names).not.toContain('entryUsed');
    expect(names).not.toContain('helperExported');
  });

  it('still retains value-references (non-call usage) via the lexical fallback', async () => {
    const dir = await createTempDir();
    await writeFile(
      join(dir, 'package.json'),
      JSON.stringify({ name: 'fixture', main: 'index.js' })
    );
    await writeFile(
      join(dir, 'lib.js'),
      [
        'export const DEFAULTS = { a: 1 };',
        'export function buildConfig() {',
        '  return { ...DEFAULTS, b: 2 };',
        '}',
        '',
      ].join('\n')
    );
    await writeFile(
      join(dir, 'index.js'),
      "import { buildConfig } from './lib.js';\nbuildConfig();\n"
    );

    const result = scanForDeadCode(dir, { includeTests: false });
    expect(result.deadExports.map(d => d.name)).not.toContain('DEFAULTS');
  });
});

describe('viaHeuristic on unreferenced-export candidates', () => {
  it('tags a candidate whose re-export chain terminated dead as "reexport-chain" and a plain one as "lexical-count"', async () => {
    const dir = await createTempDir();
    await writeFile(
      join(dir, 'package.json'),
      JSON.stringify({ name: 'fixture', main: 'index.js' })
    );
    await writeFile(
      join(dir, 'lib.js'),
      'export const reexportedButUnused = 1;\nexport const plainDead = 2;\n'
    );
    // deadBarrel named-re-exports it, but deadBarrel itself is reachable yet
    // nothing imports the binding from it.
    await writeFile(
      join(dir, 'deadBarrel.js'),
      "export { reexportedButUnused } from './lib.js';\nexport const barrelOwn = 1;\n"
    );
    await writeFile(
      join(dir, 'index.js'),
      "import './lib.js';\nimport { barrelOwn } from './deadBarrel.js';\nexport const api = barrelOwn;\n"
    );

    const result = scanForDeadCode(dir, { includeTests: false });
    const byName = new Map(result.deadExports.map(d => [d.name, d]));

    expect(byName.get('reexportedButUnused')?.reason).toBe(
      'unreferenced-export'
    );
    expect(byName.get('reexportedButUnused')?.viaHeuristic).toBe(
      'reexport-chain'
    );
    expect(byName.get('plainDead')?.viaHeuristic).toBe('lexical-count');
  });
});

describe('next.verifyReferences escalation hint', () => {
  it('emits a prefilled lspGetSemantics references continuation for dead-export candidates', async () => {
    const dir = await createTempDir();
    await writeFile(
      join(dir, 'package.json'),
      JSON.stringify({ name: 'fixture', main: 'index.js' })
    );
    await writeFile(join(dir, 'lib.js'), 'export const plainDead = 2;\n');
    await writeFile(join(dir, 'index.js'), "import './lib.js';\n");

    const output = await analyzeGraph({
      operation: 'deadCode',
      path: dir,
      includeTests: false,
    });

    expect(output.results.length).toBeGreaterThan(0);
    const verify = output.next?.verifyReferences;
    expect(verify).toBeDefined();
    expect(verify?.tool).toBe('lspGetSemantics');
    expect(verify?.query).toMatchObject({
      type: 'references',
      symbolName: 'plainDead',
      lineHint: 1,
      includeDeclaration: false,
    });
    expect(String(verify?.query.uri)).toContain('lib.js');
    expect(verify?.why).toBeTruthy();
  });

  it('emits no verify hint when there are no candidates', async () => {
    const dir = await createTempDir();
    await writeFile(
      join(dir, 'package.json'),
      JSON.stringify({ name: 'fixture', main: 'index.js' })
    );
    await writeFile(join(dir, 'index.js'), 'export const api = 1;\n');

    const output = await analyzeGraph({
      operation: 'deadCode',
      path: dir,
      includeTests: false,
    });
    expect(output.next?.verifyReferences).toBeUndefined();
  });
});
