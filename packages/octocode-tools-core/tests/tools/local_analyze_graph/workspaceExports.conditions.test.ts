import { mkdtemp, rm, writeFile, mkdir } from 'node:fs/promises';
import { join, relative } from 'node:path';
import { fileURLToPath } from 'node:url';
import { execFileSync } from 'node:child_process';
import { afterEach, describe, expect, it } from 'vitest';
import { buildWorkspacePackageExports } from '../../../src/graph/workspacePackageResolver.js';
import { buildFileGraph } from '../../../src/graph/buildFileGraph.js';

const roots: string[] = [];
afterEach(async () => {
  await Promise.all(
    roots.splice(0).map(root => rm(root, { recursive: true, force: true }))
  );
});

async function fixture(exports: unknown) {
  const root = await mkdtemp(join(process.cwd(), '.tmp-conditional-exports-'));
  roots.push(root);
  await writeFile(
    join(root, 'package.json'),
    JSON.stringify({ name: 'fixture', type: 'module', exports })
  );
  const files = [
    'import.mjs',
    'require.cjs',
    'fallback.js',
    'types.d.ts',
    'features/a.js',
    'special/a.js',
  ];
  for (const file of files) {
    await mkdir(join(root, file, '..'), { recursive: true });
    await writeFile(
      join(root, file),
      file.endsWith('.d.ts') ? 'export interface Shape {}' : ''
    );
  }
  await writeFile(
    join(root, 'oracle.mjs'),
    "import { createRequire } from 'node:module'; console.log(process.argv[3] === 'require' ? createRequire(import.meta.url).resolve(process.argv[2]) : import.meta.resolve(process.argv[2]));"
  );
  return { root, files: new Set(files) };
}

describe('workspace conditional exports', () => {
  it('keeps unknown TypeScript emit contexts explicit and distinguishes static from dynamic loads', async () => {
    const { root } = await fixture({
      import: { types: './types.d.ts', default: './import.mjs' },
      require: { types: './types.d.ts', default: './require.cjs' },
    });
    for (const extension of ['mts', 'cts', 'ts']) {
      await writeFile(
        join(root, `static.${extension}`),
        "export * from 'fixture';"
      );
      await writeFile(
        join(root, `dynamic.${extension}`),
        "void import('fixture');"
      );
      await writeFile(
        join(root, `type.${extension}`),
        "export type { Shape } from 'fixture';"
      );
    }
    await writeFile(
      join(root, 'create.mjs'),
      "import { createRequire } from 'node:module'; const req = createRequire(import.meta.url); req('fixture');"
    );
    const graph = await buildFileGraph(root, [], 50);
    const targets = (file: string) => [
      ...graph.fileGraph.get(file)!.importsFiles,
    ];
    expect(targets('static.mts')).toEqual(['import.mjs']);
    expect(targets('static.cts')).toEqual(['require.cjs']);
    expect(targets('static.ts')).toEqual([]);
    expect(targets('create.mjs')).toEqual(['require.cjs']);
    for (const extension of ['mts', 'cts', 'ts']) {
      expect(targets(`dynamic.${extension}`)).toEqual(['import.mjs']);
      expect(targets(`type.${extension}`)).toEqual(['types.d.ts']);
    }
    expect(graph.coverage.diagnostics).toContainEqual(
      expect.objectContaining({
        file: 'static.ts',
        code: 'unsupported-linking',
        message: expect.stringContaining('package-export-context-unavailable'),
      })
    );
  });

  it('does not guess between duplicate package names or exceed the selection depth budget', async () => {
    const { root, files } = await fixture('./import.mjs');
    await mkdir(join(root, 'duplicate'));
    await writeFile(
      join(root, 'duplicate/package.json'),
      JSON.stringify({ name: 'fixture', exports: './other.js' })
    );
    files.add('duplicate/other.js');
    expect(
      buildWorkspacePackageExports(root, files).resolve('fixture', 'import')
    ).toMatchObject({
      target: null,
      status: 'unsupported',
      reason: 'ambiguous-workspace-package',
    });
    let nested: unknown = './import.mjs';
    for (let index = 0; index < 40; index++) nested = { node: nested };
    const deep = await fixture(nested);
    expect(
      buildWorkspacePackageExports(deep.root, deep.files).resolve(
        'fixture',
        'import'
      )
    ).toMatchObject({ target: null, status: 'unsupported' });
  });

  it('passes distinct import, require, and type-only contexts through native graph facts', async () => {
    const { root } = await fixture({
      types: './types.d.ts',
      require: './require.cjs',
      import: './import.mjs',
    });
    await writeFile(
      join(root, 'consumer.mts'),
      "import value from 'fixture';\nvoid import('fixture');\n"
    );
    await writeFile(
      join(root, 'consumer.cjs'),
      "const value = require('fixture');\n"
    );
    await writeFile(
      join(root, 'types.mts'),
      "import type { Shape } from 'fixture';\n"
    );
    const graph = await buildFileGraph(root, [], 50);
    expect([...graph.fileGraph.get('consumer.mts')!.importsFiles]).toEqual([
      'import.mjs',
    ]);
    expect([...graph.fileGraph.get('consumer.cjs')!.importsFiles]).toEqual([
      'require.cjs',
    ]);
    expect([...graph.fileGraph.get('types.mts')!.importsFiles]).toEqual([
      'types.d.ts',
    ]);
  });

  it.each([
    {
      exports: { default: './fallback.js', import: './import.mjs' },
      specifier: 'fixture',
    },
    {
      exports: {
        node: { require: './require.cjs', import: './import.mjs' },
        default: './fallback.js',
      },
      specifier: 'fixture',
    },
    {
      exports: { node: { types: './types.d.ts' }, default: './fallback.js' },
      specifier: 'fixture',
    },
    { exports: ['../invalid.js', null, './fallback.js'], specifier: 'fixture' },
    {
      exports: { node: [{ types: './types.d.ts' }], default: './fallback.js' },
      specifier: 'fixture',
    },
    { exports: [false, './fallback.js'], specifier: 'fixture' },
    {
      exports: { import: { node: './import.mjs' }, require: './require.cjs' },
      specifier: 'fixture',
    },
    {
      exports: { './*': './features/*', './feature/*.js': './special/*.js' },
      specifier: 'fixture/feature/a.js',
    },
    {
      exports: { './*': './features/*', './a.js': './fallback.js' },
      specifier: 'fixture/a.js',
    },
  ])(
    'matches Node resolve-only oracle for $specifier with $exports',
    async testCase => {
      const { root, files } = await fixture(testCase.exports);
      const resolver = buildWorkspacePackageExports(root, files);
      for (const mode of ['import', 'require'] as const) {
        const output = execFileSync(
          process.execPath,
          [join(root, 'oracle.mjs'), testCase.specifier, mode],
          { encoding: 'utf8' }
        ).trim();
        const expected = relative(
          root,
          mode === 'import' ? fileURLToPath(output) : output
        );
        expect(resolver.resolve(testCase.specifier, mode)).toEqual({
          target: expected,
          status: 'resolved',
        });
      }
    }
  );

  it.each([
    {
      exports: { './private/*': null, './*': './features/*' },
      specifier: 'fixture/private/a.js',
      status: 'unresolvedInternal',
    },
    {
      exports: ['./missing.js', './fallback.js'],
      specifier: 'fixture',
      status: 'unresolvedInternal',
    },
    {
      exports: { node: null, default: './fallback.js' },
      specifier: 'fixture',
      status: 'unresolvedInternal',
    },
    {
      exports: { '.': './import.mjs' },
      specifier: 'fixture/',
      status: 'unresolvedInternal',
    },
    {
      exports: { './blocked.js': null, './*': './features/*' },
      specifier: 'fixture/blocked.js',
      status: 'unresolvedInternal',
    },
    {
      exports: { development: './import.mjs', default: './fallback.js' },
      specifier: 'fixture',
      status: 'unsupported',
    },
    {
      exports: { 'types@>=5': './types.d.ts', types: './types.d.ts' },
      specifier: 'fixture',
      status: 'unsupported',
      mode: 'types-import',
    },
    {
      exports: './features/../fallback.js',
      specifier: 'fixture',
      status: 'unsupported',
    },
    {
      exports: { '.': './import.mjs', import: './require.cjs' },
      specifier: 'fixture',
      status: 'unsupported',
    },
  ])('keeps $status explicit for $exports', async testCase => {
    const { root, files } = await fixture(testCase.exports);
    expect(
      buildWorkspacePackageExports(root, files).resolve(
        testCase.specifier,
        testCase.mode ?? 'import'
      )
    ).toMatchObject({ target: null, status: testCase.status });
  });
});
