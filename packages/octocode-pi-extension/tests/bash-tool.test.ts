import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { afterAll, beforeAll, test } from 'vitest';
import { allowLocalFixtureProcesses } from '../../../test-utils/external-effects-guard.js';
import {
  BASH_CONTEXT_MAX_CHARS,
  BASH_HEAD_CHARS,
  BASH_TAIL_CHARS,
  bashLooksMutatingForPlanMode,
  classifyEnvExfilCommand,
  extractBashWriteTargets,
  formatBashOutput,
  registerBashTool,
} from '../src/tools/bash-tool.js';
import { registerUniqueTool } from '../src/tools/octocode-tools.js';
import { resetApprovalStore } from '../src/tools/approval.js';
import { enterPlanMode, exitPlanMode } from '../src/tools/plan-mode.js';
import type { ToolCallResult, ToolDefinition } from '../src/types.js';

let restoreProcessGuard: () => void;
beforeAll(() => {
  restoreProcessGuard = allowLocalFixtureProcesses();
});
afterAll(() => restoreProcessGuard());

function executeBash(
  tool: object,
  id: string,
  params: Record<string, unknown>,
  signal?: AbortSignal,
  ctx?: { cwd?: string },
): Promise<ToolCallResult> {
  const definition = tool as ToolDefinition;
  const envelope = Array.isArray(params['queries']) ? params : { queries: [params] };
  return definition.execute(id, envelope, signal, undefined, ctx);
}

test('extractBashWriteTargets finds redirects and tee', () => {
  const cwd = '/tmp/work';
  assert.deepEqual(extractBashWriteTargets('echo hi > out.txt', cwd), [
    path.join(cwd, 'out.txt'),
  ]);
  assert.deepEqual(extractBashWriteTargets('echo hi >> /tmp/abs.log', cwd), [
    '/tmp/abs.log',
  ]);
  assert.ok(
    extractBashWriteTargets('printf x | tee nested/a.txt', cwd).includes(
      path.join(cwd, 'nested/a.txt'),
    ),
  );
});

test('extractBashWriteTargets finds cp/mv destinations', () => {
  const cwd = '/tmp/work';
  const targets = extractBashWriteTargets('cp a.ts b.ts', cwd);
  assert.deepEqual(targets, [path.join(cwd, 'b.ts')]);
});

test('bash abort terminates the shell process and resolves without hanging', async () => {
  const { default: extension } = await import('../src/index.js');
  const tools = new Map<
    string,
    {
      name: string;
      execute: (
        id: string,
        params: Record<string, unknown>,
        sig?: AbortSignal,
        upd?: unknown,
        ctx?: { cwd?: string },
      ) => Promise<{ content: Array<{ type: string; text: string }>; isError?: boolean }>;
    }
  >();
  await extension({
    on: () => undefined,
    sendUserMessage: () => undefined,
    registerTool: (def: { name: string }) => {
      tools.set(def.name, def as (typeof tools extends Map<string, infer V> ? V : never));
    },
    getActiveTools: () => ['bash'],
    setActiveTools: () => undefined,
  });
  const bash = tools.get('bash')!;
  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'octocode-bash-abort-'));
  const controller = new AbortController();
  try {
    const promise = executeBash(
      bash,
      'abort',
      { command: 'trap "exit 143" TERM; while true; do echo err >&2; sleep 0.05; done', reasoning: 'verify abort handling for long-running commands' },
      controller.signal,
      { cwd: tmp },
    );
    setTimeout(() => controller.abort(), 50);
    const result = await Promise.race([
      promise,
      new Promise<never>((_, reject) => setTimeout(() => reject(new Error('bash abort timed out')), 2_000)),
    ]);
    assert.equal(result.isError, true);
    assert.match((result.content[0] as { text: string }).text, /err|\(aborted\)/);
  } finally {
    controller.abort();
    fs.rmSync(tmp, { recursive: true, force: true });
  }
});

test('bash override blocks writes outside allowed roots', async () => {
  const { default: extension } = await import('../src/index.js');
  const tools = new Map<
    string,
    {
      name: string;
      label?: string;
      execute: (
        id: string,
        params: Record<string, unknown>,
        sig?: AbortSignal,
        upd?: unknown,
        ctx?: { cwd?: string },
      ) => Promise<{ content: Array<{ type: string; text: string }>; isError?: boolean }>;
    }
  >();
  const active = ['bash', 'edit', 'write'];
  await extension({
    on: () => undefined,
    sendUserMessage: () => undefined,
    registerTool: (def: { name: string }) => {
      tools.set(def.name, def as (typeof tools extends Map<string, infer V> ? V : never));
    },
    getActiveTools: () => [...active],
    setActiveTools: (names: string[]) => {
      active.splice(0, active.length, ...names);
    },
  });
  const bash = tools.get('bash')!;
  assert.equal(bash.label, 'bash (Octocode)');
  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'octocode-bash-'));
  try {
    await assert.rejects(
      () =>
        executeBash(
          bash,
          '1',
          { command: `echo pwned > /usr/octocode-bash-block-${process.pid}.txt`, reasoning: 'verify path guard blocks unsafe write targets' },
          undefined,
          { cwd: tmp },
        ),
      /bash write blocked|outside the allowed roots/,
    );
    const ok = await executeBash(
      bash,
      '2',
      { command: 'echo hello > ok.txt && cat ok.txt', reasoning: 'verify allowed writes inside the workspace still run' },
      undefined,
      { cwd: tmp },
    );
    assert.match((ok.content[0] as { text: string }).text, /hello/);
    assert.equal(fs.readFileSync(path.join(tmp, 'ok.txt'), 'utf8').trim(), 'hello');
  } finally {
    fs.rmSync(tmp, { recursive: true, force: true });
  }
});

test('bash override rejects missing reasoning', async () => {
  const { default: extension } = await import('../src/index.js');
  const tools = new Map<string, { name: string; execute: ToolDefinition['execute'] }>();
  await extension({
    on: () => undefined,
    sendUserMessage: () => undefined,
    registerTool: (def: { name: string }) => {
      tools.set(def.name, def as (typeof tools extends Map<string, infer V> ? V : never));
    },
    getActiveTools: () => ['bash'],
    setActiveTools: () => undefined,
  });
  await assert.rejects(
    () => executeBash(tools.get('bash')!, 'missing-reasoning', { queries: [{ command: 'echo hi' }] }, undefined, { cwd: os.tmpdir() }),
    /requires non-empty reasoning/,
  );
});

test('extractBashWriteTargets: sed/perl in-place targets the FILE, never the script', () => {
  const cwd = '/repo';
  // BSD/macOS: `sed -i '' <script> file` — the address script starts with '/'
  // and must NOT be treated as an absolute output path (the original bug).
  assert.deepEqual(
    extractBashWriteTargets(`sed -i '' '/^    "x": 1,$/d' package.json`, cwd),
    [path.join(cwd, 'package.json')],
  );
  // GNU: `sed -i <script> file` (no separate suffix).
  assert.deepEqual(extractBashWriteTargets(`sed -i 's/a/b/' f.txt`, cwd), [path.join(cwd, 'f.txt')]);
  // GNU attached suffix.
  assert.deepEqual(extractBashWriteTargets(`sed -i.bak 's/a/b/' f.txt`, cwd), [path.join(cwd, 'f.txt')]);
  // Multiple files.
  assert.deepEqual(
    extractBashWriteTargets(`sed -i '' 's/a/b/' a.txt b.txt`, cwd).sort(),
    [path.join(cwd, 'a.txt'), path.join(cwd, 'b.txt')].sort(),
  );
  // Explicit -e script: every positional is a file.
  assert.deepEqual(extractBashWriteTargets(`sed -i '' -e 's/a/b/' f.txt`, cwd), [path.join(cwd, 'f.txt')]);
  // perl -i -pe.
  assert.deepEqual(extractBashWriteTargets(`perl -i -pe 's/a/b/' f.txt`, cwd), [path.join(cwd, 'f.txt')]);
  // Not in-place → no write target from sed.
  assert.deepEqual(extractBashWriteTargets(`sed 's/a/b/' f.txt`, cwd), []);
});

test('bashLooksMutatingForPlanMode allows read-only commands and flags common mutations', () => {
  const cwd = '/tmp/work';
  assert.equal(bashLooksMutatingForPlanMode('git status --short', cwd), false);
  assert.equal(bashLooksMutatingForPlanMode('npm test -- --runInBand', cwd), false);
  assert.equal(bashLooksMutatingForPlanMode('echo hi > out.txt', cwd), true);
  assert.equal(bashLooksMutatingForPlanMode('touch out.txt', cwd), true);
  assert.equal(bashLooksMutatingForPlanMode(`sed -i 's/a/b/' f.txt`, cwd), true);
  assert.equal(bashLooksMutatingForPlanMode('node -e "require(\'fs\').writeFileSync(\'x\',\'y\')"', cwd), true);
});

test('classifyEnvExfilCommand flags obvious inherited environment dumps but not ordinary commands', () => {
  assert.equal(classifyEnvExfilCommand('env')?.actionClass, 'system');
  assert.equal(classifyEnvExfilCommand('printenv | sort')?.actionClass, 'system');
  assert.equal(classifyEnvExfilCommand('echo $GITHUB_TOKEN')?.actionClass, 'system');
  assert.equal(classifyEnvExfilCommand('echo hello'), null);
});

test('bash formats large output as a single bounded context block with head+tail and disk-spill', async () => {
  assert.equal(BASH_CONTEXT_MAX_CHARS, 4_000);
  assert.equal(BASH_HEAD_CHARS, 1_000);
  assert.equal(BASH_TAIL_CHARS, 3_000);
  const full = `${'a'.repeat(21_000)}\n${'b'.repeat(21_000)}\ntail`;
  const result = await formatBashOutput(full);

  assert.ok(result.truncated, 'large output must be marked truncated');
  assert.equal(result.totalChars, full.length);
  // Single block stays near 4K; only the reference/read hint is extra.
  assert.ok(result.text.length <= BASH_CONTEXT_MAX_CHARS + 768, 'single block stays bounded');
  // Head preserved
  assert.ok(result.text.startsWith('a'.repeat(Math.min(BASH_HEAD_CHARS, 20))), 'head chars preserved');
  // Tail preserved
  assert.ok(result.text.endsWith('tail'), 'tail preserved');
  // Truncation notice present
  assert.ok(result.text.includes('omitted'), 'truncation notice present');
  // Full output disk-spilled
  assert.ok(result.tempFilePath, 'temp file path returned when truncated');
  const diskContent = fs.readFileSync(result.tempFilePath!, 'utf8');
  assert.equal(diskContent, full, 'disk file contains the complete original output');
});

test('bash formatBashOutput returns small text inline and still keeps an ephemeral reference', async () => {
  const small = 'hello world\n'.repeat(10);
  const result = await formatBashOutput(small);
  assert.equal(result.truncated, false);
  assert.equal(result.text, small);
  assert.equal(result.totalChars, small.length);
  assert.ok(result.tempFilePath);
  assert.equal(fs.readFileSync(result.tempFilePath!, 'utf8'), small);
});

test('bash streams output beyond the in-memory preview cap to a chunk-readable artifact', async () => {
  const tool = loadBashTool();
  const result = await executeBash(tool, 'large-stream', {
    command: `node -e "process.stdout.write('A'.repeat(200000) + 'TAIL')"`,
    reasoning: 'verify large output stays artifact-backed',
  }, undefined, { cwd: os.tmpdir() });
  const visible = result.content.flatMap((part) => part.type === 'text' ? [part.text] : []).join('');
  const details = result.details as { outputPath?: string; stdout?: string; totalChars?: number };
  assert.ok(visible.length <= BASH_CONTEXT_MAX_CHARS + 1_000);
  assert.match(visible, /localGetFileContent/);
  assert.ok(visible.endsWith('TAIL'), 'bounded model preview retains the actual process tail');
  assert.equal(details.stdout, undefined, 'raw stdout is not duplicated into details');
  assert.equal(details.totalChars, 200_004, 'metadata reports actual output, not the capped preview source');
  assert.ok(details.outputPath);
  const disk = fs.readFileSync(details.outputPath!, 'utf8');
  assert.equal(disk.length, 200_004);
  assert.ok(disk.endsWith('TAIL'), 'artifact retains output emitted after the memory preview cap');
});

test('bash head+tail truncation never splits a Unicode surrogate pair at slice boundaries', async () => {
  // Place a 4-byte emoji (surrogate pair in JS) exactly at the head cut boundary
  const emoji = '\uD83D\uDE00'; // U+1F600 = high surrogate 0xD83D + low surrogate 0xDE00
  // Put emoji at position BASH_HEAD_CHARS - 1 so it straddles the default boundary
  const raw =
    'a'.repeat(BASH_HEAD_CHARS - 1) +
    emoji +
    'x'.repeat(BASH_TAIL_CHARS + 5_000) +
    'b'.repeat(BASH_TAIL_CHARS);
  const result = await formatBashOutput(raw);
  assert.ok(result.truncated);
  // The formatted text must not contain a lone high surrogate without a following low surrogate
  for (let i = 0; i < result.text.length; i++) {
    const code = result.text.charCodeAt(i);
    if (code >= 0xD800 && code <= 0xDBFF) {
      const next = result.text.charCodeAt(i + 1);
      assert.ok(next >= 0xDC00 && next <= 0xDFFF, `lone high surrogate at index ${i}`);
    }
    if (code >= 0xDC00 && code <= 0xDFFF && i > 0) {
      const prev = result.text.charCodeAt(i - 1);
      assert.ok(prev >= 0xD800 && prev <= 0xDBFF, `lone low surrogate at index ${i}`);
    }
  }
});

test('bash execution requires approval for obvious environment exfiltration and fails closed without UI', async () => {
  resetApprovalStore();
  const tool = loadBashTool();
  await assert.rejects(
    () => executeBash(tool, 'env-dump', { command: 'env', reasoning: 'verify env exfil approval' }, undefined, { cwd: os.tmpdir() }),
    /Expose inherited environment variables.*requires user approval.*non-interactive/i,
  );
});

test('bash execution runs obvious environment exfiltration after explicit approval', async () => {
  resetApprovalStore();
  const tool = loadBashTool();
  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'octocode-bash-env-'));
  const prev = process.env['OCTOCODE_BASH_ENV_TEST_TOKEN'];
  process.env['OCTOCODE_BASH_ENV_TEST_TOKEN'] = 'visible-after-approval';
  try {
    const ctx = {
      cwd: tmp,
      hasUI: true,
      ui: { select: async () => 'Yes (run once)' },
    };
    const ok = await executeBash(
      tool,
      'env-approved',
      { command: 'printf "$OCTOCODE_BASH_ENV_TEST_TOKEN"', reasoning: 'verify approved env access still inherits env' },
      undefined,
      ctx,
    );
    assert.equal(ok.isError ?? false, false);
    assert.match((ok.content[0] as { text: string }).text, /visible-after-approval/);
  } finally {
    if (prev === undefined) delete process.env['OCTOCODE_BASH_ENV_TEST_TOKEN'];
    else process.env['OCTOCODE_BASH_ENV_TEST_TOKEN'] = prev;
    fs.rmSync(tmp, { recursive: true, force: true });
  }
});

test('bash execution blocks mutating commands while plan mode is active', async () => {
  const tool = loadBashTool();
  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'octocode-bash-plan-mode-'));
  const ctx = { cwd: tmp } as never;
  enterPlanMode(ctx);
  try {
    await assert.rejects(
      () => executeBash(tool, 'plan-write', { command: 'echo hi > out.txt', reasoning: 'verify plan mode blocks mutating bash' }, undefined, ctx),
      /shell, and external effects stay blocked until the user separately starts implementation/,
    );
    await assert.rejects(
      () => executeBash(tool, 'plan-read', { command: 'pwd', reasoning: 'verify shell stays conservatively disabled before Start' }, undefined, ctx),
      /shell, and external effects stay blocked/,
    );
  } finally {
    exitPlanMode(ctx);
    fs.rmSync(tmp, { recursive: true, force: true });
  }
});

// ─── renderResult ────────────────────────────────────────────────────────────

import { Type } from 'typebox';

function loadBashTool(): ToolDefinition {
  let def: ToolDefinition | undefined;
  registerBashTool({ registerTool: (d: ToolDefinition) => { def = d; } }, Type as never, new Set<string>(), registerUniqueTool);
  assert.ok(def, 'bash tool registered');
  return def!;
}

const renderTheme = { fg: (c: string, t: string) => `<${c}>${t}</${c}>`, bold: (t: string) => t };

test('bash renderers identify the Octocode override in call, partial, and result rows', () => {
  const tool = loadBashTool();
  const call = (tool.renderCall!({ queries: [{ command: 'echo hi', reasoning: 'verify shell rendering' }] }, renderTheme as never) as { render(w: number): string[] }).render(120);
  assert.match(call[0]!, /bash \(Octocode\)/);
  assert.match(call[1]!, /verify shell rendering/);
  assert.doesNotMatch(call[1]!, /why:|reasoning:/i);
  const partial = (tool.renderResult!({ content: [] }, { isPartial: true }, renderTheme as never) as { render(w: number): string[] }).render(120);
  assert.match(partial[0]!, /bash \(Octocode\)/);
});

test('bash renderResult always renders the result: header with exit/lines, a short head when collapsed, all when expanded', () => {
  const tool = loadBashTool();
  const out = Array.from({ length: 6 }, (_, i) => `line ${i + 1}`).join('\n');
  const result: ToolCallResult = { content: [{ type: 'text', text: out }], details: { code: 0, stdout: out, stderr: '' } };
  const collapsed = (tool.renderResult!(result, { expanded: false }, renderTheme as never) as { render(w: number): string[] }).render(120);
  assert.match(collapsed[0]!, /<success>✓<\/success>/);
  assert.match(collapsed[0]!, /bash \(Octocode\)/);
  assert.match(collapsed[0]!, /exit 0.*6 lines/);
  assert.equal(collapsed.length, 1 + 3 + 1, 'header + 3 head lines + hidden hint');
  assert.match(collapsed.at(-1)!, /3 more lines/);
  const expanded = (tool.renderResult!(result, { expanded: true }, renderTheme as never) as { render(w: number): string[] }).render(120);
  assert.equal(expanded.length, 7);
  const failed: ToolCallResult = { content: [{ type: 'text', text: 'boom\n(exit 2)' }], isError: true, details: { code: 2, stdout: '', stderr: 'boom' } };
  const err = (tool.renderResult!(failed, { expanded: false }, renderTheme as never) as { render(w: number): string[] }).render(120);
  assert.match(err[0]!, /<error>✗<\/error>/);
  assert.match(err[0]!, /bash \(Octocode\)/);
  assert.match(err[1]!, /^  <error>boom<\/error>/);
});

test('bash expanded UI uses a smart head/tail preview while the tool result remains complete', async () => {
  const tool = loadBashTool();
  const out = `HEAD\n${'middle\n'.repeat(5_000)}TAIL`;
  const formatted = await formatBashOutput(out);
  const result: ToolCallResult = {
    content: [{ type: 'text', text: formatted.text }],
    details: { code: 0, outputPath: formatted.tempFilePath, totalChars: out.length },
  };
  const expanded = (tool.renderResult!(result, { expanded: true }, renderTheme as never) as { render(w: number): string[] }).render(120);
  assert.ok(expanded.some((line) => line.includes('HEAD')));
  assert.ok(expanded.some((line) => line.includes('TAIL')));
  assert.ok(expanded.some((line) => /hidden in UI/.test(line)));
  assert.ok(expanded.length < 3_000, 'expanded rendering stays bounded');
  assert.equal((result.details as { totalChars: number }).totalChars, out.length, 'renderer keeps only compact metadata');
});
