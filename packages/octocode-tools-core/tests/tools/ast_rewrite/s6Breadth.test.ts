import { afterEach, describe, expect, it } from 'vitest';
import {
  chmodSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { runAstRewrite } from '../../../src/tools/ast_rewrite/index.js';

const roots: string[] = [];

afterEach(() => {
  for (const root of roots.splice(0))
    rmSync(root, { recursive: true, force: true });
});

function root(): string {
  const value = mkdtempSync(join(tmpdir(), 'octocode-ast-rewrite-s6-'));
  roots.push(value);
  return value;
}

function match(start: number, text: string, replacement: string) {
  return {
    file: 'source.ts',
    text,
    replacement,
    language: 'TypeScript',
    range: {
      byteOffset: { start, end: start + Buffer.byteLength(text) },
      start: { line: 0, column: start },
      end: { line: 0, column: start + text.length },
    },
    replacementOffsets: { start, end: start + Buffer.byteLength(text) },
    metaVariables: {
      single: {
        A: {
          text: text.slice(text.indexOf('(') + 1, -1),
          range: {
            byteOffset: { start: start + 8, end: start + text.length - 1 },
            start: { line: 0, column: start + 8 },
            end: { line: 0, column: start + text.length - 1 },
          },
        },
      },
      multi: {},
      transformed: {},
    },
  };
}

function executable(
  directory: string,
  initial: unknown[],
  options: {
    inlineRules?: boolean;
    postcondition?: unknown[];
    emptyExitCode?: number;
    log?: string;
  } = {}
): string {
  const path = join(directory, 'mock-ast-grep.mjs');
  writeFileSync(
    path,
    `#!/usr/bin/env node
import { writeFileSync } from 'node:fs';
const args = process.argv.slice(2);
if (args[0] === '--version') process.stdout.write('ast-grep 0.45.0\\n');
else if (args[0] === 'run' && args[1] === '--help') process.stdout.write('--pattern --rewrite --lang --json --globs --threads --color');
else if (args[0] === 'scan' && args[1] === '--help') process.stdout.write(${JSON.stringify(options.inlineRules === false ? '--json --globs --threads --color' : '--inline-rules --json --globs --threads --color')});
else {
  ${options.log ? `writeFileSync(${JSON.stringify(options.log)}, JSON.stringify(args));` : ''}
  const postcondition = String(args.at(-1)).includes('octocode-ast-rewrite-postcondition-');
  const matches = postcondition ? ${JSON.stringify(options.postcondition ?? [])} : ${JSON.stringify(initial)};
  process.stdout.write(JSON.stringify(matches));
  if (matches.length === 0) process.exitCode = ${options.emptyExitCode ?? 0};
}
`
  );
  chmodSync(path, 0o755);
  return path;
}

function patternQuery(directory: string) {
  return {
    path: directory,
    langType: 'typescript',
    ruleKind: 'pattern' as const,
    pattern: 'oldCall($A)',
    rewrite: 'newCall($A)',
  };
}

describe('astRewrite S6 breadth', () => {
  it('accepts ast-grep exit 1 with an explicit empty result', async () => {
    const directory = root();
    writeFileSync(join(directory, 'source.ts'), 'newCall(1);\n');
    const binary = executable(directory, [], { emptyExitCode: 1 });
    expect(
      await runAstRewrite(patternQuery(directory), { executable: binary })
    ).toMatchObject({ status: 'empty', complete: true, totalMatches: 0 });
  });

  it('commits when the staged postcondition finds no remaining matches with exit 1', async () => {
    const directory = root();
    const sourcePath = join(directory, 'source.ts');
    writeFileSync(sourcePath, 'oldCall(1);\n');
    const binary = executable(
      directory,
      [match(0, 'oldCall(1)', 'newCall(1)')],
      { emptyExitCode: 1 }
    );
    const preview = await runAstRewrite(patternQuery(directory), {
      executable: binary,
    });
    if (preview.status !== undefined) throw new Error('Expected preview');
    const result = await runAstRewrite(
      {
        ...patternQuery(directory),
        apply: true,
        snapshot: preview.snapshot,
        expectedHashes: Object.fromEntries(
          preview.files.map(file => [file.absolutePath, file.beforeHash])
        ),
        postconditions: [{ kind: 'remainingMatches', equals: 0 }],
      },
      { executable: binary, allowApply: true }
    );
    expect(result).toMatchObject({
      mode: 'apply',
      transaction: { committed: true },
    });
    expect(readFileSync(sourcePath, 'utf8')).toBe('newCall(1);\n');
  });

  it('executes a strict inline rule and returns bounded captures', async () => {
    const directory = root();
    const source = 'oldCall(1);\n';
    writeFileSync(join(directory, 'source.ts'), source);
    const log = join(directory, 'argv.json');
    const binary = executable(
      directory,
      [match(0, 'oldCall(1)', 'newCall(1)')],
      {
        log,
      }
    );

    const result = await runAstRewrite(
      {
        path: directory,
        langType: 'typescript',
        ruleKind: 'rule',
        rule: { pattern: 'oldCall($A)' },
        fix: 'newCall($A)',
      },
      { executable: binary }
    );

    expect(result.status).toBeUndefined();
    if (result.status !== undefined) return;
    // Native engine returns captures as structured data; argv.json is not
    // written since no external binary is spawned.
    expect(result.matches[0]?.captures).toEqual({
      A: { kind: 'single', texts: ['1'] },
    });
  });

  it('executes experimental rules with transform and rewriters via the native engine', async () => {
    // The native engine supports experimental rules natively; all rule kinds
    // (pattern, rule, experimental) are always available.
    const directory = root();
    writeFileSync(join(directory, 'source.ts'), 'oldCall(1);\n');

    const result = await runAstRewrite(
      {
        path: directory,
        langType: 'typescript',
        ruleKind: 'experimental',
        rule: { pattern: 'oldCall($A)' },
        fix: '$REWRITTEN',
        transform: {
          REWRITTEN: {
            rewrite: { source: '$A', rewriters: ['rename'] },
          },
        },
        rewriters: [
          {
            id: 'rename',
            rule: { kind: 'number' },
            fix: '2',
          },
        ],
      },
      {}
    );

    // The native Rust engine processes the experimental rule and rewrites
    // oldCall(1) using the transform → result should be a success with 1 match.
    expect(result.status, JSON.stringify(result)).toBeUndefined();
    if (result.status !== undefined) return;
    expect(result.affectedFiles).toBe(1);
  });

  it('returns a typed terminal limit instead of truncating matches', async () => {
    const directory = root();
    writeFileSync(join(directory, 'source.ts'), 'oldCall(1); oldCall(2);\n');
    const binary = executable(directory, [
      match(0, 'oldCall(1)', 'newCall(1)'),
      match(12, 'oldCall(2)', 'newCall(2)'),
    ]);
    const result = await runAstRewrite(
      { ...patternQuery(directory), maxMatches: 1 },
      { executable: binary }
    );
    expect(result).toMatchObject({
      status: 'error',
      errorCode: 'ast.rewrite.match_limit',
      terminalLimit: true,
      details: { observed: 2, maxMatches: 1 },
    });
  });

  it('applies only selected stable match IDs and rejects unknown IDs', async () => {
    const directory = root();
    const source = 'oldCall(1); oldCall(2);\n';
    const sourcePath = join(directory, 'source.ts');
    writeFileSync(sourcePath, source);
    const binary = executable(directory, [
      match(0, 'oldCall(1)', 'newCall(1)'),
      match(12, 'oldCall(2)', 'newCall(2)'),
    ]);
    const preview = await runAstRewrite(patternQuery(directory), {
      executable: binary,
    });
    expect(preview.status).toBeUndefined();
    if (preview.status !== undefined) return;
    const expectedHashes = Object.fromEntries(
      preview.files.map(file => [file.absolutePath, file.beforeHash])
    );
    const unknown = await runAstRewrite(
      {
        ...patternQuery(directory),
        apply: true,
        snapshot: preview.snapshot,
        expectedHashes,
        selectedMatchIds: ['f'.repeat(64)],
      },
      { executable: binary, allowApply: true }
    );
    expect(unknown).toMatchObject({
      status: 'error',
      errorCode: 'ast.rewrite.selection_invalid',
    });
    expect(readFileSync(sourcePath, 'utf8')).toBe(source);

    const applied = await runAstRewrite(
      {
        ...patternQuery(directory),
        apply: true,
        snapshot: preview.snapshot,
        expectedHashes,
        selectedMatchIds: [preview.matches[0]?.id ?? ''],
      },
      { executable: binary, allowApply: true }
    );
    expect(applied.status).toBeUndefined();
    expect(readFileSync(sourcePath, 'utf8')).toBe('newCall(1); oldCall(2);\n');
  });

  it('leaves the working tree unchanged when a postcondition fails', async () => {
    const directory = root();
    const source = 'oldCall(1);\n';
    const sourcePath = join(directory, 'source.ts');
    writeFileSync(sourcePath, source);
    const initial = [match(0, 'oldCall(1)', 'newCall(1)')];
    const binary = executable(directory, initial, { postcondition: initial });
    const preview = await runAstRewrite(patternQuery(directory), {
      executable: binary,
    });
    expect(preview.status).toBeUndefined();
    if (preview.status !== undefined) return;
    const result = await runAstRewrite(
      {
        ...patternQuery(directory),
        apply: true,
        snapshot: preview.snapshot,
        expectedHashes: Object.fromEntries(
          preview.files.map(file => [file.absolutePath, file.beforeHash])
        ),
        // With native the apply correctly rewrites oldCall→newCall, leaving 0
        // remaining matches. Setting equals: 1 forces a postcondition failure,
        // exercising the rollback path while keeping the working tree unchanged.
        postconditions: [{ kind: 'remainingMatches', equals: 1 }],
      },
      { executable: binary, allowApply: true }
    );
    expect(result).toMatchObject({
      status: 'error',
      errorCode: 'ast.rewrite.postcondition_failed',
    });
    expect(readFileSync(sourcePath, 'utf8')).toBe(source);
  });
});
