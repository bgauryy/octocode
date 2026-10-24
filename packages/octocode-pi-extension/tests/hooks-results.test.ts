import fs from 'node:fs';
import path from 'node:path';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { FileGuard } from '../src/files/guard.js';
import { registerFileTool } from '../src/files/tool.js';
import { registerHooks } from '../src/hooks/register.js';
import { fingerprint, projectConfigFiles, saveDecision } from '../src/shared/trust.js';
import { fakeCtx, fakePermission, fakePi } from './fake-pi.js';
import { repo, tmp } from './helpers.js';

beforeEach(() => vi.stubEnv('OCTOCODE_HOME', tmp()));
afterEach(() => vi.unstubAllEnvs());

async function setup() {
  const cwd = fs.realpathSync(repo());
  fs.mkdirSync(path.join(cwd, '.codex'));
  fs.writeFileSync(path.join(cwd, '.codex/hooks.json'), JSON.stringify({ hooks: { PostToolUse: [{ matcher: 'Write', hooks: [{ type: 'command', command: 'node post.cjs' }] }] } }));
  fs.writeFileSync(path.join(cwd, 'post.cjs'), "let input=''; process.stdin.on('data', chunk => input += chunk).on('end', () => require('node:fs').appendFileSync(JSON.parse(input).tool_input.file_path, '[hook]'));\n");
  saveDecision(cwd, fingerprint(projectConfigFiles(cwd)), true);
  const fake = fakePi();
  registerHooks(fake.pi, { OCTOCODE_HOOKS: '1' }, tmp());
  registerFileTool(fake.pi, new FileGuard(), fakePermission('manual'));
  const ctx = fakeCtx({ cwd, ui: { selects: ['Review one by one', 'Apply', 'Reject'] } });
  await fake.fire('session_start', { reason: 'startup' }, ctx);
  return { ...fake, cwd, ctx, tool: fake.tools.get('file') };
}

describe('PostToolUse success boundary', () => {
  it('does not run success hooks for a failed or denied tool result', async () => {
    const { fire, cwd, ctx } = await setup();
    await fire('tool_result', { toolName: 'file', toolCallId: 'denied', input: { queries: [{ type: 'write', path: 'denied.txt', content: 'no', reasoning: 'test' }] }, content: [{ type: 'text', text: 'The user denied the change.' }], isError: true }, ctx);
    expect(fs.existsSync(path.join(cwd, 'denied.txt'))).toBe(false);
  });

  it('runs a success hook only for the applied queries in a partially rejected batch', async () => {
    const { tool, fire, cwd, ctx } = await setup();
    const input = { queries: ['approved.txt', 'rejected.txt'].map((file) => ({ type: 'write', path: file, content: 'authored', reasoning: 'test' })) };
    const result = await tool.execute('partial', input, undefined, undefined, ctx);
    await fire('tool_result', { toolName: 'file', toolCallId: 'partial', input, ...result, isError: false }, ctx);
    expect(fs.readFileSync(path.join(cwd, 'approved.txt'), 'utf8')).toBe('authored[hook]');
    expect(fs.existsSync(path.join(cwd, 'rejected.txt'))).toBe(false);
  });
});
