// Contract tests for the pi-extension. The awareness bridge migrated to direct
// imports from @octocodeai/octocode-memory (no subprocess, no Python). These tests
// assert the live API: createAwarenessBridge({pendingToolFiles}), handleToolCall,
// handleToolResult, and formatStatus using the real (isolated) SQLite store.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import {
  MANAGED_BLOCK_END,
  MANAGED_BLOCK_START,
  createAwarenessBridge,
  extractWriteTargetPaths,
  formatStatus,
  getAssetPaths,
  getInstallSource,
  getOctocodeMemoryHome,
  listBundledSkills,
  mergeManagedAppendSystem,
  parseSetupScope,
  shouldAppendSystemPrompt,
  splitArgs,
} from '../src/index.js';

const packageRoot = path.resolve(import.meta.dirname, '..');
const distDir = path.join(packageRoot, 'dist');

function withTempMemoryHome(fn) {
  return async () => {
    const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'octocode-pi-test-'));
    const previous = process.env.OCTOCODE_MEMORY_HOME;
    process.env.OCTOCODE_MEMORY_HOME = tmp;
    try {
      await fn(tmp);
    } finally {
      if (previous === undefined) delete process.env.OCTOCODE_MEMORY_HOME;
      else process.env.OCTOCODE_MEMORY_HOME = previous;
      fs.rmSync(tmp, { recursive: true, force: true });
    }
  };
}

// Env-free isolation for async DB tests (Node 22+ runs top-level tests concurrently, so
// tests that mutate the shared OCTOCODE_MEMORY_HOME env race). ctx.dbPath routes the tool/
// bridge straight at a temp SQLite file with no global env dependency — race-free.
function withIsolatedDb(fn) {
  return async () => {
    const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'octocode-pi-db-'));
    const ctx = { cwd: tmp, dbPath: path.join(tmp, 'awareness.sqlite3') };
    try {
      await fn(ctx);
    } finally {
      fs.rmSync(tmp, { recursive: true, force: true });
    }
  };
}

async function withAgentId(agentId, fn) {
  const previous = process.env.OCTOCODE_AGENT_ID;
  process.env.OCTOCODE_AGENT_ID = agentId;
  try {
    await fn();
  } finally {
    if (previous === undefined) {
      delete process.env.OCTOCODE_AGENT_ID;
    } else {
      process.env.OCTOCODE_AGENT_ID = previous;
    }
  }
}

test('build copies the canonical system prompt', () => {
  const paths = getAssetPaths(distDir);
  const sourcePrompt = path.join(packageRoot, 'docs', 'PI', 'APPEND_SYSTEM.md');
  assert.equal(fs.existsSync(paths.systemPrompt), true);
  assert.equal(fs.readFileSync(paths.systemPrompt, 'utf8'), fs.readFileSync(sourcePrompt, 'utf8'));

  const prompt = fs.readFileSync(paths.systemPrompt, 'utf8');
  assert.match(prompt, /<operating_model>/);
  assert.match(prompt, /<how_to_build>/);
  // The doc-placement + memory blocks added in the memory-hardening pass.
  assert.match(prompt, /<doc_placement>/);
  assert.match(prompt, /<memory>/);
  assert.match(prompt, /memory_recall/);
  assert.match(prompt, /VERIFY GATE/);
  assert.match(prompt, /Supersede, don't stack/);
});

test('build copies bundled Octocode skills without secret env files', () => {
  // octocode (architecture docs) and octocode-awareness are intentionally excluded.
  // octocode-awareness ships as native memory_* tools (memory_recall/record/reflect), not a skill.
  // octocode-stats is bundled — it reads ~/.octocode/stats.json and is referenced in the system prompt.
  const SKIPPED = ['octocode', 'octocode-awareness'];
  const skills = listBundledSkills(distDir);
  const sourceSkills = listBundledSkills(packageRoot);
  const rootSkills = listBundledSkills(path.resolve(packageRoot, '../..'));
  assert.deepEqual(skills, sourceSkills, 'dist matches package skills');
  // package/dist == root skills minus the intentionally-skipped ones (build.mjs SKIPPED_SKILLS).
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
    ].sort()
  );

  // No secret .env files must be present in any built skill.
  const forbiddenEnv = path.join(distDir, 'skills', 'octocode-brainstorming', '.env');
  assert.equal(fs.existsSync(forbiddenEnv), false);
});

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
  // In the dev workspace, extensionDir is inside the package, not node_modules
  assert.ok(!localSource.startsWith('npm:'), `expected local path, got ${localSource}`);
  assert.ok(path.isAbsolute(localSource), `expected absolute path, got ${localSource}`);

  // Simulate an npm install location
  const fakeNpmDir = path.join(os.tmpdir(), 'node_modules', '@octocodeai', 'pi-extension', 'dist');
  const npmSource = getInstallSource(fakeNpmDir);
  assert.equal(npmSource, 'npm:@octocodeai/pi-extension');
});

test('formatStatus reports the dist assets and memory module', withTempMemoryHome(() => {
  const status = formatStatus(distDir);
  // The awareness bridge moved from subprocess scripts to a direct module import.
  assert.match(status, /system prompt: found/);
  assert.match(status, /octocode-research/);
  assert.match(status, /memory module: @octocodeai\/octocode-memory \(direct import\)/);
  // DB path resolves under the isolated temp memory home.
  assert.match(status, /memory DB: not yet created/);
  assert.match(status, /octocode CLI: bundled v/);
}));

test('getOctocodeMemoryHome honors OCTOCODE_MEMORY_HOME', () => {
  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'octocode-home-'));
  const previous = process.env.OCTOCODE_MEMORY_HOME;
  process.env.OCTOCODE_MEMORY_HOME = tmp;
  try {
    assert.equal(getOctocodeMemoryHome(), tmp);
  } finally {
    if (previous === undefined) delete process.env.OCTOCODE_MEMORY_HOME;
    else process.env.OCTOCODE_MEMORY_HOME = previous;
    fs.rmSync(tmp, { recursive: true, force: true });
  }
});

test('write target extraction supports Pi write and edit inputs', () => {
  assert.deepEqual(extractWriteTargetPaths('read', { path: 'src/a.js' }), []);
  assert.deepEqual(extractWriteTargetPaths('write', { path: ' src/a.js ', filePaths: ['src/b.js', 'src/a.js'] }), [
    'src/a.js',
    'src/b.js',
  ]);
  assert.deepEqual(extractWriteTargetPaths('edit', { file_path: 'src/c.js', paths: ['src/d.js'] }), [
    'src/c.js',
    'src/d.js',
  ]);
});

test('awareness bridge claims a lock and releases it PENDING via the real DB', withIsolatedDb(async (ctx) => {
  await withAgentId('pi-test-agent', async () => {
    const bridge = createAwarenessBridge();

    const result = await bridge.handleToolCall(
      { toolName: 'write', toolCallId: 'tool-1', input: { path: 'src/a.js' } },
      ctx
    );
    // Success / fail-open both return undefined; only a lock conflict returns {block}.
    assert.equal(result, undefined);
    assert.deepEqual(bridge.pendingToolFiles.get('tool-1'), ['src/a.js']);

    // The isolated SQLite store was created at the ctx.dbPath override.
    assert.equal(fs.existsSync(ctx.dbPath), true);
    const { DatabaseSync } = await import('node:sqlite');
    const db = new DatabaseSync(ctx.dbPath);
    const active = db.prepare("SELECT COUNT(*) AS c FROM agent_intents WHERE status='ACTIVE'").get();
    assert.equal(active.c, 1);
    const locks = db.prepare("SELECT COUNT(*) AS c FROM file_locks").get();
    assert.equal(locks.c, 1);
    db.close();

    await bridge.handleToolResult({ toolCallId: 'tool-1' }, ctx);
    assert.equal(bridge.pendingToolFiles.has('tool-1'), false);

    const db2 = new DatabaseSync(ctx.dbPath);
    const pending = db2.prepare("SELECT COUNT(*) AS c FROM agent_intents WHERE status='PENDING'").get();
    assert.equal(pending.c, 1, 'release sets intent status PENDING (verification still owed)');
    const noLocks = db2.prepare("SELECT COUNT(*) AS c FROM file_locks").get();
    assert.equal(noLocks.c, 0, 'lock rows are deleted on release');
    db2.close();
  });
}));

test('awareness bridge blocks only on lock conflicts', withIsolatedDb(async (ctx) => {
  // Held by a different agent first: pre-claim the same absolute file.
  await withAgentId('other-agent', async () => {
    const holder = createAwarenessBridge();
    await holder.handleToolCall(
      { toolName: 'write', toolCallId: 'holder-1', input: { path: 'src/conflict.js' } },
      ctx
    );
  });

  await withAgentId('pi-test-agent', async () => {
    const bridge = createAwarenessBridge();
    const result = await bridge.handleToolCall(
      { toolName: 'edit', toolCallId: 'tool-2', input: { path: 'src/conflict.js' } },
      ctx
    );

    assert.equal(result.block, true);
    assert.match(result.reason, /Octocode awareness blocked this edit/);
    assert.match(result.reason, /other-agent/, 'conflict message names the holding agent');
    assert.equal(bridge.pendingToolFiles.has('tool-2'), false);
  });
}));

// ─── Memory tool output/input contract (token-efficient shapes) ───────────────
// Capture tools + commands registered by wireOctocodePiExtension via a fake pi.
// `sentUserMessages` records the follow-up messages the wiring fires (e.g. for handoff).
async function captureExtensions() {
  const tools = new Map();
  const commands = new Map();
  const sentUserMessages = [];
  const pi = {
    registerTool: (def) => { tools.set(def.name, def); },
    registerCommand: (name, cmd) => { commands.set(name, cmd); },
    sendUserMessage: (msg, opts) => { sentUserMessages.push({ msg, opts }); },
    on: () => {},
  };
  const extension = (await import('../src/index.js')).default;
  await extension(pi);
  return { tools, commands, sentUserMessages, pi };
}

// Legacy alias used by the memory-tool tests.
async function captureMemoryTools() {
  const { tools } = await captureExtensions();
  return tools;
}

function invokeExecute(tool, params, ctx = { cwd: process.cwd() }) {
  return tool.execute('call-id', params, undefined, undefined, ctx);
}

test('memory_recall output omits bookkeeping fields and null/empty provenance', withIsolatedDb(async (ctx) => {
  const tools = await captureMemoryTools();
  // Seed two memories that both match the query (share the term "build").
  await invokeExecute(tools.get('memory_record'), {
    observation: 'Never edit dist/ directly — build regenerates it from src.',
    task_context: 'Prevents silently losing dist edits when build overwrites.',
    label: 'GOTCHA',
    importance: 8,
    tags: ['build'],
    references: ['file:///abs/path/build.mjs:214'],
  }, ctx);
  await invokeExecute(tools.get('memory_record'), {
    observation: 'Use lexical search; embeddings are overkill for dist builds.',
    task_context: 'Choosing a memory store at build scale.',
    label: 'DECISION',
    importance: 6,
  }, ctx);

  const res = await invokeExecute(tools.get('memory_recall'), { query: 'build dist edit', limit: 5 }, ctx);
  const payload = JSON.parse(res.content[0].text);

  // Top-level: only count + memories. No db_path/schema_version/ok/mode/sort/states.
  assert.deepEqual(Object.keys(payload).sort(), ['count', 'memories']);
  assert.ok(payload.count >= 1);

  const gotcha = payload.memories.find((m) => m.label === 'GOTCHA');
  assert.ok(gotcha, 'GOTCHA memory recalled');
  // Action-relevant fields kept…
  for (const key of ['memory_id', 'observation', 'task_context', 'label', 'importance', 'score', 'tags', 'references']) {
    assert.ok(key in gotcha, `kept field ${key} present`);
  }
  // …bookkeeping + null provenance fields dropped (omitted, never emitted as null).
  for (const key of ['agent_id', 'state', 'superseded_by', 'access_count', 'last_accessed_at',
    'decay_half_life_days', 'valid_from', 'valid_to', 'expired_at', 'file_tree_fingerprint',
    'created_at', 'updated_at', 'workspace_path', 'repo', 'ref', 'file', 'failure_signature']) {
    assert.ok(!(key in gotcha), `dropped field ${key}`);
  }

  // Empty arrays are omitted too (token-efficient): the tagless/referenceless DECISION
  // memory carries neither key, rather than emitting them as [].
  const decision = payload.memories.find((m) => m.label === 'DECISION');
  assert.ok(decision, 'DECISION memory recalled');
  assert.ok(!('references' in decision), 'empty references omitted, not []');
  assert.ok(!('tags' in decision), 'empty tags omitted, not []');
}));

test('memory_record output does not echo back observation/task_context', withIsolatedDb(async (ctx) => {
  const tools = await captureMemoryTools();
  const res = await invokeExecute(tools.get('memory_record'), {
    observation: 'A long lesson text that should NOT be echoed back to the agent.',
    task_context: 'A long rationale that should NOT be echoed back either.',
    label: 'GOTCHA',
    importance: 7,
  }, ctx);
  const text = res.content[0].text;
  const payload = JSON.parse(text);

  // Only the actionable confirmation + id remain.
  assert.ok(payload.memory_id && payload.memory_id.startsWith('mem_'), 'memory_id returned');
  assert.equal(payload.importance, 7);
  assert.equal(payload.label, 'GOTCHA');
  // No echo of the just-sent prose, no envelope noise.
  assert.ok(!('observation' in payload));
  assert.ok(!('task_context' in payload));
  assert.ok(!('memory' in payload));
  assert.ok(!('db_path' in payload) && !('schema_version' in payload) && !('ok' in payload));
}));

test('memory_record: importance defaults from label when omitted (smart input)', withIsolatedDb(async (ctx) => {
  const tools = await captureMemoryTools();
  const res = await invokeExecute(tools.get('memory_record'), {
    observation: 'Security-sensitive secret leak gotcha.',
    task_context: 'Choosing default importance without making the agent guess.',
    label: 'SECURITY',
  }, ctx);
  const payload = JSON.parse(res.content[0].text);
  assert.equal(payload.importance, 9, 'SECURITY defaults to importance 9');
  assert.equal(payload.label, 'SECURITY');
}));

test('memory_reflect output drops stub fields and only hints next when an action is pending', withIsolatedDb(async (ctx) => {
  const tools = await captureMemoryTools();
  // Case 1: no repo-fix, no harness-fix → bare confirmation, no `next` hint.
  const bare = await invokeExecute(tools.get('memory_reflect'), {
    task: 'read a file', outcome: 'worked', lesson: 'nothing durable',
  }, ctx);
  const barePayload = JSON.parse(bare.content[0].text);
  assert.deepEqual(Object.keys(barePayload).sort(), ['memory_id', 'outcome']);
  assert.equal(barePayload.outcome, 'worked');
  assert.ok(!('next' in barePayload), 'no next hint when nothing actionable');
  assert.ok(!('eval_failure_count' in barePayload) && !('eval_failure_ids' in barePayload));

  // Case 2: fix_repo set → refinement_id present + a short next hint.
  const withFix = await invokeExecute(tools.get('memory_reflect'), {
    task: 'fixed bug', outcome: 'partial', lesson: 'x', fix_repo: 'patch the shared fn',
  }, ctx);
  const fixPayload = JSON.parse(withFix.content[0].text);
  assert.ok(fixPayload.refinement_id && fixPayload.refinement_id.startsWith('ref_'));
  assert.ok('next' in fixPayload, 'next hint present when a refinement is created');
  assert.match(fixPayload.next, /refine-get/);
}));

test('awareness bridge fails open on non-conflict errors', async () => {
  await withAgentId('pi-test-agent', async () => {
    const messages = [];
    // Route the bridge at an unwritable dbPath via ctx (no global env poisoning):
    // /dev/null cannot hold a directory, so connectDb's mkdirSync throws.
    const bridge = createAwarenessBridge();
    const result = await bridge.handleToolCall(
      { toolName: 'write', toolCallId: 'tool-3', input: { path: 'src/a.js' } },
      { cwd: '/repo', dbPath: '/dev/null/cannot-create-dir/awareness.sqlite3', ui: { notify: (message, level) => messages.push({ level, message }) } }
    );

    assert.equal(result, undefined, 'fail-open: undefined, not {block}');
    assert.equal(bridge.pendingToolFiles.has('tool-3'), false, 'no pending entry when pre-flight threw');
    assert.equal(messages.length, 1);
    assert.equal(messages[0].level, 'warning');
    assert.match(messages[0].message, /Octocode awareness warning; continuing:/);
  });
});

// ─── handoff_context: FIFO queue, no silent drop ───────────────────────────────
test('handoff_context queues multiple delegations (no silent single-slot drop)', async () => {
  const { tools, commands, sentUserMessages } = await captureExtensions();
  const handoffTool = tools.get('handoff_context');
  const handoffCmd = commands.get('octocode-handoff');
  assert.ok(handoffTool && handoffCmd, 'handoff_context tool + octocode-handoff command registered');

  // Queue three independent handoffs in one turn. With the prior single-slot, two would drop;
  // with the queue all three persist and the command drains them in FIFO order.
  const r1 = await invokeExecute(handoffTool, { summary: 'goal-one', kickoff: 'do one' });
  const r2 = await invokeExecute(handoffTool, { summary: 'goal-two', kickoff: 'do two' });
  const r3 = await invokeExecute(handoffTool, { summary: 'goal-three', kickoff: 'do three' });
  assert.match(r1.content[0].text, /position 1|Handoff queued — a new session/);
  assert.match(r2.content[0].text, /position 2/);
  assert.match(r3.content[0].text, /position 3/);
  // Each handoff fires a /octocode-handoff follow-up; the command drains one queued payload per call.
  assert.equal(sentUserMessages.length, 3);
  assert.equal(sentUserMessages.every((m) => m.msg === '/octocode-handoff' && m.opts?.deliverAs === 'followUp'), true);

  // The command handler (simulate the session manager) drains FIFO and is empty after the last.
  const drainedSummaries = [];
  for (let i = 0; i < 3; i++) {
    await handoffCmd.handler([], {
      newSession: async ({ setup, withSession }) => {
        let seeded = '';
        await setup({ appendMessage: (m) => { seeded += m.content.map((c) => c.text).join(''); } });
        drainedSummaries.push(seeded);
        await withSession?.({ sendUserMessage: () => {} });
      },
    });
  }
  // All three queued handoffs were drained (no silent drop), in FIFO order by summary.
  assert.equal(drainedSummaries.length, 3);
  assert.ok(drainedSummaries[0].includes('goal-one'), 'first drained = first queued');
  assert.ok(drainedSummaries[1].includes('goal-two'), 'FIFO order preserved');
  assert.ok(drainedSummaries[2].includes('goal-three'), 'last drained = last queued');

  // After three drains, a fourth command call reports an empty queue (no silent failure coded elsewhere).
  const warned = [];
  await handoffCmd.handler([], { ui: { notify: (m) => { warned.push(m); } }, newSession: async () => {} });
  assert.equal(warned.length, 1);
  assert.match(warned[0], /no queued handoff payload/);
});

test('handoff_context embeds artifactDir into the summary so the subagent writes on disk', async () => {
  const { tools, commands } = await captureExtensions();
  const handoffTool = tools.get('handoff_context');
  const handoffCmd = commands.get('octocode-handoff');
  const artifactDir = '.octocode/handoffs/20260703-1200-verify-gate-build';
  await invokeExecute(handoffTool, { summary: 'goal-x', artifactDir });

  let seededSummary = '';
  await handoffCmd.handler([], {
    newSession: async ({ setup }) => {
      await setup({ appendMessage: (m) => { seededSummary += m.content.map((c) => c.text).join(''); } });
    },
  });
  assert.match(seededSummary, /Deliverable path.*\.octocode\/handoffs\/20260703-1200-verify-gate-build/);
});
