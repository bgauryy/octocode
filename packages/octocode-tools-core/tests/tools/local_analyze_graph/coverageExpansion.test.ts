import { mkdir, mkdtemp, rm, symlink, writeFile } from 'node:fs/promises';
import { dirname, join } from 'node:path';
import { afterEach, describe, expect, it } from 'vitest';
import { analyzeGraph } from '../../../src/tools/local_analyze_graph/analyzeGraph.js';
import { LocalAnalyzeGraphQuerySchema } from '../../../src/tools/local_analyze_graph/scheme.js';

const roots: string[] = [];
afterEach(async () => {
  await Promise.all(
    roots.splice(0).map(root => rm(root, { recursive: true, force: true }))
  );
});

async function fixture(files: Record<string, string>): Promise<string> {
  const root = await mkdtemp(join(process.cwd(), '.tmp-graph-coverage-'));
  roots.push(root);
  for (const [file, content] of Object.entries(files)) {
    await mkdir(dirname(join(root, file)), { recursive: true });
    await writeFile(join(root, file), content);
  }
  return root;
}

describe('bounded graph coverage', () => {
  it('links native CommonJS facts with distinct loader provenance', async () => {
    const path = await fixture({
      'index.ts':
        "import { createRequire } from 'node:module'; const load = createRequire(import.meta.url); const name = './created.cjs'; load(name); require('./global.cjs');",
      'created.cjs': 'module.exports = 1;',
      'global.cjs': 'module.exports = 2;',
    });
    const result = await analyzeGraph({
      operation: 'dependencies',
      path,
      file: 'index.ts',
    });
    expect(result.results).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          file: 'created.cjs',
          edgeKinds: ['create-require'],
        }),
        expect.objectContaining({
          file: 'global.cjs',
          edgeKinds: ['commonjs-require'],
        }),
      ])
    );
    expect(result.coverage?.imports.unsupported).toBe(0);
  });

  it('does not turn shadowed require functions into module edges or coverage gaps', async () => {
    const path = await fixture({
      'index.ts': "function run(require: any) { return require('./fake.ts'); }",
      'fake.ts': 'export const fake = 1;',
    });
    const result = await analyzeGraph({
      operation: 'dependencies',
      path,
      file: 'index.ts',
    });
    expect(result.results).toEqual([]);
    expect(result.coverage?.imports.unsupported).toBe(0);
  });

  it('retains explicit diagnostics for computed CommonJS specifiers', async () => {
    const path = await fixture({ 'index.cjs': 'require(getModuleName());' });
    const result = await analyzeGraph({
      operation: 'dependencies',
      path,
      file: 'index.cjs',
    });
    expect(result.coverage?.diagnostics).toContainEqual(
      expect.objectContaining({
        file: 'index.cjs',
        line: 1,
        code: 'unsupported-linking',
        message: expect.stringContaining('non-literal-specifier'),
      })
    );
  });

  it('retains module exports reached through CommonJS and namespace imports', async () => {
    const path = await fixture({
      'index.ts':
        "require('./required.js'); import * as values from './namespace.js'; console.log(values);",
      'required.ts': 'export const fromRequire = 1;',
      'namespace.ts': 'export const fromNamespace = 2;',
    });
    const result = await analyzeGraph({
      operation: 'deadCode',
      path,
      entrypoints: ['index.ts'],
    });
    expect(result.results).toEqual([]);
  });

  it('executes diagnostic pages without dropping distinct unsupported load occurrences', async () => {
    const path = await fixture({
      'index.cjs': 'require(a);\nrequire(b);\nrequire(c);\n',
    });
    let result = await analyzeGraph({
      operation: 'dependencies',
      path,
      file: 'index.cjs',
      diagnosticPageSize: 1,
    });
    const diagnostics = [...result.coverage!.diagnostics];
    while (result.coverage?.diagnosticsPagination?.hasMore) {
      const next = result.next!.nextDiagnostics as { query: unknown };
      result = await analyzeGraph(
        LocalAnalyzeGraphQuerySchema.parse(next.query)
      );
      diagnostics.push(...result.coverage!.diagnostics);
    }
    expect(diagnostics.map(item => item.line)).toEqual([1, 2, 3]);
    expect(result.coverage?.imports.unsupported).toBe(3);
  });

  it('links in-root manifests and the nearest package manifest as bounded metadata nodes', async () => {
    const root = await fixture({
      'package.json': '{"name":"fixture"}',
      'src/index.ts':
        "import parent from '../package.json'; import local from './local/package.json'; console.log(parent, local);",
      'src/local/package.json': '{"name":"local"}',
    });
    const result = await analyzeGraph({
      operation: 'dependencies',
      path: join(root, 'src'),
      file: 'index.ts',
    });
    expect(result.results.map(row => row.file)).toEqual([
      '../package.json',
      'local/package.json',
    ]);
    expect(
      result.results.every(row => row.edgeKinds?.includes('metadata-import'))
    ).toBe(true);
    expect(result.coverage?.imports.unresolvedInternal).toBe(0);
    expect(result.filesScanned).toBe(3);
  });

  it('does not follow arbitrary manifests outside the selected root', async () => {
    const root = await fixture({
      'package.json': '{"name":"fixture"}',
      'src/index.ts':
        "import other from '../other/package.json'; console.log(other);",
      'other/package.json': '{"name":"other"}',
    });
    const result = await analyzeGraph({
      operation: 'dependencies',
      path: join(root, 'src'),
      file: 'index.ts',
    });
    expect(result.results).toEqual([]);
    expect(result.coverage?.imports.unsupported).toBe(1);
  });

  it('rejects oversized, malformed, and symlinked manifests without reading them as source nodes', async () => {
    const root = await fixture({
      'index.ts':
        "import './big/package.json'; import './bad/package.json'; import './link/package.json';",
      'big/package.json': JSON.stringify({ text: 'x'.repeat(65_536) }),
      'bad/package.json': '{invalid',
      'real/package.json': '{}',
    });
    await mkdir(join(root, 'link'));
    await symlink(
      join(root, 'real/package.json'),
      join(root, 'link/package.json')
    );
    const result = await analyzeGraph({
      operation: 'dependencies',
      path: root,
      file: 'index.ts',
    });
    expect(result.results).toEqual([]);
    expect(result.filesScanned).toBe(1);
    expect(result.coverage?.diagnostics).toHaveLength(3);
    expect(result.next?.expandScan).toBeUndefined();
  });

  it('preserves executable expansion when metadata nodes fill the scan budget', async () => {
    const path = await fixture({
      'index.ts': "import pkg from './package.json'; console.log(pkg);",
      'package.json': '{"name":"fixture"}',
    });
    const first = await analyzeGraph({
      operation: 'dependencies',
      path,
      file: 'index.ts',
      maxFiles: 1,
    });
    expect(first.terminalLimit).toBeUndefined();
    const next = first.next?.expandScan as { query: unknown };
    expect(next).toBeDefined();
    const second = await analyzeGraph(
      LocalAnalyzeGraphQuerySchema.parse(next.query)
    );
    expect(second.results).toEqual([
      expect.objectContaining({ file: 'package.json' }),
    ]);
    expect(second.partialReasons).toBeUndefined();
    expect(second.filesScanned).toBe(2);
  });

  it('links Python modules and their known package initializers without name guessing', async () => {
    const path = await fixture({
      'pkg/__init__.py': '',
      'pkg/service.py': 'from .worker import run\nrun()\n',
      'pkg/worker.py': 'def run():\n    return 1\n',
    });
    const result = await analyzeGraph({
      operation: 'dependencies',
      path,
      file: 'pkg/service.py',
    });
    expect(result.results.map(row => row.file)).toEqual([
      'pkg/__init__.py',
      'pkg/worker.py',
    ]);
    expect(
      result.results.every(row => row.edgeKinds?.includes('python-import'))
    ).toBe(true);
    expect(result.coverage?.imports.unsupported).toBe(0);
  });

  it('reports ambiguous Python module layouts instead of choosing a same-named target', async () => {
    const path = await fixture({
      'index.py': 'import worker\n',
      'worker.py': 'def run(): pass\n',
      'worker/__init__.py': 'def run(): pass\n',
    });
    const result = await analyzeGraph({
      operation: 'dependencies',
      path,
      file: 'index.py',
    });
    expect(result.results).toEqual([]);
    expect(result.coverage?.imports.unsupported).toBeGreaterThan(0);
  });

  it('does not introduce an initializer self-cycle when a package imports its own module', async () => {
    const path = await fixture({
      'pkg/__init__.py': 'from .worker import run\n',
      'pkg/worker.py': 'def run(): pass\n',
    });
    const dependencies = await analyzeGraph({
      operation: 'dependencies',
      path,
      file: 'pkg/__init__.py',
    });
    expect(dependencies.results.map(row => row.file)).toEqual([
      'pkg/worker.py',
    ]);
    const cycles = await analyzeGraph({ operation: 'cycles', path });
    expect(cycles.results).toEqual([]);
  });

  it('links a unique bounded Python package submodule and retains its namespace', async () => {
    const path = await fixture({
      'index.py': 'from pkg import worker\nworker.run()\n',
      'pkg/__init__.py': '',
      'pkg/worker.py': 'def run(): pass\n',
    });
    const dependencies = await analyzeGraph({
      operation: 'dependencies',
      path,
      file: 'index.py',
    });
    expect(dependencies.results.map(row => row.file)).toEqual([
      'pkg/__init__.py',
      'pkg/worker.py',
    ]);
    expect(dependencies.coverage?.imports.unsupported).toBe(0);
    const deadCode = await analyzeGraph({
      operation: 'deadCode',
      path,
      entrypoints: ['index.py'],
    });
    expect(deadCode.results).toEqual([]);
  });

  it('reports Python package attributes and ambiguous submodules as unsupported while retaining the initializer', async () => {
    const path = await fixture({
      'index.py': 'from pkg import value, worker\n',
      'pkg/__init__.py': 'value = 1\n',
      'pkg/worker.py': 'def run(): pass\n',
      'pkg/worker/__init__.py': '',
    });
    const result = await analyzeGraph({
      operation: 'dependencies',
      path,
      file: 'index.py',
    });
    expect(result.results.map(row => row.file)).toEqual(['pkg/__init__.py']);
    expect(result.coverage?.imports.unsupported).toBe(2);
    expect(result.partialReasons).toContain('unsupportedLinking');
  });

  it('links quoted C headers while leaving system and macro include resolution explicit', async () => {
    const path = await fixture({
      'index.c': '#include "local.h"\n#include <system.h>\n#include HEADER\n',
      'local.h': 'int local(void);\n',
      'system.h': 'int unrelated(void);\n',
    });
    const result = await analyzeGraph({
      operation: 'dependencies',
      path,
      file: 'index.c',
    });
    expect(result.results).toEqual([
      expect.objectContaining({ file: 'local.h', edgeKinds: ['c-include'] }),
    ]);
    expect(result.coverage?.imports.unsupported).toBe(2);
  });
});
