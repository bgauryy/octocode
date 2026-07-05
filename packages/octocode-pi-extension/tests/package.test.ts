// Contract tests for the pi-extension. The awareness bridge migrated to direct
// imports from @octocodeai/octocode-memory (no subprocess, no Python). These tests
// assert the live API: createAwarenessBridge({pendingToolFiles}), handleToolCall,
// handleToolResult, and formatStatus using the real (isolated) SQLite store.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { test } from 'vitest';
import {
  MANAGED_BLOCK_END,
  MANAGED_BLOCK_START,
  OCTOCODE_DIRECT_TOOL_NAMES,
  OCTOCODE_SUPPORT_TOOL_NAMES,
  createAwarenessBridge,
  extractWriteTargetPaths,
  formatStatus,
  applyOctocodeUi,
  getThinkingStatus,
  getAssetPaths,
  getInstallSource,
  getOctocodeMemoryHome,
  listBundledSkills,
  listExtensionHarness,
  mergeManagedAppendSystem,
  parseSetupScope,
  shouldAppendSystemPrompt,
  splitArgs,
  setAgentProcessFactoryForTests,
} from '../src/index.js';

const packageRoot = path.resolve(import.meta.dirname, '..');
const distDir = path.join(packageRoot, 'dist');

// ─── Test helpers ─────────────────────────────────────────────────────────────

function withTempMemoryHome(fn: (tmp?: string) => void | Promise<void>) {
  return async () => {
    const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'octocode-pi-test-'));
    const previous = process.env['OCTOCODE_MEMORY_HOME'];
    process.env['OCTOCODE_MEMORY_HOME'] = tmp;
    try {
      await fn(tmp);
    } finally {
      if (previous === undefined) delete process.env['OCTOCODE_MEMORY_HOME'];
      else process.env['OCTOCODE_MEMORY_HOME'] = previous;
      fs.rmSync(tmp, { recursive: true, force: true });
    }
  };
}

interface IsolatedDbCtx {
  cwd: string;
  dbPath: string;
}

function withIsolatedDb(fn: (ctx: IsolatedDbCtx) => Promise<void>) {
  return async () => {
    const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'octocode-pi-db-'));
    const ctx: IsolatedDbCtx = { cwd: tmp, dbPath: path.join(tmp, 'awareness.sqlite3') };
    try {
      await fn(ctx);
    } finally {
      fs.rmSync(tmp, { recursive: true, force: true });
    }
  };
}

async function withAgentId(agentId: string, fn: () => Promise<void>): Promise<void> {
  const previous = process.env['OCTOCODE_AGENT_ID'];
  process.env['OCTOCODE_AGENT_ID'] = agentId;
  try {
    await fn();
  } finally {
    if (previous === undefined) delete process.env['OCTOCODE_AGENT_ID'];
    else process.env['OCTOCODE_AGENT_ID'] = previous;
  }
}

interface ToolDef {
  name: string;
  label?: string;
  description?: string;
  promptGuidelines?: string[];
  parameters: Record<string, unknown>;
  execute: (id: string, params: Record<string, unknown>, sig?: unknown, upd?: unknown, ctx?: unknown) => Promise<{ content: Array<{ type: string; text: string }>; isError?: boolean; details?: unknown }>;
  renderCall?: (args: unknown, theme?: unknown) => { render: (w?: number) => string[] };
  renderResult?: (result: unknown, opts: unknown, theme?: unknown) => { render: (w?: number) => string[] };
}

interface CaptureResult {
  tools: Map<string, ToolDef>;
  commands: Map<string, { description: string; handler: (args: string, ctx: unknown) => Promise<void> }>;
  sentUserMessages: Array<{ msg: string; opts?: Record<string, unknown> }>;
  pi: {
    getActiveTools(): string[];
    setActiveTools(names: string[]): void;
  };
  activeTools: string[];
}

async function captureExtensions(): Promise<CaptureResult> {
  const tools = new Map<string, ToolDef>();
  const commands = new Map<string, { description: string; handler: (args: string, ctx: unknown) => Promise<void> }>();
  const sentUserMessages: Array<{ msg: string; opts?: Record<string, unknown> }> = [];
  const activeTools = ['read', 'bash', 'edit', 'write'];
  const pi = {
    registerTool: (def: ToolDef) => { tools.set(def.name, def); },
    registerCommand: (name: string, cmd: { description: string; handler: (args: string, ctx: unknown) => Promise<void> }) => { commands.set(name, cmd); },
    sendUserMessage: (msg: string, opts?: Record<string, unknown>) => { sentUserMessages.push({ msg, opts }); },
    getActiveTools: () => [...activeTools],
    setActiveTools: (names: string[]) => { activeTools.splice(0, activeTools.length, ...names); },
    on: () => { /* no-op */ },
  };
  const extension = ((await import('../src/index.js')) as { default: (pi: unknown) => Promise<void> }).default;
  await extension(pi);
  return { tools, commands, sentUserMessages, pi, activeTools };
}

async function captureMemoryTools(): Promise<Map<string, ToolDef>> {
  const { tools } = await captureExtensions();
  return tools;
}

function invokeExecute(tool: ToolDef, params: Record<string, unknown>, ctx: unknown = { cwd: process.cwd() }) {
  return tool.execute('call-id', params, undefined, undefined, ctx);
}

// ─── Build artifact tests ─────────────────────────────────────────────────────

test('build copies the canonical system prompt', () => {
  const paths = getAssetPaths(distDir);
  const sourcePrompt = path.join(packageRoot, 'docs', 'PI', 'APPEND_SYSTEM.md');
  assert.equal(fs.existsSync(paths.systemPrompt), true);
  assert.equal(
    fs.readFileSync(paths.systemPrompt, 'utf8'),
    fs.readFileSync(sourcePrompt, 'utf8'),
  );
});

test('build copies bundled Octocode skills without secret env files', () => {
  assert.equal(fs.existsSync(path.join(distDir, 'bin', 'octocode.js')), false, 'Octocode CLI is NOT bundled — install separately via npm/npx');
  assert.equal(fs.existsSync(path.join(distDir, 'awareness', 'scripts', 'awareness.mjs')), true, 'awareness CLI remains bundled');

  const SKIPPED = ['octocode', 'octocode-awareness'];
  const skills = listBundledSkills(distDir);
  const sourceSkills = listBundledSkills(packageRoot);
  const rootSkills = listBundledSkills(path.resolve(packageRoot, '../..'));
  assert.deepEqual(skills, sourceSkills, 'dist matches package skills');
  assert.deepEqual(rootSkills.filter((s) => !SKIPPED.includes(s)), sourceSkills);
  assert.deepEqual(
    skills,
    [
      'octocode-brainstorming',
      'octocode-prompt-optimizer',
      'octocode-research',
      'octocode-rfc-generator',
      'octocode-roast',
      'octocode-skills',
      'octocode-stats',
    ].sort(),
  );

  const forbiddenEnv = path.join(distDir, 'skills', 'octocode-brainstorming', '.env');
  assert.equal(fs.existsSync(forbiddenEnv), false);
});

// ─── Functional tests ─────────────────────────────────────────────────────────

test('managed APPEND_SYSTEM block is inserted and replaced without duplication', () => {
  const first = mergeManagedAppendSystem('local rules\n', 'old octocode rules');
  assert.match(first, new RegExp(MANAGED_BLOCK_START));
  assert.match(first, new RegExp(MANAGED_BLOCK_END));

  const second = mergeManagedAppendSystem(first, 'new octocode rules');
  assert.equal(second.match(new RegExp(MANAGED_BLOCK_START, 'g'))?.length, 1);
  assert.match(second, /new octocode rules/);
  assert.doesNotMatch(second, /old octocode rules/);
});

test('argument parsing supports setup scopes and quoted installer args', () => {
  assert.equal(parseSetupScope('--global'), 'global');
  assert.equal(parseSetupScope('global'), 'global');
  assert.equal(parseSetupScope(''), 'project');
  assert.deepEqual(splitArgs('--ide "VS Code" --scope user'), ['--ide', 'VS Code', '--scope', 'user']);
});

test('system prompt append guard detects existing prompt', () => {
  const prompt = '<system_prompt>\nabc\n</system_prompt>';
  assert.equal(shouldAppendSystemPrompt('', prompt), true);
  assert.equal(shouldAppendSystemPrompt(prompt, prompt), false);
});

test('getInstallSource returns npm source for node_modules installs, local path otherwise', () => {
  const localSource = getInstallSource();
  assert.ok(!localSource.startsWith('npm:'), `expected local path, got ${localSource}`);
  assert.ok(path.isAbsolute(localSource), `expected absolute path, got ${localSource}`);

  const fakeNpmDir = path.join(os.tmpdir(), 'node_modules', '@octocodeai', 'pi-extension', 'dist');
  const npmSource = getInstallSource(fakeNpmDir);
  assert.equal(npmSource, 'npm:@octocodeai/pi-extension');
});

test('formatStatus reports the dist assets and memory module', withTempMemoryHome(() => {
  const status = formatStatus(distDir);
  assert.match(status, /system prompt: found/);
  assert.match(status, /octocode-research/);
  assert.match(status, /memory module: @octocodeai\/octocode-memory \(direct import\)/);
  assert.match(status, /memory DB: not yet created/);
  assert.match(status, /octocode tools: 14 native Pi tools/);
  assert.match(status, /CLI: use `npx octocode`/);
  assert.match(status, /disabled built-ins: none/);
}));

test('getOctocodeMemoryHome honors OCTOCODE_MEMORY_HOME', () => {
  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'octocode-home-'));
  const previous = process.env['OCTOCODE_MEMORY_HOME'];
  process.env['OCTOCODE_MEMORY_HOME'] = tmp;
  try {
    assert.equal(getOctocodeMemoryHome(), tmp);
  } finally {
    if (previous === undefined) delete process.env['OCTOCODE_MEMORY_HOME'];
    else process.env['OCTOCODE_MEMORY_HOME'] = previous;
    fs.rmSync(tmp, { recursive: true, force: true });
  }
});

test('write target extraction supports Pi write and edit inputs', () => {
  assert.deepEqual(extractWriteTargetPaths('read', { path: 'src/a.js' }), []);
  assert.deepEqual(
    extractWriteTargetPaths('write', { path: ' src/a.js ', filePaths: ['src/b.js', 'src/a.js'] }),
    ['src/a.js', 'src/b.js'],
  );
  assert.deepEqual(
    extractWriteTargetPaths('edit', { file_path: 'src/c.js', paths: ['src/d.js'] }),
    ['src/c.js', 'src/d.js'],
  );
});

test('awareness bridge claims a lock and releases it PENDING via the real DB', withIsolatedDb(async (ctx) => {
  await withAgentId('pi-test-agent', async () => {
    const bridge = createAwarenessBridge();

    const result = await bridge.handleToolCall(
      { toolName: 'write', toolCallId: 'tool-1', input: { path: 'src/a.js' } },
      ctx,
    );
    assert.equal(result, undefined);
    assert.deepEqual(bridge.pendingToolFiles.get('tool-1'), ['src/a.js']);

    assert.equal(fs.existsSync(ctx.dbPath), true);
    const { DatabaseSync } = await import('node:sqlite');
    const db = new DatabaseSync(ctx.dbPath);
    const active = db.prepare("SELECT COUNT(*) AS c FROM agent_intents WHERE status='ACTIVE'").get() as { c: number };
    assert.equal(active.c, 1);
    const locks = db.prepare('SELECT COUNT(*) AS c FROM file_locks').get() as { c: number };
    assert.equal(locks.c, 1);
    db.close();

    await bridge.handleToolResult({ toolCallId: 'tool-1' }, ctx);
    assert.equal(bridge.pendingToolFiles.has('tool-1'), false);

    const db2 = new DatabaseSync(ctx.dbPath);
    const pending = db2.prepare("SELECT COUNT(*) AS c FROM agent_intents WHERE status='PENDING'").get() as { c: number };
    assert.equal(pending.c, 1, 'release sets intent status PENDING (verification still owed)');
    const noLocks = db2.prepare('SELECT COUNT(*) AS c FROM file_locks').get() as { c: number };
    assert.equal(noLocks.c, 0, 'lock rows are deleted on release');
    db2.close();
  });
}));

test('awareness bridge blocks only on lock conflicts', withIsolatedDb(async (ctx) => {
  await withAgentId('other-agent', async () => {
    const holder = createAwarenessBridge();
    await holder.handleToolCall(
      { toolName: 'write', toolCallId: 'holder-1', input: { path: 'src/conflict.js' } },
      ctx,
    );
  });

  await withAgentId('pi-test-agent', async () => {
    const bridge = createAwarenessBridge();
    const result = await bridge.handleToolCall(
      { toolName: 'edit', toolCallId: 'tool-2', input: { path: 'src/conflict.js' } },
      ctx,
    ) as { block: boolean; reason: string };

    assert.equal(result.block, true);
    assert.match(result.reason, /Octocode awareness blocked this edit/);
    assert.match(result.reason, /other-agent/, 'conflict message names the holding agent');
    assert.equal(bridge.pendingToolFiles.has('tool-2'), false);
  });
}));

test('keeps Pi built-in read available for skill progressive disclosure', async () => {
  const { activeTools } = await captureExtensions();
  assert.equal(activeTools.includes('read'), true);
  assert.equal(activeTools.includes('bash'), true);
});

test('registers all Octocode direct tools as native Pi tools', async () => {
  const { tools } = await captureExtensions();
  assert.deepEqual(
    OCTOCODE_DIRECT_TOOL_NAMES.filter((toolName) => !tools.has(toolName)),
    [],
    'every direct Octocode tool is registered as a Pi tool',
  );
  assert.equal(OCTOCODE_DIRECT_TOOL_NAMES.length, 14);
  assert.ok(OCTOCODE_DIRECT_TOOL_NAMES.includes('unzip' as never), 'unzip is registered as a native tool');

  const localViewStructure = tools.get('localViewStructure')!;
  assert.equal(localViewStructure.label, 'Local Code: Local View Structure');
  assert.equal((localViewStructure.parameters as Record<string, unknown>)['type'], 'object');
  const props = (localViewStructure.parameters as { properties: Record<string, unknown> }).properties;
  assert.ok(props['queries'], 'bulk CLI tool schema exposed to Pi');
  const queriesItems = (props['queries'] as { items: { properties: Record<string, { maximum?: number }> } }).items;
  assert.equal(queriesItems.properties['itemsPerPage']?.maximum, 50);
  assert.equal(typeof localViewStructure.renderCall, 'function');
  assert.equal(typeof localViewStructure.renderResult, 'function');

  const theme = {
    bold: (text: string) => `**${text}**`,
    fg: (_color: string, text: string) => text,
  };
  assert.deepEqual(
    localViewStructure.renderCall!({ queries: [{ path: packageRoot }] }, theme).render(80)[0]!.includes('localViewStructure'),
    true,
  );
  assert.deepEqual(
    localViewStructure.renderResult!(
      { isError: false, content: [{ type: 'text', text: 'ok' }], details: { results: [1, 2] } },
      { expanded: false },
      theme,
    ).render(80)[0],
    '✓ localViewStructure · 2 items · expand for full output',
  );
  const expanded = localViewStructure.renderResult!(
    { isError: false, content: [{ type: 'text', text: 'x'.repeat(450) }], details: {} },
    { expanded: true },
    theme,
  ).render(80);
  // render(80) must respect the width contract: every line's visible width ≤ 80.
  // Strip ANSI escape codes to measure the visible character count.
  const ANSI_RE = /\x1B(?:[@-Z\\-_]|\[[0-?]*[ -/]*[@-~])/g;
  const previewVisibleWidth = expanded[1]!.replace(ANSI_RE, '').length;
  assert.ok(
    previewVisibleWidth <= 80,
    `expanded preview line visible width (${previewVisibleWidth}) must not exceed render width 80`,
  );
  assert.ok(expanded[1]!.includes('\u2026'), 'preview is truncated with ellipsis when wider than render width');
  assert.match(expanded[2]!, /user preview truncated \(150 chars hidden/);
});

test('applies Octocode Pi UI status and hidden thinking label', () => {
  const calls: Array<[string, ...string[]]> = [];
  // hasUI:true is required: applyOctocodeUi guards setStatus/setHiddenThinkingLabel
  // with ctx.hasUI because they are TUI/RPC-mode features.
  applyOctocodeUi({
    hasUI: true,
    ui: {
      theme: { fg: (_color: string, text: string) => `<${text}>`, bold: (t: string) => t },
      setHiddenThinkingLabel: (label: string) => calls.push(['thinking', label]),
      setStatus: (key: string, value: string) => calls.push(['status', key, value]),
    },
  });
  assert.deepEqual(calls, [
    ['thinking', 'Octocode thinking'],
    ['status', 'octocode', '<◆ Octocode>'],
    ['status', 'octocode-thinking', '<thinking: unknown model>'],
  ]);
  assert.equal(
    getThinkingStatus({ model: { id: 'gpt-5.5', reasoning: false } }, 'high'),
    'thinking: off (gpt-5.5 has reasoning:false)',
  );
  assert.equal(
    getThinkingStatus({ model: { id: 'claude', reasoning: true } }, 'high'),
    'thinking: high (claude)',
  );
});

test('CLI slash commands removed — extension commands are lean', async () => {
  const { commands } = await captureExtensions();
  // Extension-only commands still registered.
  assert.equal(commands.has('octocode-status'), true, 'extension status command is preserved');
  assert.equal(commands.has('octocode-harness'), true, 'harness listing command is registered');
  assert.equal(commands.has('octocode-setup'), true, 'setup command is registered');
  assert.equal(commands.has('octocode-skills-update'), true, 'skills-update command is registered');
  // Session-control internal trampoline stays for clear_context only.
  assert.equal(commands.has('_octocode-handoff-impl'), false, 'legacy handoff command removed');
  assert.equal(commands.has('_octocode-clear-context-impl'), true, 'internal clear command registered for command-context session control');
  // CLI slash commands are gone — users use `npx octocode` instead.
  assert.equal(commands.has('octocode-cli'), false, 'generic CLI escape hatch removed');
  assert.equal(commands.has('octocode-cli-status'), false, 'CLI status slash command removed');
  assert.equal(commands.has('octocode-search'), false, 'CLI search slash command removed');
  assert.equal(commands.has('octocode-auth'), false, 'CLI auth slash command removed');
});

test('registers split typed memory support tools with strict schemas', async () => {
  const { tools } = await captureExtensions();
  const memoryTools = [
    'memory_recall',
    'memory_record',
    'memory_reflect',
    'memory_workspace_status',
    'memory_refine_get',
    'memory_audit_unverified',
    'memory_verify',
    'memory_digest',
  ];

  for (const toolName of memoryTools) {
    assert.equal(tools.has(toolName), true, `${toolName} registered`);
    assert.ok(OCTOCODE_SUPPORT_TOOL_NAMES.includes(toolName as never));
  }
  assert.equal(tools.has('memory'), false, 'legacy type-discriminated memory tool removed');
  assert.equal(tools.has('memory_mine_weakness'), false, 'memory_mine_weakness removed — notifyGet briefing covers it');

  const recallParams = tools.get('memory_recall')!.parameters as { required?: string[] };
  assert.deepEqual(recallParams.required, ['query']);
  const recordParams = tools.get('memory_record')!.parameters as { required?: string[] };
  assert.deepEqual(recordParams.required, ['task_context', 'observation']);
  const verifyParams = tools.get('memory_verify')!.parameters as { required?: string[] };
  assert.deepEqual(verifyParams.required, ['intent_id']);
});

test('compact_context queues a continuation after compaction completes', async () => {
  const { tools, sentUserMessages } = await captureExtensions();
  const compactTool = tools.get('compact_context')!;
  let compactOptions: { customInstructions?: string; onComplete?: (opts?: unknown) => void; onError?: (err: Error) => void } = {};
  const notifications: Array<{ message: string; level: string }> = [];

  const result = await invokeExecute(
    compactTool,
    { instructions: 'focus on recent file changes' },
    {
      // hasUI:true required so onComplete notification fires (notify is guarded in TUI/RPC mode)
      hasUI: true,
      compact: (options: typeof compactOptions) => { compactOptions = options; },
      ui: { notify: (message: string, level: string) => notifications.push({ message, level }) },
    },
  );

  assert.match(result.content[0]!.text, /will continue after the summary is saved/);
  assert.equal(compactOptions.customInstructions, 'focus on recent file changes');
  assert.equal(sentUserMessages.length, 0, 'no follow-up before compaction completes');

  compactOptions.onComplete?.();
  assert.equal(sentUserMessages.length, 1);
  assert.match(sentUserMessages[0]!.msg, /Continue from the compacted context/);
  assert.equal(sentUserMessages[0]!.opts?.['deliverAs'], 'followUp');
  assert.deepEqual(notifications[0], {
    message: 'Compaction completed. Continuing from the compacted context.',
    level: 'info',
  });
});

test('compact_context reports compaction errors without queueing continuation', async () => {
  const { tools, sentUserMessages } = await captureExtensions();
  const compactTool = tools.get('compact_context')!;
  let compactOptions: { onError?: (err: Error) => void } = {};
  const notifications: Array<{ message: string; level: string }> = [];

  await invokeExecute(
    compactTool,
    {},
    {
      // hasUI:true required so onError notification fires (notify is guarded in TUI/RPC mode)
      hasUI: true,
      compact: (options: typeof compactOptions) => { compactOptions = options; },
      ui: { notify: (message: string, level: string) => notifications.push({ message, level }) },
    },
  );

  compactOptions.onError?.(new Error('Nothing to compact'));
  assert.equal(sentUserMessages.length, 0);
  assert.deepEqual(notifications[0], {
    message: 'Compaction failed: Nothing to compact',
    level: 'error',
  });
});

test('lists every extension harness surface', () => {
  const harness = listExtensionHarness(distDir);
  assert.deepEqual(harness.tools, OCTOCODE_DIRECT_TOOL_NAMES);
  assert.deepEqual(harness.supportTools, OCTOCODE_SUPPORT_TOOL_NAMES);
  assert.ok(harness.extensionCommands.includes('/octocode-harness'));
  assert.ok(harness.skills.includes('octocode-research'));
  assert.match(harness.cliNote, /npx octocode/, 'cliNote directs users to npx octocode');
  assert.ok(!('cliCommands' in harness), 'cliCommands removed from harness');
});

test('README lists every harness surface exposed by the extension', () => {
  const readme = fs.readFileSync(path.join(packageRoot, 'README.md'), 'utf8');
  const harness = listExtensionHarness(distDir);
  const missing: string[] = [];

  for (const toolName of harness.tools) {
    if (!readme.includes(`\`${toolName}\``)) missing.push(`native tool ${toolName}`);
  }
  for (const toolName of harness.supportTools) {
    if (!readme.includes(`\`${toolName}\``)) missing.push(`support tool ${toolName}`);
  }
  for (const command of harness.extensionCommands) {
    if (!readme.includes(`\`${command}`)) missing.push(`extension command ${command}`);
  }
  for (const skill of harness.skills) {
    if (!readme.includes(`\`${skill}\``)) missing.push(`skill ${skill}`);
  }

  assert.deepEqual(missing, []);
});

test('native Octocode local tool executes through the Pi tool wrapper', async () => {
  const { tools } = await captureExtensions();
  const tmp = fs.mkdtempSync(path.join(packageRoot, '.octocode-pi-local-tool-'));
  try {
    fs.writeFileSync(path.join(tmp, 'example.txt'), 'hello', 'utf8');
    const result = await invokeExecute(
      tools.get('localViewStructure')!,
      { queries: [{ path: tmp, filesOnly: true }] },
      { cwd: packageRoot },
    );

    assert.ok(Array.isArray(result.content));
    assert.equal(typeof result.content[0]?.text, 'string');
    assert.match(result.content[0]!.text, /example\.txt/);
    assert.equal(typeof result.details, 'object');
    assert.match(JSON.stringify(result.details), /example\.txt/);
  } finally {
    fs.rmSync(tmp, { recursive: true, force: true });
  }
});

test('native Octocode tool wrapper preserves responseCharLength pagination in text output', async () => {
  const { tools } = await captureExtensions();
  const tmp = fs.mkdtempSync(path.join(packageRoot, '.octocode-pi-local-tool-'));
  try {
    fs.writeFileSync(path.join(tmp, 'alpha.txt'), 'alpha', 'utf8');
    fs.writeFileSync(path.join(tmp, 'beta.txt'), 'beta', 'utf8');
    fs.writeFileSync(path.join(tmp, 'gamma.txt'), 'gamma', 'utf8');

    const result = await invokeExecute(
      tools.get('localViewStructure')!,
      { queries: [{ path: tmp, filesOnly: true }], responseCharLength: 80 },
      { cwd: packageRoot },
    );

    assert.match(result.content[0]!.text, /^# Response page 1\//);
    assert.ok(result.content[0]!.text.length < JSON.stringify(result.details).length);
    assert.equal((result.details as { responsePagination?: { hasMore?: boolean } })?.responsePagination?.hasMore, true);
  } finally {
    fs.rmSync(tmp, { recursive: true, force: true });
  }
});

test('native Octocode tool wrapper throws so Pi marks execution failed', async () => {
  const { tools } = await captureExtensions();
  await assert.rejects(
    () => invokeExecute(
      tools.get('localViewStructure')!,
      { queries: [{}] },
      { cwd: packageRoot },
    ),
    /path|expected string/,
  );
});

// ─── Memory tool output/input contract (token-efficient shapes) ───────────────

test('memory_recall output omits bookkeeping fields and null/empty provenance', withIsolatedDb(async (ctx) => {
  const tools = await captureMemoryTools();
  await invokeExecute(tools.get('memory_record')!, {
    observation: 'Never edit dist/ directly — build regenerates it from src.',
    task_context: 'Prevents silently losing dist edits when build overwrites.',
    label: 'GOTCHA',
    importance: 8,
    tags: ['build'],
    references: ['file:///abs/path/build.mjs:214'],
  }, ctx);
  await invokeExecute(tools.get('memory_record')!, {
    observation: 'Use lexical search; embeddings are overkill for dist builds.',
    task_context: 'Choosing a memory store at build scale.',
    label: 'DECISION',
    importance: 6,
  }, ctx);

  const res = await invokeExecute(tools.get('memory_recall')!, { query: 'build dist edit', limit: 5 }, ctx);
  const payload = JSON.parse(res.content[0]!.text) as {
    count: number;
    memories: Array<Record<string, unknown>>;
  };

  assert.deepEqual(Object.keys(payload).sort(), ['count', 'memories']);
  assert.ok(payload.count >= 1);

  const gotcha = payload.memories.find((m) => m['label'] === 'GOTCHA');
  assert.ok(gotcha, 'GOTCHA memory recalled');
  for (const key of ['memory_id', 'observation', 'task_context', 'label', 'importance', 'score', 'tags', 'references']) {
    assert.ok(key in gotcha, `kept field ${key} present`);
  }
  for (const key of ['agent_id', 'state', 'superseded_by', 'access_count', 'last_accessed_at',
    'decay_half_life_days', 'valid_from', 'valid_to', 'expired_at', 'file_tree_fingerprint',
    'created_at', 'updated_at', 'workspace_path', 'repo', 'ref', 'file', 'failure_signature']) {
    assert.ok(!(key in gotcha), `dropped field ${key}`);
  }

  const decision = payload.memories.find((m) => m['label'] === 'DECISION');
  assert.ok(decision, 'DECISION memory recalled');
  assert.ok(!('references' in decision), 'empty references omitted, not []');
  assert.ok(!('tags' in decision), 'empty tags omitted, not []');
}));

test('memory_record output does not echo back observation/task_context', withIsolatedDb(async (ctx) => {
  const tools = await captureMemoryTools();
  const res = await invokeExecute(tools.get('memory_record')!, {
    observation: 'A long lesson text that should NOT be echoed back to the agent.',
    task_context: 'A long rationale that should NOT be echoed back either.',
    label: 'GOTCHA',
    importance: 7,
  }, ctx);
  const text = res.content[0]!.text;
  const payload = JSON.parse(text) as Record<string, unknown>;

  assert.ok(typeof payload['memory_id'] === 'string' && (payload['memory_id'] as string).startsWith('mem_'), 'memory_id returned');
  assert.equal(payload['importance'], 7);
  assert.equal(payload['label'], 'GOTCHA');
  assert.equal(typeof payload['novelty'], 'number');
  assert.ok(!('observation' in payload));
  assert.ok(!('task_context' in payload));
  assert.ok(!('memory' in payload));
  assert.ok(!('db_path' in payload) && !('schema_version' in payload) && !('ok' in payload));
}));

test('memory_record skips similar memories without echoing prose', withIsolatedDb(async (ctx) => {
  const tools = await captureMemoryTools();
  const first = await invokeExecute(tools.get('memory_record')!, {
    observation: 'Never edit generated dist files because the build overwrites them.',
    task_context: 'Duplicate memory prevention.',
    label: 'GOTCHA',
    importance: 7,
  }, ctx);
  const firstPayload = JSON.parse(first.content[0]!.text) as { memory_id: string };
  const second = await invokeExecute(tools.get('memory_record')!, {
    observation: 'Never edit generated dist files because the build overwrites them.',
    task_context: 'Duplicate memory prevention.',
    label: 'GOTCHA',
    importance: 7,
  }, ctx);
  const payload = JSON.parse(second.content[0]!.text) as {
    skipped: boolean;
    reason: string;
    similar: Array<{ memory_id: string }>;
  };
  assert.equal(payload.skipped, true);
  assert.equal(payload.reason, 'similar_memory_exists');
  assert.equal(payload.similar[0]!.memory_id, firstPayload.memory_id);
  assert.ok(!JSON.stringify(payload).includes('Never edit generated dist'));
}));

test('memory_record importance defaults from label when omitted (smart input)', withIsolatedDb(async (ctx) => {
  const tools = await captureMemoryTools();
  const memProps = (tools.get('memory_record')!.parameters as { properties: Record<string, { description: string }> }).properties;
  assert.match(memProps['label']?.description ?? '', /EXPERIENCE/);
  const res = await invokeExecute(tools.get('memory_record')!, {
    observation: 'Security-sensitive secret leak gotcha.',
    task_context: 'Choosing default importance without making the agent guess.',
    label: 'SECURITY',
  }, ctx);
  const payload = JSON.parse(res.content[0]!.text) as { importance: number; label: string };
  assert.equal(payload.importance, 9, 'SECURITY defaults to importance 9');
  assert.equal(payload.label, 'SECURITY');
}));



test('memory_record stores file/folder/repo scope and memory_recall can find it', withIsolatedDb(async (ctx) => {
  const tools = await captureMemoryTools();
  const record = await invokeExecute(tools.get('memory_record')!, {
    task_context: 'Scoped repo memory for docs and source files.',
    observation: 'Scoped memories should connect lessons to files, folders, and repo-wide docs.',
    label: 'DOCS',
    file: 'README.md',
    files: ['docs/PI/APPEND_SYSTEM.md'],
    folders: ['docs'],
    repo: 'bgauryy/octocode',
    references: ['file:AGENTS.md'],
  }, ctx);
  const recordPayload = JSON.parse(record.content[0]!.text) as { memory_id: string };
  assert.match(recordPayload.memory_id, /^mem_/);

  const recall = await invokeExecute(tools.get('memory_recall')!, {
    query: 'scoped docs lesson',
    file: 'README.md',
    folders: ['docs'],
    repo: 'bgauryy/octocode',
    limit: 5,
  }, ctx);
  const payload = JSON.parse(recall.content[0]!.text) as { memories: Array<Record<string, unknown>> };
  const scoped = payload.memories.find((m) => m['memory_id'] === recordPayload.memory_id)!;
  assert.ok(scoped, 'scoped memory recalled');
  assert.ok((scoped['file'] as string).endsWith('/README.md'));
  assert.deepEqual(scoped['references'], [
    'file:AGENTS.md',
    'file:README.md',
    'file:docs/PI/APPEND_SYSTEM.md',
    'dir:docs',
  ]);
  assert.equal(scoped['repo'], 'bgauryy/octocode');
}));

test('memory_digest marks expired valid_to memories stale', withIsolatedDb(async (ctx) => {
  const tools = await captureMemoryTools();
  await invokeExecute(tools.get('memory_record')!, {
    task_context: 'Temporary migration workaround.',
    observation: 'This workaround expires immediately.',
    label: 'GOTCHA',
    valid_to: '2000-01-01T00:00:00Z',
  }, ctx);

  const digestResult = JSON.parse((await invokeExecute(tools.get('memory_digest')!, {}, ctx)).content[0]!.text) as { archived_memories: number };
  assert.equal(digestResult.archived_memories, 1);

  const recall = JSON.parse((await invokeExecute(tools.get('memory_recall')!, { query: 'Temporary migration workaround', limit: 5 }, ctx)).content[0]!.text) as { count: number };
  assert.equal(recall.count, 0, 'expired memory no longer appears in ACTIVE recall');
}));

test('memory_audit_unverified and memory_verify clear pending edit intents', withIsolatedDb(async (ctx) => {
  await withAgentId('pi-test-agent', async () => {
    const tools = await captureMemoryTools();
    const bridge = createAwarenessBridge();
    await bridge.handleToolCall(
      { toolName: 'write', toolCallId: 'verify-tool-1', input: { path: 'src/a.js' } },
      ctx,
    );
    await bridge.handleToolResult({ toolCallId: 'verify-tool-1' }, ctx);

    const audit = await invokeExecute(tools.get('memory_audit_unverified')!, {}, ctx);
    const auditPayload = JSON.parse(audit.content[0]!.text) as {
      count: number;
      pending: Array<{ intent_id: string; test_plan: string; files?: string[] }>;
    };
    assert.equal((audit.details as { exit: number }).exit, 1, 'pending edits make audit exit non-zero');
    assert.equal(auditPayload.count, 1);
    assert.match(auditPayload.pending[0]!.intent_id, /^intent_/);
    assert.equal(auditPayload.pending[0]!.files?.length, 1);
    assert.ok(auditPayload.pending[0]!.files![0]!.endsWith('/src/a.js'));

    const verify = await invokeExecute(tools.get('memory_verify')!, {
      intent_id: auditPayload.pending[0]!.intent_id,
      status: 'SUCCESS',
    }, ctx);
    const verifyPayload = JSON.parse(verify.content[0]!.text) as { status: string };
    assert.equal((verify.details as { exit: number }).exit, 0);
    assert.equal(verifyPayload.status, 'SUCCESS');

    const clear = await invokeExecute(tools.get('memory_audit_unverified')!, {}, ctx);
    const clearPayload = JSON.parse(clear.content[0]!.text) as { count: number };
    assert.equal((clear.details as { exit: number }).exit, 0);
    assert.equal(clearPayload.count, 0);
  });
}));

test('memory_reflect output drops stub fields and only hints next when an action is pending', withIsolatedDb(async (ctx) => {
  const tools = await captureMemoryTools();
  const bare = await invokeExecute(tools.get('memory_reflect')!, {
    task: 'read a file',
    outcome: 'worked',
    lesson: 'nothing durable',
  }, ctx);
  const barePayload = JSON.parse(bare.content[0]!.text) as Record<string, unknown>;
  assert.deepEqual(Object.keys(barePayload).sort(), ['memory_id', 'outcome']);
  assert.equal(barePayload['outcome'], 'worked');
  assert.ok(!('next' in barePayload), 'no next hint when nothing actionable');
  assert.ok(!('eval_failure_count' in barePayload) && !('eval_failure_ids' in barePayload));

  const withFix = await invokeExecute(tools.get('memory_reflect')!, {
    task: 'fixed bug',
    outcome: 'partial',
    lesson: 'x',
    fix_repo: 'patch the shared fn',
  }, ctx);
  const fixPayload = JSON.parse(withFix.content[0]!.text) as { refinement_id?: string; next?: string };
  assert.ok(fixPayload.refinement_id?.startsWith('ref_'));
  assert.ok('next' in fixPayload, 'next hint present when a refinement is created');
  assert.match(fixPayload.next ?? '', /memory_refine_get/);
}));

test('memory self-healing tools use lean outputs', withIsolatedDb(async (ctx) => {
  const tools = await captureMemoryTools();
  await invokeExecute(tools.get('memory_reflect')!, {
    task: 'flaky test',
    outcome: 'failed',
    lesson: 'retrying without reading output repeats failures',
    failure_signature: 'mechanism:test|cause:unread-output',
    fix_repo: 'add clearer test output',
  }, ctx);
  await invokeExecute(tools.get('memory_reflect')!, {
    task: 'flaky test again',
    outcome: 'failed',
    lesson: 'retrying without reading output repeats failures',
    failure_signature: 'mechanism:test|cause:unread-output',
  }, ctx);

  // workspace_status replaces mine_weakness — shows locks and counts
  const wsStatus = JSON.parse((await invokeExecute(tools.get('memory_workspace_status')!, {}, ctx)).content[0]!.text) as {
    active_memories: number;
    pending_intents: number;
    active_intents: number;
    open_refinements: number;
  };
  assert.ok(typeof wsStatus.active_memories === 'number', 'active_memories is a number');
  assert.ok(!('ok' in wsStatus) && !('schema_version' in wsStatus), 'workspace_status output is lean');

  const refinements = JSON.parse((await invokeExecute(tools.get('memory_refine_get')!, {}, ctx)).content[0]!.text) as {
    count: number;
    refinements: Array<{ refinement_id: string }>;
  };
  assert.equal(refinements.count, 1);
  assert.ok(refinements.refinements[0]!.refinement_id.startsWith('ref_'));
  assert.ok(!('reasoning' in refinements.refinements[0]!));

  const digestResult = JSON.parse((await invokeExecute(tools.get('memory_digest')!, {}, ctx)).content[0]!.text) as Record<string, unknown>;
  assert.deepEqual(Object.keys(digestResult).sort(), ['archived_memories', 'fts_rebuilt', 'pruned_locks', 'pruned_old']);

  // dry_run mode returns prediction fields only
  const dryResult = JSON.parse((await invokeExecute(tools.get('memory_digest')!, { dry_run: true }, ctx)).content[0]!.text) as Record<string, unknown>;
  assert.deepEqual(Object.keys(dryResult).sort(), ['dry_run', 'would_archive', 'would_prune_locks', 'would_prune_old']);
  assert.equal(dryResult['dry_run'], true);
}));

test('awareness bridge fails open on non-conflict errors', async () => {
  await withAgentId('pi-test-agent', async () => {
    const messages: Array<{ level: string; message: string }> = [];
    const bridge = createAwarenessBridge();
    const result = await bridge.handleToolCall(
      { toolName: 'write', toolCallId: 'tool-3', input: { path: 'src/a.js' } },
      {
        cwd: '/repo',
        dbPath: '/dev/null/cannot-create-dir/awareness.sqlite3',
        ui: { notify: (message: string, level?: string) => messages.push({ level: level ?? 'info', message }) },
      },
    );

    assert.equal(result, undefined, 'fail-open: undefined, not {block}');
    assert.equal(bridge.pendingToolFiles.has('tool-3'), false, 'no pending entry when pre-flight threw');
    assert.equal(messages.length, 1);
    assert.equal(messages[0]!.level, 'warning');
    assert.match(messages[0]!.message, /Octocode awareness warning; continuing:/);
  });
});

// ─── spawnAgent / AgentMessage: real parallel process orchestration ─────────

interface MockAgentProcess {
  stdinWrites: string[];
  stdin: { write(data: string): void; end(): void };
  stdout: { on(event: string, cb: (chunk: Buffer | string) => void): void };
  stderr: { on(event: string, cb: (chunk: Buffer | string) => void): void };
  on(event: string, cb: (...args: unknown[]) => void): void;
  kill(signal?: NodeJS.Signals): boolean;
  killed?: boolean;
  emitStdout(line: unknown): void;
  emitStderr(text: string): void;
  close(code?: number, signal?: string): void;
}

function createMockAgentProcess(): MockAgentProcess {
  const stdoutHandlers: Array<(chunk: Buffer | string) => void> = [];
  const stderrHandlers: Array<(chunk: Buffer | string) => void> = [];
  const closeHandlers: Array<(...args: unknown[]) => void> = [];
  const errorHandlers: Array<(...args: unknown[]) => void> = [];
  const proc: MockAgentProcess = {
    stdinWrites: [],
    stdin: {
      write(data: string) { proc.stdinWrites.push(data); },
      end() { /* no-op */ },
    },
    stdout: { on(event, cb) { if (event === 'data') stdoutHandlers.push(cb); } },
    stderr: { on(event, cb) { if (event === 'data') stderrHandlers.push(cb); } },
    on(event, cb) {
      if (event === 'close') closeHandlers.push(cb);
      if (event === 'error') errorHandlers.push(cb);
    },
    kill() { proc.killed = true; return true; },
    emitStdout(line: unknown) { stdoutHandlers.forEach((cb) => cb(`${JSON.stringify(line)}\n`)); },
    emitStderr(text: string) { stderrHandlers.forEach((cb) => cb(text)); },
    close(code = 0, signal?: string) { closeHandlers.forEach((cb) => cb(code, signal)); },
  };
  void errorHandlers;
  return proc;
}

test('spawnAgent starts a lean RPC Pi process and AgentMessage can list/status/send', async () => {
  const spawned: Array<{ command: string; args: string[]; options: { cwd?: string } ; proc: MockAgentProcess }> = [];
  setAgentProcessFactoryForTests((command, args, options) => {
    const proc = createMockAgentProcess();
    spawned.push({ command, args, options, proc });
    return proc;
  });
  try {
    const { tools } = await captureExtensions();
    const spawnTool = tools.get('spawnAgent')!;
    const messageTool = tools.get('AgentMessage')!;
    assert.ok(spawnTool, 'spawnAgent registered');
    assert.ok(messageTool, 'AgentMessage registered');
    assert.match(spawnTool.promptGuidelines?.join('\n') ?? '', /delegation materially helps/);
    assert.match(messageTool.promptGuidelines?.join('\n') ?? '', /synthesize findings instead of dumping raw worker JSON/);
    assert.equal(tools.has('handoff_context'), false, 'legacy handoff_context removed');

    const result = await invokeExecute(
      spawnTool,
      {
        task: 'check the docs',
        context: 'Relevant file: docs/a.md',
        name: 'docs-scout',
        model: 'sonnet:high',
        tools: ['read', 'grep'],
      },
      { cwd: '/repo' },
    );

    assert.equal(spawned.length, 1);
    assert.ok(spawned[0]!.args.includes('--mode'));
    assert.ok(spawned[0]!.args.includes('rpc'));
    assert.ok(spawned[0]!.args.includes('--no-extensions'));
    assert.ok(spawned[0]!.args.includes('--no-skills'));
    assert.ok(spawned[0]!.args.includes('--model'));
    assert.ok(spawned[0]!.args.includes('sonnet:high'));
    assert.ok(spawned[0]!.args.includes('--exclude-tools'));
    assert.ok(spawned[0]!.args.includes('spawnAgent,AgentMessage'));
    assert.ok(spawned[0]!.args.includes('--tools'));
    assert.ok(spawned[0]!.args.includes('read,grep'));
    assert.equal(spawned[0]!.options.cwd, '/repo');
    assert.match(spawned[0]!.proc.stdinWrites[0]!, /Context for this delegated agent/);
    assert.match(spawned[0]!.proc.stdinWrites[0]!, /check the docs/);

    const agentId = (result.details as { agent: { agentId: string } }).agent.agentId;
    const list = await invokeExecute(messageTool, { action: 'list' });
    assert.match(list.content[0]!.text, new RegExp(agentId));

    spawned[0]!.proc.emitStdout({ type: 'agent_end', messages: [] });
    await invokeExecute(messageTool, { action: 'wait', agentId, timeoutMs: 1000 });
    await invokeExecute(messageTool, { action: 'send', agentId, message: 'also inspect tests' });
    const idleSend = JSON.parse(spawned[0]!.proc.stdinWrites.at(-1)!);
    assert.equal(idleSend.type, 'prompt');
    assert.equal(idleSend.message, 'also inspect tests');
    assert.equal('streamingBehavior' in idleSend, false, 'idle send must not force followUp');

    await invokeExecute(messageTool, { action: 'send', agentId, message: 'queue after current turn' });
    const runningSend = JSON.parse(spawned[0]!.proc.stdinWrites.at(-1)!);
    assert.equal(runningSend.streamingBehavior, 'followUp', 'running send defaults to followUp');
  } finally {
    setAgentProcessFactoryForTests(null);
  }
});

test('spawnAgent does not register recursively inside spawned workers', async () => {
  const previous = process.env['OCTOCODE_PI_SUBAGENT'];
  process.env['OCTOCODE_PI_SUBAGENT'] = '1';
  try {
    const { tools } = await captureExtensions();
    assert.equal(tools.has('spawnAgent'), false);
    assert.equal(tools.has('AgentMessage'), false);
    assert.equal(tools.has('localSearchCode'), true, 'Octocode tools remain available in octocode worker mode');
  } finally {
    if (previous === undefined) delete process.env['OCTOCODE_PI_SUBAGENT'];
    else process.env['OCTOCODE_PI_SUBAGENT'] = previous;
  }
});

test('AgentMessage wait collects worker output and kill terminates stale workers', async () => {
  const spawned: MockAgentProcess[] = [];
  setAgentProcessFactoryForTests((_command, _args, _options) => {
    const proc = createMockAgentProcess();
    spawned.push(proc);
    return proc;
  });
  try {
    const { tools } = await captureExtensions();
    const spawnTool = tools.get('spawnAgent')!;
    const messageTool = tools.get('AgentMessage')!;

    const first = await invokeExecute(spawnTool, { task: 'produce output', resourceMode: 'default' }, { cwd: '/repo' });
    const firstId = (first.details as { agent: { agentId: string } }).agent.agentId;
    spawned[0]!.emitStdout({ type: 'message_end', message: { role: 'assistant', content: [{ type: 'text', text: 'worker result' }] } });
    spawned[0]!.emitStdout({ type: 'agent_end', messages: [] });
    const waited = await invokeExecute(messageTool, { action: 'wait', agentId: firstId, timeoutMs: 1000 });
    assert.match(waited.content[0]!.text, /worker result/);
    assert.ok(spawned[0]!.stdinWrites[0]!.includes('produce output'));
    assert.equal(spawned[0]!.stdinWrites[0]!.includes('spawnAgent'), false);

    const second = await invokeExecute(spawnTool, { task: 'hang around' }, { cwd: '/repo' });
    const secondId = (second.details as { agent: { agentId: string } }).agent.agentId;
    const killed = await invokeExecute(messageTool, { action: 'kill', agentId: secondId, remove: true });
    assert.match(killed.content[0]!.text, /killed/);
    assert.equal(spawned[1]!.killed, true);
  } finally {
    setAgentProcessFactoryForTests(null);
  }
});
