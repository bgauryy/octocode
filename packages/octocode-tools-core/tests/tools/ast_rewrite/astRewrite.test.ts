import { afterEach, describe, expect, it } from 'vitest';
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
import {
  runAstRewrite,
  type AstRewriteRuntimeDeps,
} from '../../../src/tools/ast_rewrite/index.js';

const roots: string[] = [];

afterEach(() => {
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
    path: root,
    langType: 'ts',
    pattern: 'oldCall($A)',
    rewrite: 'newCall($A)',
  };
}

async function previewForApply(
  root: string,
  executable: string,
  overrides: Record<string, unknown> = {}
) {
  const preview = await runAstRewrite(
    { ...query(root), ...overrides },
    { executable }
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

    expect(result.status).toBeUndefined();
    if (result.status !== undefined) return;
    expect(result.mode).toBe('preview');
    expect(result.executable).toEqual({
      path: realpathSync(executable),
      version: '0.40.1',
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

  it('requires every preview hash before apply and leaves all files unchanged', async () => {
    const { root, source } = fixture();
    writeFileSync(join(root, 'other.ts'), 'oldCall(3);\n');
    const start = source.indexOf('oldCall(1)');
    const executable = mockExecutable(root, [
      match('source.ts', start, start + 10, 'oldCall(1)', 'newCall(1)'),
      match('other.ts', 0, 10, 'oldCall(3)', 'newCall(3)'),
    ]);
    const preview = await previewForApply(root, executable);

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

  it('atomically stages and applies all validated files', async () => {
    const { root, source } = fixture();
    const other = 'oldCall(3);\n';
    writeFileSync(join(root, 'other.ts'), other);
    const start = source.indexOf('oldCall(1)');
    const executable = mockExecutable(root, [
      match('source.ts', start, start + 10, 'oldCall(1)', 'newCall(1)'),
      match('other.ts', 0, 10, 'oldCall(3)', 'newCall(3)'),
    ]);
    const preview = await previewForApply(root, executable);

    const result = await runAstRewrite(
      {
        ...query(root),
        apply: true,
        ...preview,
      },
      { executable, allowApply: true }
    );

    expect(result.status).toBeUndefined();
    if (result.status !== undefined) return;
    expect(result.mode).toBe('apply');
    expect(result.transaction).toMatchObject({ committed: true, files: 2 });
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
    const preview = await previewForApply(root, executable);

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

  it('rejects output paths that escape the real root through a symlink', async () => {
    const { root } = fixture();
    const outside = mkdtempSync(
      join(tmpdir(), 'octocode-ast-rewrite-outside-')
    );
    roots.push(outside);
    writeFileSync(join(outside, 'escape.ts'), 'oldCall(1);\n');
    symlinkSync(outside, join(root, 'linked'));
    const executable = mockExecutable(root, [
      match('linked/escape.ts', 0, 10, 'oldCall(1)', 'newCall(1)'),
    ]);

    const result = await runAstRewrite(query(root), { executable });

    expect(result).toMatchObject({
      status: 'error',
      errorCode: 'ast.rewrite.path_escape',
    });
    expect(readFileSync(join(outside, 'escape.ts'), 'utf8')).toBe(
      'oldCall(1);\n'
    );
  });

  it('rejects overlapping ast-grep edits', async () => {
    const { root, source } = fixture();
    const start = source.indexOf('oldCall(1)');
    const executable = mockExecutable(root, [
      match('source.ts', start, start + 10, 'oldCall(1)', 'newCall(1)'),
      match('source.ts', start + 3, start + 8, 'Call(', 'invoke'),
    ]);

    const result = await runAstRewrite(query(root), { executable });

    expect(result).toMatchObject({
      status: 'error',
      errorCode: 'ast.rewrite.overlap',
    });
    expect(readFileSync(join(root, 'source.ts'), 'utf8')).toBe(source);
  });

  it('rejects an unavailable or incompatible executable before scanning', async () => {
    const { root } = fixture();
    const incompatible = mockExecutable(root, [], '1.0.0');

    const unavailable = await runAstRewrite(query(root), {
      executable: join(root, 'missing'),
    });
    const wrongVersion = await runAstRewrite(query(root), {
      executable: incompatible,
    });

    expect(unavailable).toMatchObject({
      status: 'error',
      errorCode: 'ast.rewrite.executable_unavailable',
    });
    expect(wrongVersion).toMatchObject({
      status: 'error',
      errorCode: 'ast.rewrite.version_incompatible',
    });
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
    const preview = await previewForApply(root, executable);
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

  it('binds apply to the exact preview query including page size', async () => {
    const { root, source } = fixture();
    const start = source.indexOf('oldCall(1)');
    const executable = mockExecutable(root, [
      match('source.ts', start, start + 10, 'oldCall(1)', 'newCall(1)'),
    ]);
    const preview = await previewForApply(root, executable, { pageSize: 7 });

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
    const start = source.indexOf('oldCall(1)');
    const executable = mockExecutable(root, [
      match('source.ts', start, start + 10, 'oldCall(1)', 'newCall(1)'),
      match('other.ts', 0, 10, 'oldCall(3)', 'newCall(3)'),
    ]);
    const preview = await previewForApply(root, executable);

    mockExecutable(root, [
      match('source.ts', start, start + 10, 'oldCall(1)', 'newCall(1)'),
    ]);
    const disappeared = await runAstRewrite(
      { ...query(root), apply: true, ...preview },
      { executable, allowApply: true }
    );
    expect(disappeared).toMatchObject({
      status: 'error',
      errorCode: 'ast.rewrite.snapshot_changed',
    });

    mockExecutable(root, [
      match('source.ts', start, start + 10, 'oldCall(1)', 'newCall(1)'),
      match('other.ts', 0, 10, 'oldCall(3)', 'newCall(3)'),
    ]);
    const extra = await runAstRewrite(
      {
        ...query(root),
        apply: true,
        ...preview,
        expectedHashes: {
          ...preview.expectedHashes,
          [executable]: digest(readFileSync(executable)),
        },
      },
      { executable, allowApply: true }
    );
    expect(extra).toMatchObject({
      status: 'error',
      errorCode: 'ast.rewrite.expected_hash_set_mismatch',
    });
  });

  it('reports snapshot drift when a continuation changes to zero matches', async () => {
    const { root, source } = fixture();
    const first = source.indexOf('oldCall(1)');
    const second = source.indexOf('oldCall(2)');
    const executable = mockExecutable(root, [
      match('source.ts', first, first + 10, 'oldCall(1)', 'newCall(1)'),
      match('source.ts', second, second + 10, 'oldCall(2)', 'newCall(2)'),
    ]);
    const page = await runAstRewrite(
      { ...query(root), pageSize: 1 },
      { executable }
    );
    expect(page.status).toBeUndefined();
    if (page.status !== undefined || !page.next) return;
    mockExecutable(root, []);

    const result = await runAstRewrite(page.next.nextPage.query, {
      executable,
    });
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
    const preview = await previewForApply(root, executable);
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
    const preview = await previewForApply(root, executable);
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

    expect(firstResult.status).toBeUndefined();
    expect(secondResult.status).toBe('error');
    expect(readFileSync(sourcePath, 'utf8')).toBe(
      source.replace('oldCall(1)', 'newCall(1)')
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

  it('passes include and exclude globs as argv values without a shell', async () => {
    const { root } = fixture();
    const argvLog = join(root, 'argv.json');
    const executable = join(root, 'logging-ast-grep.mjs');
    writeFileSync(
      executable,
      `#!/usr/bin/env node
import { writeFileSync } from 'node:fs';
if (process.argv[2] === '--version') process.stdout.write('ast-grep 0.40.1\\n');
else { writeFileSync(${JSON.stringify(argvLog)}, JSON.stringify(process.argv.slice(2))); process.stdout.write('[]'); }
`
    );
    chmodSync(executable, 0o755);

    const result = await runAstRewrite(
      {
        ...query(root),
        include: ['src/**/*.ts', 'literal;touch SHOULD_NOT_EXIST'],
        exclude: ['**/*.test.ts'],
      },
      { executable }
    );

    expect(result.status).toBe('empty');
    expect(JSON.parse(readFileSync(argvLog, 'utf8'))).toEqual(
      expect.arrayContaining([
        '--globs',
        'src/**/*.ts',
        'literal;touch SHOULD_NOT_EXIST',
        '!**/*.test.ts',
      ])
    );
    expect(() => readFileSync(join(root, 'SHOULD_NOT_EXIST'))).toThrow();
  });
});
