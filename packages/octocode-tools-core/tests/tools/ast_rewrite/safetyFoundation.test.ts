import { afterEach, describe, expect, it } from 'vitest';
import {
  chmodSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  realpathSync,
  rmSync,
  writeFileSync,
} from 'node:fs';
import { createHash } from 'node:crypto';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { runAstRewrite } from '../../../src/tools/ast_rewrite/index.js';
import {
  acquireRootLock,
  type RootLock,
} from '../../../src/tools/ast_rewrite/rootLock.js';
import {
  applyTransaction,
  recoverTransactions,
  SimulatedTransactionCrash,
} from '../../../src/tools/ast_rewrite/transaction.js';

const roots: string[] = [];
const locks: RootLock[] = [];

afterEach(async () => {
  for (const lock of locks.splice(0)) await lock.release();
  for (const root of roots.splice(0)) {
    rmSync(root, { recursive: true, force: true });
  }
});

function temporaryRoot(prefix = 'octocode-ast-rewrite-safety-'): string {
  const root = mkdtempSync(join(tmpdir(), prefix));
  roots.push(root);
  return root;
}

function sha256(value: string | Buffer): string {
  return createHash('sha256').update(value).digest('hex');
}

function match(file: string, text: string, replacement: string) {
  return {
    file,
    text,
    replacement,
    language: 'TypeScript',
    range: {
      byteOffset: { start: 0, end: Buffer.byteLength(text) },
      start: { line: 0, column: 0 },
      end: { line: 0, column: text.length },
    },
    replacementOffsets: { start: 0, end: Buffer.byteLength(text) },
  };
}

function capableExecutable(
  root: string,
  matches: unknown[],
  options: { help?: string; marker?: string; log?: string } = {}
): string {
  const path = join(root, 'mock-ast-grep.mjs');
  const help =
    options.help ??
    '--pattern --rewrite --lang --json --globs --threads --color';
  writeFileSync(
    path,
    `#!/usr/bin/env node
import { writeFileSync } from 'node:fs';
if (process.argv[2] === '--version') process.stdout.write('ast-grep 0.45.0\\n');
else if (process.argv[2] === 'run' && process.argv[3] === '--help') process.stdout.write(${JSON.stringify(help)});
else {
  ${options.log ? `writeFileSync(${JSON.stringify(options.log)}, JSON.stringify({ cwd: process.cwd(), home: process.env.HOME, argv: process.argv.slice(2) }));` : ''}
  process.stdout.write(${JSON.stringify(JSON.stringify(matches))});
}
// ${options.marker ?? 'default'}
`
  );
  chmodSync(path, 0o755);
  return path;
}

describe('astRewrite safety foundation', () => {
  it('attests the tested executable bytes and required capability surface', async () => {
    const root = temporaryRoot();
    const source = 'oldCall(1);\n';
    writeFileSync(join(root, 'source.ts'), source);
    const executable = capableExecutable(root, [
      match('source.ts', 'oldCall(1)', 'newCall(1)'),
    ]);

    const result = await runAstRewrite(
      {
        path: root,
        langType: 'ts',
        pattern: 'oldCall($A)',
        rewrite: 'newCall($A)',
      },
      { executable }
    );

    expect(result.status).toBeUndefined();
    if (result.status !== undefined) return;
    expect(result.executable).toMatchObject({
      version: '0.45.0',
      sha256: sha256(readFileSync(executable)),
      capabilityContract: 1,
      capabilities: [
        'color',
        'globs',
        'json',
        'lang',
        'pattern',
        'rewrite',
        'threads',
      ],
    });
    expect(result.executable.capabilityDigest).toMatch(/^[a-f0-9]{64}$/);
  });

  it('rejects a version-shaped executable missing a required capability', async () => {
    const root = temporaryRoot();
    writeFileSync(join(root, 'source.ts'), 'oldCall(1);\n');
    const executable = capableExecutable(root, [], {
      help: '--pattern --lang --json --globs --threads --color',
    });

    const result = await runAstRewrite(
      {
        path: root,
        langType: 'ts',
        pattern: 'oldCall($A)',
        rewrite: 'newCall($A)',
      },
      { executable }
    );

    expect(result).toMatchObject({
      status: 'error',
      errorCode: 'ast.rewrite.capability_incompatible',
    });
  });

  it('runs the scan outside the repository without inheriting HOME or project config', async () => {
    const root = temporaryRoot();
    const log = join(root, 'invocation.json');
    writeFileSync(join(root, 'source.ts'), 'oldCall(1);\n');
    writeFileSync(
      join(root, 'sgconfig.yml'),
      'customLanguages:\n  unsafe:\n    libraryPath: ./untrusted.so\n'
    );
    const executable = capableExecutable(root, [], { log });

    const result = await runAstRewrite(
      {
        path: root,
        langType: 'ts',
        pattern: 'oldCall($A)',
        rewrite: 'newCall($A)',
      },
      { executable }
    );

    expect(result.status).toBe('empty');
    const invocation = JSON.parse(readFileSync(log, 'utf8')) as {
      cwd: string;
      home?: string;
      argv: string[];
    };
    expect(resolve(invocation.cwd)).not.toBe(resolve(root));
    expect(invocation.home).toBeUndefined();
    expect(invocation.argv.at(-1)).toBe(realpathSync(root));
  });

  it('excludes overlapping canonical roots across independent lock clients', async () => {
    const root = temporaryRoot();
    const child = join(root, 'child');
    mkdirSync(child);
    const first = await acquireRootLock(root, { timeoutMs: 100 });
    expect(first.ok).toBe(true);
    if (!first.ok) return;
    locks.push(first.lock);

    const blocked = await acquireRootLock(child, {
      timeoutMs: 40,
      pollMs: 5,
    });
    expect(blocked).toMatchObject({ ok: false, reason: 'timeout' });

    await first.lock.release();
    locks.splice(locks.indexOf(first.lock), 1);
    const acquired = await acquireRootLock(child, { timeoutMs: 100 });
    expect(acquired.ok).toBe(true);
    if (acquired.ok) locks.push(acquired.lock);
  });

  it.each(['after-backup:0', 'after-promote:0'])(
    'recovers the complete before-state after an interrupted %s transition',
    async faultPoint => {
      const root = temporaryRoot();
      const firstPath = join(root, 'a.ts');
      const secondPath = join(root, 'b.ts');
      writeFileSync(firstPath, 'before-a');
      writeFileSync(secondPath, 'before-b');

      const interrupted = await applyTransaction(
        [
          {
            absolutePath: firstPath,
            before: Buffer.from('before-a'),
            after: Buffer.from('after-a'),
            mode: 0o644,
          },
          {
            absolutePath: secondPath,
            before: Buffer.from('before-b'),
            after: Buffer.from('after-b'),
            mode: 0o644,
          },
        ],
        undefined,
        {
          rootBoundary: root,
          fault: point => {
            if (point === faultPoint)
              throw new SimulatedTransactionCrash(point);
          },
        }
      );
      expect(interrupted).toMatchObject({ ok: false, interrupted: true });

      const recovery = await recoverTransactions(root);
      expect(recovery).toMatchObject({ ok: true, recovered: 1 });
      expect(readFileSync(firstPath, 'utf8')).toBe('before-a');
      expect(readFileSync(secondPath, 'utf8')).toBe('before-b');
    }
  );

  it('finishes the complete after-state when interruption follows durable commit', async () => {
    const root = temporaryRoot();
    const path = join(root, 'source.ts');
    writeFileSync(path, 'before');
    const interrupted = await applyTransaction(
      [
        {
          absolutePath: path,
          before: Buffer.from('before'),
          after: Buffer.from('after'),
          mode: 0o644,
        },
      ],
      undefined,
      {
        rootBoundary: root,
        fault: point => {
          if (point === 'after-commit')
            throw new SimulatedTransactionCrash(point);
        },
      }
    );
    expect(interrupted).toMatchObject({ ok: false, interrupted: true });

    const recovery = await recoverTransactions(root);
    expect(recovery).toMatchObject({ ok: true, recovered: 1 });
    expect(readFileSync(path, 'utf8')).toBe('after');
  });

  it('reports a durable commit with cleanup warnings when finalization fails', async () => {
    const root = temporaryRoot();
    const path = join(root, 'source.ts');
    writeFileSync(path, 'before');
    const result = await applyTransaction(
      [
        {
          absolutePath: path,
          before: Buffer.from('before'),
          after: Buffer.from('after'),
          mode: 0o644,
        },
      ],
      undefined,
      {
        rootBoundary: root,
        fault: point => {
          if (point === 'after-commit')
            throw new Error('finalization unavailable');
        },
      }
    );
    try {
      expect(result).toMatchObject({
        ok: true,
        receipt: {
          committed: true,
          cleanupWarnings: ['finalization unavailable'],
        },
      });
      expect(readFileSync(path, 'utf8')).toBe('after');
    } finally {
      expect(await recoverTransactions(root)).toMatchObject({ ok: true });
    }
    expect(readFileSync(path, 'utf8')).toBe('after');
  });

  it('performs journal recovery before the next preview scan', async () => {
    const root = temporaryRoot();
    const path = join(root, 'source.ts');
    writeFileSync(path, 'oldCall(1);\n');
    const executable = capableExecutable(root, [
      match('source.ts', 'oldCall(1)', 'newCall(1)'),
    ]);
    const interrupted = await applyTransaction(
      [
        {
          absolutePath: path,
          before: Buffer.from('oldCall(1);\n'),
          after: Buffer.from('broken(1);\n'),
          mode: 0o644,
        },
      ],
      undefined,
      {
        rootBoundary: root,
        fault: point => {
          if (point === 'after-promote:0')
            throw new SimulatedTransactionCrash(point);
        },
      }
    );
    expect(interrupted).toMatchObject({ ok: false, interrupted: true });

    const preview = await runAstRewrite(
      {
        path: root,
        langType: 'ts',
        pattern: 'oldCall($A)',
        rewrite: 'newCall($A)',
      },
      { executable }
    );
    expect(preview.status, JSON.stringify(preview)).toBeUndefined();
    expect(readFileSync(path, 'utf8')).toBe('oldCall(1);\n');
  });
});
