import { afterEach, describe, expect, it } from 'vitest';
import {
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
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


describe('astRewrite safety foundation', () => {
  it('reports native engine receipt instead of binary attestation', async () => {
    // With the native engine there is no external binary to attest.
    // The receipt is a fixed constant that identifies the embedded engine.
    const root = temporaryRoot();
    writeFileSync(join(root, 'source.ts'), 'oldCall(1);\n');

    const result = await runAstRewrite(
      {
        ruleKind: 'pattern' as const,
        path: root,
        langType: 'ts',
        pattern: 'oldCall($A)',
        rewrite: 'newCall($A)',
      },
      {}
    );

    expect(result.status).toBeUndefined();
    if (result.status !== undefined) return;
    expect(result.executable).toMatchObject({
      path: 'native',
      version: 'embedded',
      sha256: '',
      capabilityContract: 1,
      capabilityDigest: 'native',
      capabilities: ['pattern', 'inline-rules', 'experimental'],
    });
  });

  it.skip('rejects a version-shaped executable missing a required capability', () => {
    // capability_incompatible was emitted when the external ast-grep binary
    // lacked a required CLI flag. The native engine has a fixed capability set;
    // this check no longer applies.
  });

  it.skip('runs the scan outside the repository without inheriting HOME or project config', () => {
    // Process isolation (ephemeral CWD, no HOME) was required when the scan ran
    // as a subprocess. The native Rust engine runs in-process with no inherited
    // environment exposure; this test is not applicable.
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
        ruleKind: 'pattern' as const,
        path: root,
        langType: 'ts',
        pattern: 'oldCall($A)',
        rewrite: 'newCall($A)',
      },
      {}
    );
    expect(preview.status, JSON.stringify(preview)).toBeUndefined();
    expect(readFileSync(path, 'utf8')).toBe('oldCall(1);\n');
  });
});
