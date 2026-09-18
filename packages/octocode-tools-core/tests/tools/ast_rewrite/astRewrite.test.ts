import { afterEach, describe, expect, it, vi } from 'vitest';
import {
  chmodSync,
  mkdtempSync,
  mkdirSync,
  readFileSync,
  realpathSync,
  rmSync,
  symlinkSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { basename, join } from 'node:path';
import { rename } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { recoverTransactions } from '../../../src/tools/ast_rewrite/transaction.js';

import {
  runAstRewrite,
  type AstRewriteRuntimeDeps,
} from '../../../src/tools/ast_rewrite/index.js';
import {
  resetContextUtilsNativeLoaderForTesting,
  setContextUtilsNativeLoaderForTesting,
} from '../../../src/utils/contextUtils.js';

type NativeModule = typeof import('@octocodeai/octocode-engine');

/**
 * Install a lightweight native-loader override for the named methods only.
 * Only the methods listed in `overrides` are accessed during the test;
 * the real engine is not loaded.
 */
function withRewriteMock(
  overrides: Partial<NativeModule>
): void {
  setContextUtilsNativeLoaderForTesting(
    () => overrides as unknown as NativeModule
  );
}

const roots: string[] = [];

afterEach(() => {
  resetContextUtilsNativeLoaderForTesting();
  for (const root of roots.splice(0)) {
    rmSync(root, { recursive: true, force: true });
  }
});

function fixture() {
  const root = mkdtempSync(join(tmpdir(), 'octocode-ast-rewrite-'));
  roots.push(root);
  const source = 'const first = oldCall(1);\nconst second = oldCall(2);\n';
  writeFileSync(join(root, 'source.ts'), source);
  return { root, source };
}

function digest(value: string | Buffer): string {
  return createHash('sha256').update(value).digest('hex');
}

function mockExecutable(root: string, matches: unknown[], version = '0.40.1') {
  const path = join(root, 'mock-ast-grep.mjs');
  const program = `#!/usr/bin/env node
if (process.argv[2] === '--version') {
  process.stdout.write(${JSON.stringify(`ast-grep ${version}\n`)});
} else if (process.argv[2] === 'run' && process.argv[3] === '--help') {
  process.stdout.write('--pattern --rewrite --lang --json --globs --threads --color');
} else {
  process.stdout.write(${JSON.stringify(JSON.stringify(matches))});
}
`;
  writeFileSync(path, program);
  chmodSync(path, 0o755);
  return path;
}

function match(
  file: string,
  start: number,
  end: number,
  text: string,
  replacement: string
) {
  return {
    file,
    text,
    replacement,
    language: 'TypeScript',
    range: {
      byteOffset: { start, end },
      start: { line: 0, column: start },
      end: { line: 0, column: end },
    },
    replacementOffsets: { start, end },
  };
}

function query(root: string) {
  return {
    ruleKind: 'pattern' as const,
    path: root,
    langType: 'ts',
    pattern: 'oldCall($A)',
    rewrite: 'newCall($A)',
  };
}

async function previewForApply(
  root: string,
  overrides: Record<string, unknown> = {}
) {
  const preview = await runAstRewrite(
    { ...query(root), ...overrides },
    {}
  );
  expect(preview.status).toBeUndefined();
  if (preview.status !== undefined) throw new Error('preview failed');
  return {
    snapshot: preview.snapshot,
    expectedHashes: Object.fromEntries(
      preview.files.map(file => [file.absolutePath, file.beforeHash])
    ),
  };
}

describe('runAstRewrite', () => {
  it.skip('reports a process output cap as a terminal evidence limit without writing', () => {
    // maxProcessOutputBytes was a binary-process output cap.
    // The native engine streams results directly; there is no subprocess output
    // to cap. The test is not applicable to the native implementation.
  });
  it('previews stable matches, hashes, bounded patches, and pagination without writing', async () => {
    const { root, source } = fixture();
    const first = source.indexOf('oldCall(1)');
    const second = source.indexOf('oldCall(2)');
    const executable = mockExecutable(root, [
      match('source.ts', first, first + 10, 'oldCall(1)', 'newCall(1)'),
      match('source.ts', second, second + 10, 'oldCall(2)', 'newCall(2)'),
    ]);

    const result = await runAstRewrite(
      { ...query(root), page: 1, pageSize: 1 },
      { executable }
    );

    expect(result.status, JSON.stringify(result)).toBeUndefined();
    if (result.status !== undefined) return;
    expect(result.mode).toBe('preview');
    expect(result.executable).toMatchObject({
      path: 'native',
      version: 'embedded',
    });
    expect(result.totalMatches).toBe(2);
    expect(result.affectedFiles).toBe(1);
    expect(result.matches).toHaveLength(1);
    expect(result.matches[0]?.id).toMatch(/^[a-f0-9]{64}$/);
    expect(result.files[0]).toMatchObject({
      path: 'source.ts',
      beforeHash: digest(source),
      afterHash: digest(source.replaceAll('oldCall', 'newCall')),
    });
    expect(result.files[0]?.patch).toContain('-const first = oldCall(1);');
    expect(result.files[0]?.patch).toContain('+const first = newCall(1);');
    expect(result.next?.nextPage.query).toMatchObject({
      page: 2,
      pageSize: 1,
      snapshot: result.snapshot,
    });
    expect(readFileSync(join(root, 'source.ts'), 'utf8')).toBe(source);
  });

  it('executes a continuation losslessly and rejects source drift between pages', async () => {
    const { root, source } = fixture();
    const first = source.indexOf('oldCall(1)');
    const second = source.indexOf('oldCall(2)');
    const executable = mockExecutable(root, [
      match('source.ts', first, first + 10, 'oldCall(1)', 'newCall(1)'),
      match('source.ts', second, second + 10, 'oldCall(2)', 'newCall(2)'),
    ]);
    const firstPage = await runAstRewrite(
      { ...query(root), page: 1, pageSize: 1 },
      { executable }
    );
    expect(firstPage.status).toBeUndefined();
    if (firstPage.status !== undefined || !firstPage.next) return;

    const secondPage = await runAstRewrite(firstPage.next.nextPage.query, {
      executable,
    });
    expect(secondPage.status).toBeUndefined();
    if (secondPage.status !== undefined) return;
    expect(
      new Set(
        [...firstPage.matches, ...secondPage.matches].map(item => item.id)
      ).size
    ).toBe(2);

    writeFileSync(join(root, 'source.ts'), `${source}// drift\n`);
    const drifted = await runAstRewrite(firstPage.next.nextPage.query, {
      executable,
    });
    expect(drifted).toMatchObject({
      status: 'error',
      errorCode: 'ast.rewrite.snapshot_changed',
      next: { restart: { query: { page: 1 } } },
    });
  });

  it('keeps apply disabled unless the caller grants the separate capability', async () => {
    const { root, source } = fixture();
    const start = source.indexOf('oldCall(1)');
    const executable = mockExecutable(root, [
      match('source.ts', start, start + 10, 'oldCall(1)', 'newCall(1)'),
    ]);

    const result = await runAstRewrite(
      {
        ...query(root),
        apply: true,
        expectedHashes: { [join(root, 'source.ts')]: digest(source) },
      },
      { executable }
    );

    expect(result).toMatchObject({
      status: 'error',
      errorCode: 'ast.rewrite.apply_disabled',
    });
    expect(readFileSync(join(root, 'source.ts'), 'utf8')).toBe(source);
  });

  it('requires a snapshot token when apply is granted', async () => {
    const { root, source } = fixture();
    const result = await runAstRewrite(
      {
        ...query(root),
        apply: true,
        expectedHashes: { [join(root, 'source.ts')]: digest(source) },
        // no snapshot provided
      },
      { allowApply: true }
    );
    expect(result).toMatchObject({
      status: 'error',
      errorCode: 'ast.rewrite.snapshot_required',
    });
  });

  it('requires every preview hash before apply and leaves all files unchanged', async () => {
    const { root, source } = fixture();
    writeFileSync(join(root, 'other.ts'), 'oldCall(3);\n');
    const start = source.indexOf('oldCall(1)');
    const executable = mockExecutable(root, [
      match('source.ts', start, start + 10, 'oldCall(1)', 'newCall(1)'),
      match('other.ts', 0, 10, 'oldCall(3)', 'newCall(3)'),
    ]);
    const preview = await previewForApply(root);

    const result = await runAstRewrite(
      {
        ...query(root),
        apply: true,
        snapshot: preview.snapshot,
        expectedHashes: { [join(root, 'source.ts')]: digest(source) },
      },
      { executable, allowApply: true }
    );

    expect(result).toMatchObject({
      status: 'error',
      errorCode: 'ast.rewrite.expected_hash_set_mismatch',
    });
    expect(readFileSync(join(root, 'source.ts'), 'utf8')).toBe(source);
    expect(readFileSync(join(root, 'other.ts'), 'utf8')).toBe('oldCall(3);\n');
  });

  it('stages all validated files and returns one complete apply receipt', async () => {
    const { root, source } = fixture();
    const other = 'oldCall(3);\n';
    writeFileSync(join(root, 'other.ts'), other);
    const start = source.indexOf('oldCall(1)');
    const executable = mockExecutable(root, [
      match('source.ts', start, start + 10, 'oldCall(1)', 'newCall(1)'),
      match('other.ts', 0, 10, 'oldCall(3)', 'newCall(3)'),
    ]);
    const preview = await previewForApply(root, { pageSize: 1 });

    const result = await runAstRewrite(
      {
        ...query(root),
        apply: true,
        pageSize: 1,
        ...preview,
      },
      { executable, allowApply: true }
    );

    expect(result.status).toBeUndefined();
    if (result.status !== undefined) return;
    expect(result.mode).toBe('apply');
    expect(result.transaction).toMatchObject({ committed: true, files: 2 });
    expect(result.pagination).toEqual({
      currentPage: 1,
      totalPages: 1,
      pageSize: 3, // native finds all 3 matches
      hasMore: false,
    });
    expect(result.matches).toHaveLength(3);
    expect(readFileSync(join(root, 'source.ts'), 'utf8')).toContain(
      'newCall(1)'
    );
    expect(readFileSync(join(root, 'other.ts'), 'utf8')).toBe('newCall(3);\n');
  });

  it('rejects changed hashes before staging any write', async () => {
    const { root, source } = fixture();
    const start = source.indexOf('oldCall(1)');
    const executable = mockExecutable(root, [
      match('source.ts', start, start + 10, 'oldCall(1)', 'newCall(1)'),
    ]);
    const preview = await previewForApply(root);

    const result = await runAstRewrite(
      {
        ...query(root),
        apply: true,
        snapshot: preview.snapshot,
        expectedHashes: { [join(root, 'source.ts')]: '0'.repeat(64) },
      },
      { executable, allowApply: true }
    );

    expect(result).toMatchObject({
      status: 'error',
      errorCode: 'ast.rewrite.hash_mismatch',
    });
    expect(readFileSync(join(root, 'source.ts'), 'utf8')).toBe(source);
  });

  it('does not follow symlinks outside the root boundary', async () => {
    // Native engine uses WalkBuilder without follow_links, so symlinked
    // directories are never traversed. Files outside root remain untouched.
    const root = mkdtempSync(join(tmpdir(), 'octocode-ast-rewrite-symtest-'));
    roots.push(root);
    writeFileSync(join(root, 'noop.ts'), '// no matches\n');

    const outside = mkdtempSync(
      join(tmpdir(), 'octocode-ast-rewrite-outside-')
    );
    roots.push(outside);
    writeFileSync(join(outside, 'escape.ts'), 'oldCall(1);\n');
    symlinkSync(outside, join(root, 'linked'));

    const result = await runAstRewrite(query(root), {});

    // Symlinks are not followed → no matches found → empty
    expect(result).toMatchObject({ status: 'empty' });
    // Outside file is never touched
    expect(readFileSync(join(outside, 'escape.ts'), 'utf8')).toBe(
      'oldCall(1);\n'
    );
  });

  it('rejects overlapping ast-grep edits', async () => {
    // The native engine (ast_grep find_all) never produces overlapping matches
    // for a single rule, but the overlap guard must remain correct. We inject
    // synthetic overlapping results via the native loader mock.
    const { root, source } = fixture();
    const filePath = join(root, 'source.ts');
    const start = source.indexOf('oldCall(1)');

    withRewriteMock({
      structuralRewriteFiles: vi.fn().mockResolvedValue(
        JSON.stringify([
          {
            path: filePath,
            matches: [
              {
                byteStart: start,
                byteEnd: start + 10,
                range: { start: { line: 0, column: start }, end: { line: 0, column: start + 10 } },
                text: 'oldCall(1)',
                replacedText: 'newCall(1)',
                replacement: 'newCall($A)',
                captures: { A: { kind: 'single', texts: ['1'] } },
              },
              {
                byteStart: start + 3,
                byteEnd: start + 8,
                range: { start: { line: 0, column: start + 3 }, end: { line: 0, column: start + 8 } },
                text: 'Call(',
                replacedText: 'invoke',
                replacement: 'invoke',
                captures: {},
              },
            ],
          },
        ])
      ),
    });

    const result = await runAstRewrite(query(root), {});

    expect(result).toMatchObject({
      status: 'error',
      errorCode: 'ast.rewrite.overlap',
    });
    expect(readFileSync(join(root, 'source.ts'), 'utf8')).toBe(source);
  });

  it.skip('rejects an unavailable or incompatible executable before scanning', () => {
    // These error codes (executable_unavailable, version_incompatible) were
    // emitted when the tool resolved an external ast-grep binary. The native
    // engine is always available at a fixed embedded version; this check no
    // longer applies.
  });

  it('rolls back promoted files when a later promotion fails', async () => {
    const { root, source } = fixture();
    const other = 'oldCall(3);\n';
    writeFileSync(join(root, 'other.ts'), other);
    const start = source.indexOf('oldCall(1)');
    const executable = mockExecutable(root, [
      match('source.ts', start, start + 10, 'oldCall(1)', 'newCall(1)'),
      match('other.ts', 0, 10, 'oldCall(3)', 'newCall(3)'),
    ]);
    const preview = await previewForApply(root);
    const deps: AstRewriteRuntimeDeps = {
      executable,
      allowApply: true,
      rename: async (from, to) => {
        if (basename(from).includes('.stage-') && to.endsWith('source.ts')) {
          throw new Error('injected promotion failure');
        }
        await rename(from, to);
      },
    };

    const result = await runAstRewrite(
      {
        ...query(root),
        apply: true,
        ...preview,
      },
      deps
    );

    expect(result).toMatchObject({
      status: 'error',
      errorCode: 'ast.rewrite.transaction_failed',
    });
    expect(readFileSync(join(root, 'source.ts'), 'utf8')).toBe(source);
    expect(readFileSync(join(root, 'other.ts'), 'utf8')).toBe(other);
  });

  it('reports incomplete recovery without claiming a concurrent edit was rolled back', async () => {
    const { root, source } = fixture();
    const sourcePath = realpathSync(join(root, 'source.ts'));
    const start = source.indexOf('oldCall(1)');
    const executable = mockExecutable(root, [
      match('source.ts', start, start + 10, 'oldCall(1)', 'newCall(1)'),
    ]);
    const preview = await previewForApply(root);
    const externalEdit = 'editorCall(99);\n';
    const result = await runAstRewrite(
      { ...query(root), apply: true, ...preview },
      {
        executable,
        allowApply: true,
        rename: async (from, to) => {
          await rename(from, to);
          if (basename(from).includes('.stage-') && to === sourcePath) {
            writeFileSync(sourcePath, externalEdit);
            throw new Error('promotion interrupted by concurrent edit');
          }
        },
      }
    );

    try {
      expect(result).toMatchObject({
        status: 'error',
        errorCode: 'ast.rewrite.transaction_failed',
        details: { rollback: { restored: false } },
      });
      if (result.status !== 'error')
        throw new Error('Expected transaction failure');
      expect(result.error).toContain('recovery is incomplete');
      expect(result.error).not.toContain('were rolled back');
      expect(readFileSync(sourcePath, 'utf8')).toBe(externalEdit);
    } finally {
      // Resolve this test's deliberate conflict before removing its fixture.
      writeFileSync(sourcePath, source);
      expect(await recoverTransactions(root)).toMatchObject({ ok: true });
    }
  });

  it('binds apply to the exact preview query including page size', async () => {
    const { root, source } = fixture();
    const start = source.indexOf('oldCall(1)');
    const executable = mockExecutable(root, [
      match('source.ts', start, start + 10, 'oldCall(1)', 'newCall(1)'),
    ]);
    const preview = await previewForApply(root, { pageSize: 7 });

    for (const changed of [
      { rewrite: 'otherCall($A)', pageSize: 7 },
      { rewrite: 'newCall($A)', pageSize: 8 },
    ]) {
      const result = await runAstRewrite(
        { ...query(root), ...changed, apply: true, ...preview },
        { executable, allowApply: true }
      );
      expect(result).toMatchObject({
        status: 'error',
        errorCode: 'ast.rewrite.snapshot_changed',
      });
    }
    expect(readFileSync(join(root, 'source.ts'), 'utf8')).toBe(source);
  });

  it('rejects a disappeared preview file and an extra expected hash', async () => {
    const { root, source } = fixture();
    const other = 'oldCall(3);\n';
    writeFileSync(join(root, 'other.ts'), other);
    // Native engine finds source.ts (2 matches) + other.ts (1 match) in preview.
    const preview = await previewForApply(root);

    // Scenario 1 — extra key: add a file within root that isn't a match target.
    // Native finds the same 2 files; expectedHashes has 3 keys → mismatch.
    const extraFile = join(root, 'extra-marker.ts');
    writeFileSync(extraFile, '// not a match\n');
    const extra = await runAstRewrite(
      {
        ...query(root),
        apply: true,
        ...preview,
        expectedHashes: {
          ...preview.expectedHashes,
          [extraFile]: digest(readFileSync(extraFile)),
        },
      },
      { allowApply: true }
    );
    expect(extra).toMatchObject({
      status: 'error',
      errorCode: 'ast.rewrite.expected_hash_set_mismatch',
    });

    // Scenario 2 — disappeared: delete other.ts so native only finds source.ts.
    // The snapshot from preview included other.ts → mismatch.
    rmSync(join(root, 'other.ts'));
    const disappeared = await runAstRewrite(
      { ...query(root), apply: true, ...preview },
      { allowApply: true }
    );
    expect(disappeared).toMatchObject({
      status: 'error',
      errorCode: 'ast.rewrite.snapshot_changed',
    });

    expect(readFileSync(join(root, 'source.ts'), 'utf8')).toBe(source);
  });

  it('reports snapshot drift when a continuation changes to zero matches', async () => {
    const { root } = fixture();
    // Page 1: native finds oldCall(1) and oldCall(2); pageSize=1 returns one match.
    const page = await runAstRewrite({ ...query(root), pageSize: 1 }, {});
    expect(page.status).toBeUndefined();
    if (page.status !== undefined || !page.next) return;

    // Overwrite the file so all patterns disappear before the continuation.
    writeFileSync(join(root, 'source.ts'), '// no matches left\n');

    const result = await runAstRewrite(page.next.nextPage.query, {});
    expect(result).toMatchObject({
      status: 'error',
      errorCode: 'ast.rewrite.snapshot_changed',
    });
  });

  it('revalidates renamed backup bytes before promotion', async () => {
    const { root, source } = fixture();
    const sourcePath = realpathSync(join(root, 'source.ts'));
    const start = source.indexOf('oldCall(1)');
    const executable = mockExecutable(root, [
      match('source.ts', start, start + 10, 'oldCall(1)', 'newCall(1)'),
    ]);
    const preview = await previewForApply(root);
    const externalEdit = `${source}// editor change\n`;

    const result = await runAstRewrite(
      { ...query(root), apply: true, ...preview },
      {
        executable,
        allowApply: true,
        rename: async (from, to) => {
          if (from === sourcePath && basename(to).includes('.backup-')) {
            writeFileSync(sourcePath, externalEdit);
          }
          await rename(from, to);
        },
      }
    );

    expect(result).toMatchObject({
      status: 'error',
      errorCode: 'ast.rewrite.transaction_failed',
    });
    expect(readFileSync(sourcePath, 'utf8')).toBe(externalEdit);
  });

  it('serializes concurrent apply attempts before scanning and committing', async () => {
    const { root, source } = fixture();
    const sourcePath = realpathSync(join(root, 'source.ts'));
    const start = source.indexOf('oldCall(1)');
    const executable = mockExecutable(root, [
      match('source.ts', start, start + 10, 'oldCall(1)', 'newCall(1)'),
    ]);
    const preview = await previewForApply(root);
    let release!: () => void;
    let entered!: () => void;
    const gate = new Promise<void>(resolve => {
      release = resolve;
    });
    const firstEnteredCommit = new Promise<void>(resolve => {
      entered = resolve;
    });
    let paused = false;

    const first = runAstRewrite(
      { ...query(root), apply: true, ...preview },
      {
        executable,
        allowApply: true,
        rename: async (from, to) => {
          if (
            !paused &&
            from === sourcePath &&
            basename(to).includes('.backup-')
          ) {
            paused = true;
            entered();
            await gate;
          }
          await rename(from, to);
        },
      }
    );
    await firstEnteredCommit;
    const second = runAstRewrite(
      { ...query(root), apply: true, ...preview },
      { executable, allowApply: true }
    );
    release();
    const [firstResult, secondResult] = await Promise.all([first, second]);

    expect(firstResult.status, JSON.stringify(firstResult)).toBeUndefined();
    expect(secondResult.status).toBe('error');
    // Native engine replaces ALL matches in the file in one atomic apply;
    // both oldCall(1) and oldCall(2) are rewritten by the first apply.
    expect(readFileSync(sourcePath, 'utf8')).toBe(
      source.replaceAll('oldCall', 'newCall')
    );
  });

  it('refuses a patch that exceeds the configured response bound', async () => {
    const { root, source } = fixture();
    const start = source.indexOf('oldCall(1)');
    const executable = mockExecutable(root, [
      match('source.ts', start, start + 10, 'oldCall(1)', 'x'.repeat(200)),
    ]);

    const result = await runAstRewrite(query(root), {
      executable,
      maxPatchBytes: 64,
    });

    expect(result).toMatchObject({
      status: 'error',
      errorCode: 'ast.rewrite.patch_limit',
      terminalLimit: true,
    });
    expect(readFileSync(join(root, 'source.ts'), 'utf8')).toBe(source);
  });

  it('applies include and exclude glob filters without spawning a shell', async () => {
    // The native engine applies include/exclude globs inside the Rust walker;
    // there is no shell involved, so injection attempts are inert.
    const root = mkdtempSync(join(tmpdir(), 'octocode-ast-rewrite-globs-'));
    roots.push(root);
    mkdirSync(join(root, 'src'));
    mkdirSync(join(root, 'lib'));
    writeFileSync(join(root, 'src', 'match.ts'), 'oldCall(1);\n');
    writeFileSync(join(root, 'lib', 'skip.ts'), 'oldCall(2);\n');
    writeFileSync(join(root, 'src', 'also.test.ts'), 'oldCall(3);\n');

    const result = await runAstRewrite(
      {
        ...query(root),
        include: ['src/**/*.ts', 'literal;touch SHOULD_NOT_EXIST'],
        exclude: ['**/*.test.ts'],
      },
      {}
    );

    // Only src/match.ts satisfies the include and is not excluded.
    expect(result.status, JSON.stringify(result)).toBeUndefined();
    if (result.status !== undefined) return;
    expect(result.affectedFiles).toBe(1);
    expect(result.files[0]).toMatchObject({ path: 'src/match.ts' });
    // Shell-injection attempt in the include glob had no effect.
    expect(() => readFileSync(join(root, 'SHOULD_NOT_EXIST'))).toThrow();
  });
});
