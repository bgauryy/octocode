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
  getAwarenessBridgeStatus,
  getInstallSource,
  listBundledSkills,
  mergeManagedAppendSystem,
  parseSetupScope,
  shouldAppendSystemPrompt,
  splitArgs,
} from '../src/index.js';

const packageRoot = path.resolve(import.meta.dirname, '..');
const distDir = path.join(packageRoot, 'dist');

function createAwarenessFixture() {
  const baseDir = fs.mkdtempSync(path.join(os.tmpdir(), 'octocode-pi-extension-'));
  const scriptPath = path.join(baseDir, 'skills', 'octocode-awareness', 'scripts', 'awareness.py');
  fs.mkdirSync(path.dirname(scriptPath), { recursive: true });
  fs.writeFileSync(scriptPath, '# awareness fixture\n', 'utf8');
  return baseDir;
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
});

test('build copies bundled Octocode skills without secret env files', () => {
  const SKIPPED = ['octocode', 'octocode-awareness', 'octocode-stats'];
  const skills = listBundledSkills(distDir);
  const sourceSkills = listBundledSkills(packageRoot);
  const rootSkills = listBundledSkills(path.resolve(packageRoot, '../..'));
  assert.deepEqual(skills, sourceSkills, 'dist matches package skills');
  // package/dist == root skills minus the intentionally-skipped ones (build.mjs SKIPPED_SKILLS).
  assert.deepEqual(rootSkills.filter((s) => !SKIPPED.includes(s)), sourceSkills);
  // octocode, octocode-awareness, and octocode-stats are intentionally excluded by
  // build.mjs SKIPPED_SKILLS (awareness ships as native memory_* tools, not a skill).
  assert.deepEqual(
    skills,
    [
      'octocode-brainstorming',
      'octocode-prompt-optimizer',
      'octocode-research',
      'octocode-rfc-generator',
      'octocode-roast',
      'octocode-skills',
    ].sort()
  );

  const forbiddenEnv = path.join(distDir, 'skills', 'octocode-brainstorming', '.env');
  const allowedExample = path.join(distDir, 'skills', 'octocode-brainstorming', '.env.example');
  assert.equal(fs.existsSync(forbiddenEnv), false);
  assert.equal(fs.existsSync(allowedExample), true);
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

test('status reports the dist assets', () => {
  const status = formatStatus(distDir);
  assert.match(status, /system prompt: found/);
  assert.match(status, /octocode-research/);
  assert.match(status, /awareness file locks: (available|missing)/);
});

test('awareness status detects the bundled script', () => {
  const missingBaseDir = fs.mkdtempSync(path.join(os.tmpdir(), 'octocode-pi-missing-'));
  assert.equal(getAwarenessBridgeStatus(missingBaseDir), 'missing');
  assert.equal(getAwarenessBridgeStatus(createAwarenessFixture()), 'available');
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

test('awareness bridge claims and releases Pi write tool calls', async () => {
  await withAgentId('pi-test-agent', async () => {
    const calls = [];
    const baseDir = createAwarenessFixture();
    const bridge = createAwarenessBridge({
      baseDir,
      runCommand: async (command, args, options) => {
        calls.push({ args, command, options });
        return { status: 0, stdout: '', stderr: '' };
      },
    });

    const ctx = { cwd: '/repo' };
    const result = await bridge.handleToolCall(
      { toolName: 'write', toolCallId: 'tool-1', input: { path: 'src/a.js' } },
      ctx
    );
    assert.equal(result, undefined);
    assert.deepEqual(bridge.pendingToolFiles.get('tool-1'), ['src/a.js']);
    assert.deepEqual(calls[0].args, [
      path.join(baseDir, 'skills', 'octocode-awareness', 'scripts', 'awareness.py'),
      'pre-flight-intent',
      '--agent-id',
      'pi-test-agent',
      '--workspace',
      '/repo',
      '--rationale',
      'auto: Pi write/edit tool call via octocode-pi-extension',
      '--test-plan',
      'post-edit verification',
      '--ttl-minutes',
      '15',
      '--target-file',
      'src/a.js',
    ]);

    await bridge.handleToolResult({ toolCallId: 'tool-1' }, ctx);
    assert.equal(bridge.pendingToolFiles.has('tool-1'), false);
    assert.deepEqual(calls[1].args, [
      path.join(baseDir, 'skills', 'octocode-awareness', 'scripts', 'awareness.py'),
      'release-file-lock',
      '--agent-id',
      'pi-test-agent',
      '--status',
      'PENDING',
      '--target-file',
      'src/a.js',
    ]);
  });
});

test('awareness bridge blocks only on lock conflicts', async () => {
  await withAgentId('pi-test-agent', async () => {
    const bridge = createAwarenessBridge({
      baseDir: createAwarenessFixture(),
      runCommand: async () => ({ status: 2, stdout: '{"ok":false}', stderr: 'locked by another agent' }),
    });

    const result = await bridge.handleToolCall(
      { toolName: 'edit', toolCallId: 'tool-2', input: { path: 'src/a.js' } },
      { cwd: '/repo' }
    );

    assert.deepEqual(result, {
      block: true,
      reason: 'Octocode awareness blocked this edit:\nlocked by another agent\n{"ok":false}',
    });
    assert.equal(bridge.pendingToolFiles.has('tool-2'), false);
  });
});

test('awareness bridge fails open on non-conflict errors', async () => {
  await withAgentId('pi-test-agent', async () => {
    const messages = [];
    const bridge = createAwarenessBridge({
      baseDir: createAwarenessFixture(),
      runCommand: async () => ({ status: 1, stdout: '', stderr: 'python unavailable' }),
    });

    const result = await bridge.handleToolCall(
      { toolName: 'write', toolCallId: 'tool-3', input: { path: 'src/a.js' } },
      { cwd: '/repo', ui: { notify: (message, level) => messages.push({ level, message }) } }
    );

    assert.equal(result, undefined);
    assert.equal(bridge.pendingToolFiles.has('tool-3'), false);
    assert.deepEqual(messages, [
      {
        level: 'warning',
        message: 'Octocode awareness warning; continuing: python unavailable',
      },
    ]);
  });
});
