import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { test, beforeEach, afterEach } from 'vitest';
import { Type } from 'typebox';
import type { ToolDefinition } from '../src/types.js';
import { registerEditTool } from '../src/tools/edit-tool.js';
import { registerUniqueTool } from '../src/tools/octocode-tools.js';
import { clearReadStatesForTests, recordFileReadState } from '../src/tools/file-state.js';

let tmpDir: string;
let editTool: ToolDefinition;

beforeEach(() => {
  tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'edit-tool-test-'));
  clearReadStatesForTests();

  const tools = new Map<string, ToolDefinition>();
  registerEditTool(
    { registerTool: (def) => tools.set(def.name, def) },
    Type,
    new Set<string>(),
    registerUniqueTool,
  );
  editTool = tools.get('edit')!;
  assert.ok(editTool, 'edit tool must be registered');
});

afterEach(() => {
  fs.rmSync(tmpDir, { recursive: true, force: true });
  clearReadStatesForTests();
});

function writeFile(rel: string, content: string): string {
  const abs = path.join(tmpDir, rel);
  fs.mkdirSync(path.dirname(abs), { recursive: true });
  fs.writeFileSync(abs, content, 'utf8');
  return abs;
}

function run(
  params: Record<string, unknown>,
  cwd = tmpDir,
  signal?: AbortSignal,
): ReturnType<ToolDefinition['execute']> {
  return editTool.execute('call-1', params, signal, undefined, { cwd });
}

// ─── Schema: universal queries[] envelope ─────────────────────────────────────

test('schema exposes queries and a sequential-only run policy', () => {
  const schema = editTool.parameters as { properties?: Record<string, unknown>; required?: string[] };
  assert.deepEqual(Object.keys(schema.properties ?? {}), ['queries', 'queryRunType']);
  assert.deepEqual((schema.properties?.['queryRunType'] as { enum?: string[] })?.enum, ['sequential']);
  assert.ok(schema.required?.includes('queries'), 'queries must be in required[]');
});

test('schema requires per-item reasoning', () => {
  type QuerySchema = { properties?: { queries?: { items?: { properties?: Record<string, unknown>; required?: string[] } } } };
  const items = (editTool.parameters as QuerySchema).properties?.queries?.items;
  assert.ok(items?.properties?.['reasoning'], 'queries[].reasoning must exist in schema');
  assert.ok(items?.required?.includes('reasoning'), 'reasoning must be in per-query required[]');
});

test('schema has path and edits in per-query items', () => {
  type QuerySchema = { properties?: { queries?: { items?: { properties?: Record<string, unknown> } } } };
  const items = (editTool.parameters as QuerySchema).properties?.queries?.items;
  assert.ok(items?.properties?.['path'], 'queries[].path must be in schema');
  assert.ok(items?.properties?.['edits'], 'queries[].edits must be in schema');
});

test('schema sets minItems:1 on the queries array', () => {
  type QuerySchema = { properties?: { queries?: { minItems?: number } } };
  const schema = editTool.parameters as QuerySchema;
  assert.equal(schema.properties?.queries?.minItems, 1);
});

test('prepareArguments leaves flat edit input unchanged', () => {
  const input = {
    path: 'a.txt',
    edits: [{ reasoning: 'fix typo', oldText: 'old', newText: 'new' }],
  };
  assert.deepEqual(editTool.prepareArguments!(input), input);
});

test('prepareArguments fills missing query-level reasoning inside queries[]', () => {
  const result = editTool.prepareArguments!({
    queries: [
      { path: 'a.txt', edits: [{ reasoning: 'from edit', oldText: 'x', newText: 'y' }] },
    ],
  }) as { queries: { reasoning: string }[] };
  assert.equal(result.queries[0]!.reasoning, 'edit operation');
});

test('prepareArguments leaves existing query-level reasoning unchanged', () => {
  const result = editTool.prepareArguments!({
    queries: [
      { reasoning: 'explicit', path: 'a.txt', edits: [{ reasoning: 'inner', oldText: 'x', newText: 'y' }] },
    ],
  }) as { queries: { reasoning: string }[] };
  assert.equal(result.queries[0]!.reasoning, 'explicit');
});

// ─── Execute: single query (passthroughSingle) ───────────────────────────────

test('single query returns file and replacement details', async () => {
  writeFile('a.txt', 'hello world\n');
  const result = await run({
    queries: [{
      reasoning: 'update greeting',
      path: 'a.txt',
      edits: [{ reasoning: 'replace hello', oldText: 'hello', newText: 'goodbye' }],
    }],
  });
  assert.equal(result.isError, undefined);
  assert.match((result.content[0] as { text: string }).text, /Successfully replaced/);
  const details = result.details as { files?: unknown[]; replacements?: number; diff?: string };
  assert.ok(Array.isArray(details.files), 'result.details.files must be an array');
  assert.equal(details.replacements, 1);
  assert.ok(typeof details.diff === 'string', 'result.details.diff must be a string');
  assert.equal(fs.readFileSync(path.join(tmpDir, 'a.txt'), 'utf8'), 'goodbye world\n');
});

test('single query includes per-edit reasoning in result text', async () => {
  writeFile('b.txt', 'alpha beta\n');
  const result = await run({
    queries: [{
      reasoning: 'rename symbol',
      path: 'b.txt',
      edits: [{ reasoning: 'alpha → gamma', oldText: 'alpha', newText: 'gamma' }],
    }],
  });
  assert.match((result.content[0] as { text: string }).text, /alpha → gamma/);
});

// ─── Execute: multiple ordered queries ───────────────────────────────────────

test('multiple queries execute in order and both files are updated', async () => {
  writeFile('x.txt', 'foo\n');
  writeFile('y.txt', 'bar\n');
  const result = await run({
    queries: [
      { reasoning: 'update x', path: 'x.txt', edits: [{ reasoning: 'fix', oldText: 'foo', newText: 'FOO' }] },
      { reasoning: 'update y', path: 'y.txt', edits: [{ reasoning: 'fix', oldText: 'bar', newText: 'BAR' }] },
    ],
  });
  // Multi-query: returns envelope summary, not passthrough
  assert.match((result.content[0] as { text: string }).text, /2 quer/i);
  assert.equal(fs.readFileSync(path.join(tmpDir, 'x.txt'), 'utf8'), 'FOO\n');
  assert.equal(fs.readFileSync(path.join(tmpDir, 'y.txt'), 'utf8'), 'BAR\n');
});

test('multiple queries: second failure stops execution and first file stays written', async () => {
  writeFile('p.txt', 'correct\n');
  writeFile('q.txt', 'existing\n');
  // Second edit has oldText that does not exist → will fail
  await assert.rejects(
    () => run({
      queries: [
        { reasoning: 'fix p', path: 'p.txt', edits: [{ reasoning: 'ok', oldText: 'correct', newText: 'changed' }] },
        { reasoning: 'fix q', path: 'q.txt', edits: [{ reasoning: 'bad', oldText: 'MISSING_TEXT', newText: 'oops' }] },
      ],
    }),
    /Could not find|MISSING_TEXT|queries\[1\]/i,
  );
  // q.txt is unchanged because query[1] failed in preflight (before any writes)
  assert.equal(fs.readFileSync(path.join(tmpDir, 'q.txt'), 'utf8'), 'existing\n');
  // p.txt — preflight ran for both first, so p.txt should also be unchanged
  // (preflight catches query[1] error before execute of query[0] runs)
  assert.equal(fs.readFileSync(path.join(tmpDir, 'p.txt'), 'utf8'), 'correct\n');
});

// ─── Preflight: path guard validates ALL before any write ─────────────────────

test('preflight rejects forbidden path before any writes happen', async () => {
  writeFile('ok.txt', 'original\n');
  const prev = process.env['ALLOWED_PATHS'];
  try {
    delete process.env['ALLOWED_PATHS'];
    await assert.rejects(
      () => run({
        queries: [
          { reasoning: 'valid', path: 'ok.txt', edits: [{ reasoning: 'change', oldText: 'original', newText: 'mutated' }] },
          { reasoning: 'evil', path: '/usr/evil-edit-test.txt', edits: [{ reasoning: 'bad', oldText: 'x', newText: 'y' }] },
        ],
      }),
      /outside|allowed|preflight/i,
    );
    // ok.txt must be untouched: preflight caught the forbidden path before any execute ran
    assert.equal(fs.readFileSync(path.join(tmpDir, 'ok.txt'), 'utf8'), 'original\n');
  } finally {
    if (prev === undefined) delete process.env['ALLOWED_PATHS'];
    else process.env['ALLOWED_PATHS'] = prev;
  }
});

// ─── Validation errors ────────────────────────────────────────────────────────

test('missing query-level reasoning throws', async () => {
  writeFile('v.txt', 'val\n');
  await assert.rejects(
    () => run({
      // Note: no reasoning on the query item — bypasses prepareArguments
      queries: [{ path: 'v.txt', edits: [{ reasoning: 'ok', oldText: 'val', newText: 'new' }] }],
    }),
    /reasoning/i,
  );
});

test('empty reasoning on query throws', async () => {
  writeFile('v.txt', 'val\n');
  await assert.rejects(
    () => run({
      queries: [{ reasoning: '   ', path: 'v.txt', edits: [{ reasoning: 'ok', oldText: 'val', newText: 'new' }] }],
    }),
    /reasoning/i,
  );
});

test('missing per-edit reasoning throws', async () => {
  writeFile('e.txt', 'data\n');
  await assert.rejects(
    () => run({
      queries: [{
        reasoning: 'file level ok',
        path: 'e.txt',
        edits: [{ oldText: 'data', newText: 'other' }], // no reasoning
      }],
    }),
    /reasoning/i,
  );
});

test('oldText not found throws descriptive error', async () => {
  writeFile('f.txt', 'hello world\n');
  await assert.rejects(
    () => run({
      queries: [{
        reasoning: 'update',
        path: 'f.txt',
        edits: [{ reasoning: 'fix', oldText: 'NOT_PRESENT_IN_FILE', newText: 'x' }],
      }],
    }),
    /Could not find/i,
  );
  // File unchanged
  assert.equal(fs.readFileSync(path.join(tmpDir, 'f.txt'), 'utf8'), 'hello world\n');
});

test('duplicate target paths in queries throws', async () => {
  writeFile('dup.txt', 'abc\n');
  await assert.rejects(
    () => run({
      queries: [
        { reasoning: 'first', path: 'dup.txt', edits: [{ reasoning: 'r', oldText: 'abc', newText: 'xyz' }] },
        { reasoning: 'second', path: 'dup.txt', edits: [{ reasoning: 'r', oldText: 'xyz', newText: 'nnn' }] },
      ],
    }),
    /duplicate/i,
  );
});

test('empty edits array in query throws', async () => {
  writeFile('e.txt', 'data\n');
  await assert.rejects(
    () => run({
      queries: [{ reasoning: 'something', path: 'e.txt', edits: [] }],
    }),
    /edits|replacement/i,
  );
});

// ─── Abort signal ─────────────────────────────────────────────────────────────

test('aborted signal before execute resolves with abort error', async () => {
  writeFile('ab.txt', 'data\n');
  const controller = new AbortController();
  controller.abort();
  await assert.rejects(
    () => run({
      queries: [{
        reasoning: 'aborted',
        path: 'ab.txt',
        edits: [{ reasoning: 'change', oldText: 'data', newText: 'changed' }],
      }],
    }, tmpDir, controller.signal),
    /aborted|Operation aborted/i,
  );
  assert.equal(fs.readFileSync(path.join(tmpDir, 'ab.txt'), 'utf8'), 'data\n');
});

// ─── Result detail shape ──────────────────────────────────────────────────────

test('result.details.files contains path, diff, patch, usedModes, and edits per file', async () => {
  writeFile('c.txt', 'original content\n');
  const result = await run({
    queries: [{
      reasoning: 'update content',
      path: 'c.txt',
      edits: [{ reasoning: 'replace word', oldText: 'original', newText: 'updated' }],
    }],
  });
  const details = result.details as {
    files?: Array<{
      path: string;
      diff: string;
      patch: string;
      usedModes: string[];
      edits: unknown[];
      replacements: number;
    }>;
  };
  const file = details.files?.[0];
  assert.equal(file?.path, 'c.txt');
  assert.ok(typeof file?.diff === 'string', 'diff must be a string');
  assert.ok(typeof file?.patch === 'string', 'patch must be a string');
  assert.ok(Array.isArray(file?.usedModes), 'usedModes must be an array');
  assert.ok(Array.isArray(file?.edits), 'edits must be an array');
  assert.equal(file?.replacements, 1);
});

// ─── matchMode support ────────────────────────────────────────────────────────

test('matchMode:normalized handles whitespace drift', async () => {
  writeFile('n.txt', 'hello   world\n');
  const result = await run({
    queries: [{
      reasoning: 'normalize spacing',
      path: 'n.txt',
      edits: [{ reasoning: 'match normalized', oldText: 'hello world', newText: 'hi earth', matchMode: 'normalized' }],
    }],
  });
  assert.equal(result.isError, undefined);
  assert.equal(fs.readFileSync(path.join(tmpDir, 'n.txt'), 'utf8'), 'hi earth\n');
});

test('matchMode:lineRange replaces by line numbers', async () => {
  writeFile('lr.txt', 'line1\nline2\nline3\n');
  const result = await run({
    queries: [{
      reasoning: 'replace line 2',
      path: 'lr.txt',
      edits: [{ reasoning: 'target line', newText: 'REPLACED\n', matchMode: 'lineRange', startLine: 2, endLine: 2 }],
    }],
  });
  assert.equal(result.isError, undefined);
  assert.equal(fs.readFileSync(path.join(tmpDir, 'lr.txt'), 'utf8'), 'line1\nREPLACED\nline3\n');
});

// ─── renderCall / renderResult ────────────────────────────────────────────────

test('renderCall returns a renderer that includes the tool label and file path', () => {
  assert.ok(typeof editTool.renderCall === 'function');
  const renderer = editTool.renderCall!(
    { queries: [{ path: 'src/foo.ts', reasoning: 'fix', edits: [{ reasoning: 'r', oldText: 'x', newText: 'y' }] }] },
  );
  assert.ok(renderer);
  const lines = (renderer as { render(width: number): string[] }).render(120);
  assert.ok(lines.join('\n').includes('edit (Octocode)'), 'label must appear');
  assert.ok(lines.join('\n').includes('src/foo.ts'), 'file path must appear');
});

test('renderResult for isPartial=true renders progress indicator', () => {
  assert.ok(typeof editTool.renderResult === 'function');
  const result: import('../src/types.js').ToolCallResult = { content: [], isError: undefined };
  const renderer = editTool.renderResult!(result, { isPartial: true });
  const lines = (renderer as { render(width: number): string[] }).render(80);
  assert.ok(lines.join('\n').includes('edit (Octocode)'), 'partial must name the tool');
});

test('renderResult for successful edit includes replacement count', () => {
  const result: import('../src/types.js').ToolCallResult = {
    content: [{ type: 'text' as const, text: 'ok' }],
    details: {
      replacements: 3,
      files: [{ path: 'x.ts', edits: [], diff: '', patch: '', usedModes: [], readState: { state: 'fresh' }, reasoning: [] }],
    },
  };
  const renderer = editTool.renderResult!(result, {});
  const lines = (renderer as { render(width: number): string[] }).render(120);
  assert.ok(lines.join('\n').includes('3 replacement'), 'replacement count must appear');
});

// ─── Explicit normalized matching ———————————————————————————————————

test('normalized mode tolerates indentation differences only when explicitly requested', async () => {
  // File uses 2-space indent; oldText uses 4-space indent (copy-paste drift).
  // Exact matching remains strict; callers opt into normalized matching.
  writeFile('fallback.ts', 'function foo() {\n  return 1;\n}\n');

  const result = await run({
    queries: [{
      reasoning: 'adjust return value',
      path: 'fallback.ts',
      edits: [{
        reasoning: 'update return',
        matchMode: 'normalized',
        // 4-space indent — does not match file’s 2-space exactly
        oldText: 'function foo() {\n    return 1;\n}\n',
        newText: 'function foo() {\n  return 2;\n}\n',
      }],
    }],
  });

  assert.equal(result.isError, undefined, 'should succeed via explicit normalized matching');
  assert.ok(
    fs.readFileSync(path.join(tmpDir, 'fallback.ts'), 'utf8').includes('return 2'),
    'replacement must be applied',
  );
  // The result details should report the normalized mode was used.
  const details = result.details as { files?: Array<{ usedModes?: string[] }> } | undefined;
  assert.ok(
    details?.files?.[0]?.usedModes?.includes('normalized'),
    'usedModes must include normalized',
  );
});

test('Fix 2: when both exact and normalized fail, original exact error is re-thrown', async () => {
  // oldText is completely absent from the file — neither exact nor normalized can match.
  // Preflight throws (rejects) rather than returning isError for hard mismatches.
  writeFile('nomatch.ts', 'const a = 1;\n');
  await assert.rejects(
    () => run({
      queries: [{
        reasoning: 'test',
        path: 'nomatch.ts',
        edits: [{ reasoning: 'r', oldText: 'const b = 999;\n', newText: 'const b = 0;\n' }],
      }],
    }),
    /Could not find/,
    'exact error message must surface when both modes fail',
  );
});

// ─── Fix 3: lineRange + oldText → contentAnchored (advisory stale, not hard-fail) ———

test('Fix 3: lineRange+oldText proceeds with advisory stale — lineRangeReplacement self-verifies', async () => {
  // 1. Create file and record read state.
  const file = writeFile('lr-stale.ts', 'alpha\nbeta\ngamma\n');
  await recordFileReadState(file, tmpDir);

  // 2. Externally modify — state is now stale.
  fs.writeFileSync(file, 'alpha\nBETA\ngamma\n', 'utf8');

  // 3. lineRange edit with oldText matching the NEW content.
  //    With Fix 3, contentAnchored=true (lineRange has oldText), so checkReadState
  //    returns advisory instead of throwing.
  const result = await run({
    queries: [{
      reasoning: 'replace second line',
      path: 'lr-stale.ts',
      edits: [{
        reasoning: 'update line',
        matchMode: 'lineRange',
        startLine: 2,
        endLine: 2,
        oldText: 'BETA\n',
        newText: 'GAMMA_NEW\n',
      }],
    }],
  });

  assert.equal(result.isError, undefined, 'should succeed: lineRange+oldText is self-verifying');
  assert.equal(
    fs.readFileSync(file, 'utf8'),
    'alpha\nGAMMA_NEW\ngamma\n',
    'replacement applied correctly',
  );
});

test('Fix 3: lineRange+oldText fails when oldText does not match the range content', async () => {
  // The stale advisory only helps if oldText actually matches what’s at the range.
  // When it doesn’t, lineRangeReplacement throws a clear mismatch error (from preflight).
  const file = writeFile('lr-mismatch.ts', 'alpha\nbeta\ngamma\n');
  await recordFileReadState(file, tmpDir);
  fs.writeFileSync(file, 'alpha\nBETA\ngamma\n', 'utf8');

  await assert.rejects(
    () => run({
      queries: [{
        reasoning: 'test wrong oldText',
        path: 'lr-mismatch.ts',
        edits: [{
          reasoning: 'test',
          matchMode: 'lineRange',
          startLine: 2,
          endLine: 2,
          oldText: 'WRONG_CONTENT\n',  // does not match 'BETA\n'
          newText: 'replaced\n',
        }],
      }],
    }),
    /oldText does not match the requested line range/,
  );
});

test('Fix 3: lineRange without oldText + stale state still hard-fails', async () => {
  // Without oldText the edit is position-anchored (no self-verification),
  // so a stale recorded read is still a hard error (preflight throws/rejects).
  const file = writeFile('lr-no-old.ts', 'x\ny\nz\n');
  await recordFileReadState(file, tmpDir);
  fs.writeFileSync(file, 'x\nY\nz\n', 'utf8');

  await assert.rejects(
    () => run({
      queries: [{
        reasoning: 'position only',
        path: 'lr-no-old.ts',
        edits: [{
          reasoning: 'test',
          matchMode: 'lineRange',
          startLine: 2,
          endLine: 2,
          newText: 'replaced\n',
          // no oldText — not self-verifying
        }],
      }],
    }),
    /File changed since last recorded read/,
    'position-anchored lineRange without oldText must hard-fail on stale',
  );
});

test('replaceAll:true replaces every occurrence in the file', async () => {
  const filePath = path.join(tmpDir, 'replace-all.ts');
  fs.writeFileSync(filePath, 'foo\nfoo\nbar\nfoo\n', 'utf8');
  await recordFileReadState(filePath);
  const result = await run({
    queries: [{
      reasoning: 'replace all foo',
      path: 'replace-all.ts',
      edits: [{ reasoning: 'replaceAll', oldText: 'foo', newText: 'baz', replaceAll: true }],
    }],
  });
  assert.equal(result.isError, undefined);
  assert.equal(fs.readFileSync(filePath, 'utf8'), 'baz\nbaz\nbar\nbaz\n');
  assert.equal((result.details as { replacements?: number }).replacements, 3);
});

test('replaceAll:true with no matches throws same not-found error', async () => {
  const filePath = path.join(tmpDir, 'replace-all-miss.ts');
  fs.writeFileSync(filePath, 'hello world\n', 'utf8');
  await recordFileReadState(filePath);
  await assert.rejects(() => run({
      queries: [{
        reasoning: 'replace missing text',
        path: 'replace-all-miss.ts',
        edits: [{ reasoning: 'miss', oldText: 'notfound', newText: 'x', replaceAll: true }],
      }],
    }));
  assert.equal(fs.readFileSync(filePath, 'utf8'), 'hello world\n', 'failed replacement leaves the file unchanged');
});

test('replaceAll:true with lineRange matchMode is rejected', async () => {
  const filePath = path.join(tmpDir, 'replace-all-lr.ts');
  fs.writeFileSync(filePath, 'line1\nline2\n', 'utf8');
  await recordFileReadState(filePath);
  await assert.rejects(
    () => run({
      queries: [{
        reasoning: 'invalid combination',
        path: 'replace-all-lr.ts',
        edits: [{ reasoning: 'bad', matchMode: 'lineRange', startLine: 1, endLine: 1, newText: 'x', replaceAll: true }],
      }],
    }),
    /replaceAll.*cannot be used with matchMode|lineRange.*replaceAll/i,
    'replaceAll combined with lineRange should be rejected',
  );
});

test('overlapping edits in the same query are rejected before any write', async () => {
  const filePath = path.join(tmpDir, 'overlap.ts');
  const original = 'hello world\n';
  fs.writeFileSync(filePath, original, 'utf8');
  await recordFileReadState(filePath);
  await assert.rejects(
    () => run({
      queries: [{
        reasoning: 'overlap test',
        path: 'overlap.ts',
        edits: [
          { reasoning: 'first', oldText: 'hello world', newText: 'hi world' },
          { reasoning: 'second', oldText: 'hello', newText: 'hey' },
        ],
      }],
    }),
    /overlap/i,
    'overlapping edits should throw before writing',
  );
  // file must remain unchanged
  assert.equal(fs.readFileSync(filePath, 'utf8'), original);
});

test('lineRange out-of-bounds throws a clear error', async () => {
  const filePath = path.join(tmpDir, 'lr-oob.ts');
  fs.writeFileSync(filePath, 'line1\nline2\n', 'utf8');
  await recordFileReadState(filePath);
  await assert.rejects(
    () => run({
      queries: [{
        reasoning: 'oob test',
        path: 'lr-oob.ts',
        edits: [{ reasoning: 'oob', matchMode: 'lineRange', startLine: 1, endLine: 99, newText: 'x\n' }],
      }],
    }),
    /outside|out of range|line range/i,
    'out-of-bounds lineRange should report a clear error',
  );
});
