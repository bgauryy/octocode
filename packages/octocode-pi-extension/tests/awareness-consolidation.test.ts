import { test, expect } from 'vitest';
import { existsSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { resolve } from 'node:path';
import { getAwarenessCLIPath } from '../src/assets.js';

// Regression contract for the CLI+skill consolidation: awareness memory/
// coordination must NOT be re-exposed as agent tools. The agent drives it via
// the octocode-awareness CLI ($OCTOCODE_AWARENESS_CLI) with the lifecycle
// automated by the awareness hooks; only the digest/forget user commands remain.
test('awareness consolidation: no memory tools; hooks, commands, and env intact', async () => {
  const previousAgentId = process.env.OCTOCODE_AGENT_ID;
  delete process.env.OCTOCODE_AGENT_ID;
  const tools = new Map<string, unknown>(), commands = new Map<string, unknown>(), handlers = new Map<string, unknown[]>();
  const pi = {
    registerTool: (d: { name: string }) => tools.set(d.name, d),
    registerCommand: (n: string, c: unknown) => commands.set(n, c),
    registerFlag: () => {}, getFlag: () => undefined,
    sendUserMessage: () => {}, sendMessage: () => {},
    getActiveTools: () => ['bash', 'edit', 'write'], setActiveTools: () => {},
    getThinkingLevel: () => 'medium',
    on: (e: string, h: unknown) => { const a = handlers.get(e) ?? []; a.push(h); handlers.set(e, a); },
  };
  const ext = ((await import('../src/index.js')) as { default: (pi: unknown) => Promise<void> }).default;
  await ext(pi);
  const sessionHandlers = (handlers.get('session_start') ?? []) as Array<(e: unknown, c: unknown) => unknown>;
  const shutdownHandlers = (handlers.get('session_shutdown') ?? []) as Array<(e: unknown, c: unknown) => unknown>;
  const context = (session: string) => ({
    cwd: process.cwd(), hasUI: false,
    sessionManager: { getSessionFile: () => `/tmp/${session}.jsonl` },
  });
  try {
    for (const h of sessionHandlers) await h({}, context('session-one'));
    expect(process.env.OCTOCODE_AGENT_ID).toBe('pi:session-one');
    for (const h of sessionHandlers) await h({}, context('session-two'));
    expect(process.env.OCTOCODE_AGENT_ID).toBe('pi:session-two');

  const REMOVED = ['memory_recall', 'memory_record', 'memory_reflect', 'workspace_status', 'agent_signal', 'file_lock', 'memory_refine_get', 'memory_audit_unverified', 'memory_verify', 'memory_export_harness'];
  expect(REMOVED.filter(n => tools.has(n)), 'no awareness memory tools registered').toEqual([]);
  for (const e of ['tool_call', 'tool_result', 'before_agent_start', 'agent_end']) {
    expect(handlers.has(e), `awareness hook ${e} wired`).toBe(true);
  }
  expect(commands.has('octocode-memory-digest'), 'digest user command kept').toBe(true);
  expect(commands.has('octocode-memory-forget'), 'forget user command kept').toBe(true);
    expect(process.env.OCTOCODE_AGENT_ID, 'agent id pinned for CLI inheritance').toBeTruthy();
    expect(process.env.OCTOCODE_AWARENESS_CLI, 'awareness CLI env exported').toBeTruthy();
    const cli = getAwarenessCLIPath(resolve(import.meta.dirname, '../dist'));
    expect(cli.endsWith('skills/octocode-awareness/scripts/awareness.mjs'), 'CLI path points at bundled skill script').toBe(true);
    expect(existsSync(cli), 'bundled Awareness CLI exists').toBe(true);
    const smoke = spawnSync(process.execPath, [cli, 'schema', 'commands', '--compact'], {
      encoding: 'utf8', timeout: 5000,
    });
    expect(smoke.status, smoke.stderr || smoke.stdout).toBe(0);
    expect(JSON.parse(smoke.stdout).commands.length).toBeGreaterThan(10);
    for (const h of shutdownHandlers) await h({}, context('session-two'));
    expect(process.env.OCTOCODE_AGENT_ID).toBeUndefined();
  } finally {
    if (previousAgentId == null) delete process.env.OCTOCODE_AGENT_ID;
    else process.env.OCTOCODE_AGENT_ID = previousAgentId;
  }
});
