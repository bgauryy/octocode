import { mkdir, mkdtemp, rm, writeFile } from 'node:fs/promises';
import { spawnSync } from 'node:child_process';
import { dirname, join } from 'node:path';
import { afterEach, describe, expect, it } from 'vitest';
import { resolvePythonImport } from '../../../src/graph/languageImportResolver.js';
import { analyzeGraph } from '../../../src/tools/local_analyze_graph/analyzeGraph.js';

const roots: string[] = [];
afterEach(async () => {
  await Promise.all(
    roots.splice(0).map(root => rm(root, { recursive: true, force: true }))
  );
});
async function fixture(files: Record<string, string>) {
  const root = await mkdtemp(join(process.cwd(), '.tmp-python-relative-'));
  roots.push(root);
  for (const [file, content] of Object.entries(files)) {
    await mkdir(dirname(join(root, file)), { recursive: true });
    await writeFile(join(root, file), content);
  }
  return root;
}
const resolve = (
  specifier: string,
  importer: string,
  name: string,
  files: string[]
) =>
  resolvePythonImport(
    specifier,
    importer,
    name,
    'python-relative',
    new Set(files)
  );

describe('bounded relative Python imports', () => {
  it.each(['sys', 'builtins'])(
    'does not invent a local target for builtin %s collisions',
    async name => {
      const path = await fixture({
        'entry.py': `import ${name}\n`,
        [`${name}.py`]: 'raise RuntimeError("local collision must not run")\n',
      });
      const result = await analyzeGraph({
        operation: 'dependencies',
        path,
        file: 'entry.py',
      });
      expect(result.results).toEqual([]);
      expect(result.coverage?.imports.unsupported).toBe(1);
      expect(result.partialReasons).toContain('unsupportedLinking');
    }
  );

  it('checks the default Python import oracle for builtin collisions', async context => {
    const path = await fixture({
      'sys.py': 'raise RuntimeError("wrong sys")',
      'builtins.py': 'raise RuntimeError("wrong builtins")',
    });
    const result = spawnSync(
      'python3',
      [
        '-S',
        '-E',
        '-c',
        'import sys, builtins; print(sys.__spec__.origin, builtins.__spec__.origin)',
      ],
      { cwd: path, encoding: 'utf8', timeout: 5000, shell: false }
    );
    if (
      result.error &&
      'code' in result.error &&
      result.error.code === 'ENOENT'
    ) {
      context.skip(
        'Python oracle unavailable; product does not require Python.'
      );
      return;
    }
    expect(result.status, result.stderr).toBe(0);
    expect(result.stdout.trim()).toBe('built-in built-in');
  });

  it('retains explicitly relative sys module imports', () => {
    expect(
      resolve('.sys', 'pkg/entry.py', '', [
        'pkg/__init__.py',
        'pkg/entry.py',
        'pkg/sys.py',
      ])
    ).toMatchObject({ target: 'pkg/sys.py', status: 'resolved' });
  });

  it('links a unique same-package submodule and retains its namespace', () => {
    expect(
      resolve('.', 'pkg/service.py', 'worker', [
        'pkg/__init__.py',
        'pkg/service.py',
        'pkg/worker.py',
      ])
    ).toEqual({
      target: 'pkg/worker.py',
      status: 'resolved',
      additionalTargets: ['pkg/__init__.py', 'pkg/worker.py'],
    });
  });

  it('resolves parent-relative named packages and retains traversed package initializers', () => {
    const result = resolve('..pkg', 'app/sub/service.py', 'worker', [
      'app/__init__.py',
      'app/sub/__init__.py',
      'app/sub/service.py',
      'app/pkg/__init__.py',
      'app/pkg/worker.py',
    ]);
    expect(result.target).toBe('app/pkg/worker.py');
    expect(result.status).toBe('resolved');
    expect(result.additionalTargets).toEqual([
      'app/__init__.py',
      'app/pkg/__init__.py',
      'app/pkg/worker.py',
      'app/sub/__init__.py',
    ]);
  });

  it('supports a selected root that is itself a known package', () => {
    expect(
      resolve('.', 'service.py', 'worker', [
        '__init__.py',
        'service.py',
        'worker.py',
      ])
    ).toEqual({
      target: 'worker.py',
      status: 'resolved',
      additionalTargets: ['__init__.py', 'worker.py'],
    });
    expect(
      resolve('..', 'sub/service.py', 'worker', [
        '__init__.py',
        'sub/__init__.py',
        'sub/service.py',
        'worker.py',
      ])
    ).toMatchObject({ target: 'worker.py', status: 'resolved' });
  });

  it.each([
    ['..', 'service.py', ['__init__.py', 'service.py', 'worker.py']],
    [
      '...',
      'pkg/service.py',
      ['__init__.py', 'pkg/__init__.py', 'pkg/service.py', 'worker.py'],
    ],
    ['.', 'pkg/service.py', ['pkg/service.py', 'pkg/worker.py']],
    [
      '..pkg',
      'app/sub/service.py',
      [
        'app/sub/__init__.py',
        'app/sub/service.py',
        'app/pkg/__init__.py',
        'app/pkg/worker.py',
      ],
    ],
  ] as const)(
    'rejects out-of-root or unknown package context: %s from %s',
    (specifier, importer, files) => {
      expect(resolve(specifier, importer, 'worker', [...files])).toMatchObject({
        target: null,
        status: 'unsupported',
      });
    }
  );

  it.each([
    ['value', ['pkg/__init__.py', 'pkg/service.py']],
    [
      'worker',
      [
        'pkg/__init__.py',
        'pkg/service.py',
        'pkg/worker.py',
        'pkg/worker/__init__.py',
      ],
    ],
    [
      'worker',
      ['pkg/__init__.py', 'pkg/service.py', 'pkg/worker.py', 'pkg/worker.pyi'],
    ],
    ['*', ['pkg/__init__.py', 'pkg/service.py']],
  ] as const)(
    'keeps the initializer but diagnoses ambiguous/unsupported binding %s',
    (name, files) => {
      expect(resolve('.', 'pkg/service.py', name, [...files])).toMatchObject({
        target: 'pkg/__init__.py',
        status: 'unsupported',
      });
    }
  );

  it('recognizes a unique stub initializer and rejects conflicting package markers', () => {
    expect(
      resolve('.', 'pkg/service.pyi', 'worker', [
        'pkg/__init__.pyi',
        'pkg/service.pyi',
        'pkg/worker.pyi',
      ])
    ).toMatchObject({
      target: 'pkg/worker.pyi',
      status: 'resolved',
      additionalTargets: ['pkg/__init__.pyi', 'pkg/worker.pyi'],
    });
    expect(
      resolve('.', 'pkg/service.py', 'worker', [
        'pkg/__init__.py',
        'pkg/__init__.pyi',
        'pkg/service.py',
        'pkg/worker.py',
      ])
    ).toMatchObject({ target: null, status: 'unsupported' });
  });

  it('never emits the importer as a primary or auxiliary dependency', () => {
    for (const [importer, name] of [
      ['pkg/service.py', 'service'],
      ['pkg/__init__.py', '__init__'],
    ]) {
      const result = resolve('.', importer!, name!, [
        'pkg/__init__.py',
        'pkg/service.py',
      ]);
      expect(result.status).toBe('resolved');
      expect(result.target).toBeNull();
      expect(result.additionalTargets).not.toContain(importer);
    }
  });

  it('links native from-dot alias facts without an initializer self-cycle', async () => {
    const path = await fixture({
      'pkg/__init__.py': 'from . import worker as alias\nalias.run()\n',
      'pkg/worker.py': 'def run():\n    return 1\n',
    });
    const deps = await analyzeGraph({
      operation: 'dependencies',
      path,
      file: 'pkg/__init__.py',
    });
    expect(deps.results.map(row => row.file)).toEqual(['pkg/worker.py']);
    expect(deps.coverage?.imports.unsupported).toBe(0);
    const cycles = await analyzeGraph({ operation: 'cycles', path });
    expect(cycles.results).toEqual([]);
    const dead = await analyzeGraph({
      operation: 'deadCode',
      path,
      entrypoints: ['pkg/__init__.py'],
    });
    expect(dead.results).toEqual([]);
  });

  it('links native parent-relative named and module-empty imports', async () => {
    const path = await fixture({
      'app/__init__.py': '',
      'app/sub/__init__.py': '',
      'app/sub/service.py':
        'from ..pkg import worker as alias\nfrom .. import helper\n',
      'app/pkg/__init__.py': '',
      'app/pkg/worker.py': 'def run(): pass\n',
      'app/helper.py': 'def helper(): pass\n',
    });
    const result = await analyzeGraph({
      operation: 'dependencies',
      path,
      file: 'app/sub/service.py',
    });
    expect(result.results.map(row => row.file).sort()).toEqual([
      'app/__init__.py',
      'app/helper.py',
      'app/pkg/__init__.py',
      'app/pkg/worker.py',
      'app/sub/__init__.py',
    ]);
    expect(result.coverage?.imports.unsupported).toBe(0);
  });

  it('exposes typed diagnostics for ambiguous relative imports without choosing a submodule', async () => {
    const path = await fixture({
      'pkg/__init__.py': 'value = 1\n',
      'pkg/service.py': 'from . import value, worker\n',
      'pkg/worker.py': '',
      'pkg/worker/__init__.py': '',
    });
    const result = await analyzeGraph({
      operation: 'dependencies',
      path,
      file: 'pkg/service.py',
    });
    expect(result.results.map(row => row.file)).toEqual(['pkg/__init__.py']);
    expect(result.coverage?.imports.unsupported).toBe(2);
    expect(result.coverage?.diagnostics).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          file: 'pkg/service.py',
          line: 1,
          code: 'unsupported-linking',
        }),
      ])
    );
  });
});
