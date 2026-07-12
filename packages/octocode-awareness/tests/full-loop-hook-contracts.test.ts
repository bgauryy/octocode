import { describe, expect, it } from 'vitest';
import { spawn } from 'node:child_process';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import {
  hookBlockOutcome,
  hookContextEnvelope,
  runHookCommand,
} from '../bin/hook-runner.js';
import { agentId } from '../bin/hook-payload.js';
import { connectDb, resolveDbPath } from '../src/db.js';
import { runHooksInstall } from '../src/hooks-install.js';
import { wirePiAwarenessHooks } from '../src/pi-hooks.js';
import { auditUnverified, markVerified } from '../src/verify.js';

function runPreEditChild(payload: Record<string, unknown>, env: NodeJS.ProcessEnv): Promise<void> {
  const hookRunnerUrl = new URL('../bin/hook-runner.ts', import.meta.url).href;
  const source = `import { runHookCommand } from ${JSON.stringify(hookRunnerUrl)}; process.exitCode = await runHookCommand('pre-edit', process.env.HOOK_PAYLOAD, { host: 'claude' });`;
  return new Promise((resolvePromise, reject) => {
    const child = spawn(process.execPath, ['--import', 'tsx', '--input-type=module', '-e', source], {
      cwd: process.cwd(),
      env: { ...process.env, ...env, HOOK_PAYLOAD: JSON.stringify(payload) },
      stdio: ['ignore', 'ignore', 'pipe'],
    });
    let stderr = '';
    child.stderr.setEncoding('utf8');
    child.stderr.on('data', chunk => { stderr += chunk; });
    child.on('error', reject);
    child.on('exit', code => {
      if (code === 0) resolvePromise();
      else reject(new Error(`pre-edit child exited ${code}: ${stderr}`));
    });
  });
}

describe('full-loop host hook contracts', () => {
  it('uses the host subagent identity instead of collapsing it into the parent agent', () => {
    const previous = process.env.OCTOCODE_AGENT_ID;
    process.env.OCTOCODE_AGENT_ID = 'parent-agent';
    try {
      expect(agentId({
        hook_event_name: 'SubagentStart',
        agent_id: 'child-agent',
        agent_type: 'Explore',
      })).toBe('child-agent');
    } finally {
      if (previous === undefined) delete process.env.OCTOCODE_AGENT_ID;
      else process.env.OCTOCODE_AGENT_ID = previous;
    }
  });

  it('uses event-specific model-context envelopes for Claude and Codex', () => {
    for (const host of ['claude', 'codex'] as const) {
      expect(hookContextEnvelope(host, 'PreToolUse', 'peer changed')).toEqual({
        hookSpecificOutput: {
          hookEventName: 'PreToolUse',
          additionalContext: 'peer changed',
        },
      });
      expect(hookContextEnvelope(host, 'UserPromptSubmit', 'briefing changed')).toEqual({
        hookSpecificOutput: {
          hookEventName: 'UserPromptSubmit',
          additionalContext: 'briefing changed',
        },
      });
      expect(hookBlockOutcome(host, 'pre-edit', 'exclusive conflict')).toEqual({
        exitCode: 2,
        stderr: 'exclusive conflict',
      });
    }
  });

  it('uses Cursor-native permission, context, and follow-up responses', () => {
    expect(hookContextEnvelope('cursor', 'preToolUse', 'peer changed')).toEqual({
      permission: 'allow',
      agent_message: 'peer changed',
    });
    expect(hookContextEnvelope('cursor', 'sessionStart', 'briefing changed')).toEqual({
      additional_context: 'briefing changed',
    });
    expect(hookBlockOutcome('cursor', 'pre-edit', 'exclusive conflict')).toEqual({
      exitCode: 0,
      payload: {
        permission: 'deny',
        user_message: 'exclusive conflict',
        agent_message: 'exclusive conflict',
      },
    });
    expect(hookBlockOutcome('cursor', 'stop', 'verification debt')).toEqual({
      exitCode: 0,
      payload: { followup_message: 'verification debt' },
    });
  });

  it('quotes hook paths, adds a Codex Windows command, and reports config-only health', () => {
    const projectDir = mkdtempSync(join(tmpdir(), 'awareness hook contract '));
    const hookDir = resolve(projectDir, 'skill with spaces/scripts/hooks');
    mkdirSync(hookDir, { recursive: true });
    for (const script of [
      'pre-edit.sh',
      'post-edit.sh',
      'stop-verify.sh',
      'session-compact.sh',
      'notify-deliver.sh',
    ]) {
      writeFileSync(resolve(hookDir, script), '#!/bin/sh\n');
    }
    writeFileSync(resolve(hookDir, '..', 'hook-runner.mjs'), '#!/usr/bin/env node\n');
    try {
      const installed = runHooksInstall(
        ['--host', 'codex', '--project-dir', projectDir],
        { cwd: projectDir, hookDir },
      );
      expect(installed.exitCode).toBe(0);
      const settingsPath = resolve(projectDir, '.codex/hooks.json');
      const settings = JSON.parse(readFileSync(settingsPath, 'utf8')) as {
        hooks: Record<string, Array<{ hooks?: Array<Record<string, unknown>> }>>;
      };
      const commandHook = settings.hooks.PreToolUse?.[0]?.hooks?.[0];
      expect(commandHook?.command).toContain(`"${process.execPath}"`);
      expect(commandHook?.command).toContain('skill with spaces/scripts/hook-runner.mjs" pre-edit --host codex --skill-root');
      expect(commandHook?.commandWindows).toContain('hook-runner.mjs');
      expect(commandHook?.commandWindows).toContain('--host codex');
      expect(commandHook?.commandWindows).toContain('--skill-root');

      const checked = runHooksInstall(
        ['--host', 'codex', '--project-dir', projectDir, '--check', '--strict'],
        { cwd: projectDir, hookDir },
      );
      expect(checked.exitCode).toBe(0);
      expect(checked.payload).toMatchObject({
        ok: true,
        strict_scope: 'config_only',
        health: {
          config: { status: 'ready', verified: true },
          runtime: {
            status: 'unverified',
            verified: false,
            project_trust: 'not_checked',
            hook_definition_trust: 'not_checked',
            hooks_feature_enabled: 'not_checked',
          },
        },
      });
    } finally {
      rmSync(projectDir, { recursive: true, force: true });
    }
  });

  it('keeps Pi lifecycle context and continuation in process', () => {
    const events: string[] = [];
    const bridge = wirePiAwarenessHooks({
      on: (eventName: string) => { events.push(eventName); },
    });
    expect(bridge).toBeTruthy();
    expect(events).toEqual(expect.arrayContaining([
      'tool_call',
      'tool_result',
      'before_agent_start',
      'agent_end',
      'session_before_compact',
      'session_shutdown',
    ]));
  });

  it('aggregates one session turn into one pending fallback HOOK run', async () => {
    const memoryDir = mkdtempSync(join(tmpdir(), 'awareness hook aggregation '));
    const workspace = join(memoryDir, 'workspace');
    mkdirSync(workspace, { recursive: true });
    const priorMemoryHome = process.env.OCTOCODE_MEMORY_HOME;
    const priorAgentId = process.env.OCTOCODE_AGENT_ID;
    process.env.OCTOCODE_MEMORY_HOME = memoryDir;
    process.env.OCTOCODE_AGENT_ID = 'hook-contract-agent';
    try {
      for (let index = 0; index < 5; index += 1) {
        const payload = JSON.stringify({
          cwd: workspace,
          session_id: 'hook-contract-session',
          tool_name: 'Write',
          tool_use_id: `edit-${index}`,
          tool_input: { path: `src/file-${index}.ts` },
        });
        expect(await runHookCommand('pre-edit', payload, { host: 'claude' })).toBe(0);
        expect(await runHookCommand('post-edit', payload, { host: 'claude' })).toBe(0);
      }

      const database = connectDb(resolveDbPath(null));
      expect(database.prepare("SELECT COUNT(*) AS count FROM task_runs WHERE origin = 'HOOK' AND status = 'ACTIVE'").get()).toEqual({ count: 1 });
      expect(database.prepare('SELECT COUNT(*) AS count FROM run_files WHERE ended_at IS NULL').get()).toEqual({ count: 5 });
      const activePlan = database.prepare("SELECT test_plan FROM task_runs WHERE origin = 'HOOK' AND status = 'ACTIVE'").get() as { test_plan: string };
      expect(activePlan.test_plan).toContain('smallest relevant test/typecheck');
      expect(activePlan.test_plan).toContain('+2 more');

      const stopPayload = JSON.stringify({ cwd: workspace, session_id: 'hook-contract-session' });
      expect(await runHookCommand('stop-verify', stopPayload, { host: 'claude' })).toBe(2);
      const pending = database.prepare("SELECT run_id FROM task_runs WHERE origin = 'HOOK' AND status = 'PENDING'").get() as { run_id: string };
      expect(pending.run_id).toMatch(/^run_/);
      expect(auditUnverified(database, { agentId: 'hook-contract-agent', workspacePath: workspace }).count).toBe(1);

      expect(markVerified(database, {
        agentId: 'hook-contract-agent',
        runId: pending.run_id,
        status: 'SUCCESS',
        message: '5 edits aggregated; contract test passed',
      }).ok).toBe(true);
      expect(auditUnverified(database, { agentId: 'hook-contract-agent', workspacePath: workspace }).count).toBe(0);

      for (let index = 0; index < 2; index += 1) {
        const payload = JSON.stringify({
          cwd: workspace,
          session_id: 'hook-session-end-contract',
          tool_name: 'Edit',
          tool_use_id: `session-end-edit-${index}`,
          tool_input: { path: `src/session-end-${index}.ts` },
        });
        expect(await runHookCommand('pre-edit', payload, { host: 'codex' })).toBe(0);
        expect(await runHookCommand('post-edit', payload, { host: 'codex' })).toBe(0);
      }
      expect(database.prepare("SELECT COUNT(*) AS count FROM task_runs WHERE origin = 'HOOK' AND status = 'ACTIVE'").get()).toEqual({ count: 1 });
      expect(await runHookCommand('session-end', JSON.stringify({
        cwd: workspace,
        session_id: 'hook-session-end-contract',
        reason: 'complete',
      }), { host: 'codex' })).toBe(0);
      const sessionEndPending = database.prepare("SELECT run_id FROM task_runs WHERE origin = 'HOOK' AND status = 'PENDING'").get() as { run_id: string };
      expect(sessionEndPending.run_id).toMatch(/^run_/);
      expect(database.prepare('SELECT COUNT(*) AS count FROM run_files WHERE run_id = ?').get(sessionEndPending.run_id)).toEqual({ count: 2 });
      expect(markVerified(database, {
        agentId: 'hook-contract-agent',
        runId: sessionEndPending.run_id,
        status: 'SUCCESS',
        message: 'session-end finalized aggregate',
      }).ok).toBe(true);
      expect(auditUnverified(database, { agentId: 'hook-contract-agent', workspacePath: workspace }).count).toBe(0);

      const compactEdit = JSON.stringify({
        cwd: workspace,
        session_id: 'hook-compact-contract',
        tool_name: 'Write',
        tool_use_id: 'compact-edit',
        tool_input: { path: 'src/compact.ts' },
      });
      expect(await runHookCommand('pre-edit', compactEdit, { host: 'codex' })).toBe(0);
      expect(await runHookCommand('post-edit', compactEdit, { host: 'codex' })).toBe(0);
      expect(await runHookCommand('session-compact', JSON.stringify({
        cwd: workspace,
        session_id: 'hook-compact-contract',
        reason: 'compact:auto',
      }), { host: 'codex' })).toBe(0);
      expect(database.prepare("SELECT ended_at FROM sessions WHERE session_id = 'hook-compact-contract'").get()).toEqual({ ended_at: null });

      const compactPending = database.prepare("SELECT run_id FROM task_runs WHERE origin = 'HOOK' AND status = 'PENDING' ORDER BY created_at DESC LIMIT 1").get() as { run_id: string };
      expect(markVerified(database, {
        agentId: 'hook-contract-agent',
        runId: compactPending.run_id,
        status: 'SUCCESS',
        message: 'pre-compact finalized aggregate without ending session',
      }).ok).toBe(true);
      expect(await runHookCommand('pre-edit', JSON.stringify({
        cwd: workspace,
        session_id: 'hook-compact-contract',
        tool_name: 'Write',
        tool_use_id: 'after-compact-edit',
        tool_input: { path: 'src/after-compact.ts' },
      }), { host: 'codex' })).toBe(0);
      expect(database.prepare("SELECT COUNT(*) AS count FROM task_runs WHERE origin = 'HOOK' AND status = 'ACTIVE'").get()).toEqual({ count: 1 });
      database.close();
    } finally {
      if (priorMemoryHome === undefined) delete process.env.OCTOCODE_MEMORY_HOME;
      else process.env.OCTOCODE_MEMORY_HOME = priorMemoryHome;
      if (priorAgentId === undefined) delete process.env.OCTOCODE_AGENT_ID;
      else process.env.OCTOCODE_AGENT_ID = priorAgentId;
      rmSync(memoryDir, { recursive: true, force: true });
    }
  });

  it('discards a failed shell write instead of creating verification debt', async () => {
    const memoryDir = mkdtempSync(join(tmpdir(), 'awareness failed hook '));
    const workspace = join(memoryDir, 'workspace');
    mkdirSync(workspace, { recursive: true });
    const priorMemoryHome = process.env.OCTOCODE_MEMORY_HOME;
    const priorAgentId = process.env.OCTOCODE_AGENT_ID;
    process.env.OCTOCODE_MEMORY_HOME = memoryDir;
    process.env.OCTOCODE_AGENT_ID = 'hook-failure-agent';
    try {
      const payload = {
        cwd: workspace,
        session_id: 'hook-failure-session',
        tool_name: 'Write',
        tool_use_id: 'failed-edit',
        tool_input: { path: 'src/failed.ts' },
      };
      expect(await runHookCommand('pre-edit', JSON.stringify(payload), { host: 'claude' })).toBe(0);
      expect(await runHookCommand('post-edit', JSON.stringify({
        ...payload,
        hook_event_name: 'PostToolUseFailure',
        is_error: true,
      }), { host: 'claude' })).toBe(0);

      const database = connectDb(resolveDbPath(null));
      expect(database.prepare('SELECT COUNT(*) AS count FROM edit_log').get()).toEqual({ count: 0 });
      expect(database.prepare('SELECT COUNT(*) AS count FROM run_files').get()).toEqual({ count: 0 });
      expect(database.prepare('SELECT COUNT(*) AS count FROM task_runs').get()).toEqual({ count: 0 });
      expect(auditUnverified(database, { agentId: 'hook-failure-agent', workspacePath: workspace }).count).toBe(0);
      database.close();
    } finally {
      if (priorMemoryHome === undefined) delete process.env.OCTOCODE_MEMORY_HOME;
      else process.env.OCTOCODE_MEMORY_HOME = priorMemoryHome;
      if (priorAgentId === undefined) delete process.env.OCTOCODE_AGENT_ID;
      else process.env.OCTOCODE_AGENT_ID = priorAgentId;
      rmSync(memoryDir, { recursive: true, force: true });
    }
  });

  it('coalesces concurrent shell pre-edit processes into one aggregate', async () => {
    const memoryDir = mkdtempSync(join(tmpdir(), 'awareness hook race '));
    const workspace = join(memoryDir, 'workspace');
    mkdirSync(workspace, { recursive: true });
    const priorMemoryHome = process.env.OCTOCODE_MEMORY_HOME;
    const priorAgentId = process.env.OCTOCODE_AGENT_ID;
    process.env.OCTOCODE_MEMORY_HOME = memoryDir;
    process.env.OCTOCODE_AGENT_ID = 'hook-race-agent';
    const basePayload = { cwd: workspace, session_id: 'hook-race-session', tool_name: 'Write' };
    try {
      await Promise.all([
        runPreEditChild({ ...basePayload, tool_use_id: 'race-a', tool_input: { path: 'src/a.ts' } }, {
          OCTOCODE_MEMORY_HOME: memoryDir,
          OCTOCODE_AGENT_ID: 'hook-race-agent',
        }),
        runPreEditChild({ ...basePayload, tool_use_id: 'race-b', tool_input: { path: 'src/b.ts' } }, {
          OCTOCODE_MEMORY_HOME: memoryDir,
          OCTOCODE_AGENT_ID: 'hook-race-agent',
        }),
      ]);

      const database = connectDb(resolveDbPath(null));
      expect(database.prepare("SELECT COUNT(*) AS count FROM task_runs WHERE origin = 'HOOK' AND status = 'ACTIVE'").get()).toEqual({ count: 1 });
      expect(database.prepare('SELECT COUNT(*) AS count FROM run_files WHERE ended_at IS NULL').get()).toEqual({ count: 2 });
      for (const [toolUseId, file] of [['race-a', 'src/a.ts'], ['race-b', 'src/b.ts']] as const) {
        expect(await runHookCommand('post-edit', JSON.stringify({
          ...basePayload,
          tool_use_id: toolUseId,
          tool_input: { path: file },
        }), { host: 'claude' })).toBe(0);
      }
      expect(await runHookCommand('stop-verify', JSON.stringify(basePayload), { host: 'claude' })).toBe(2);
      expect(database.prepare("SELECT COUNT(*) AS count FROM task_runs WHERE origin = 'HOOK' AND status = 'PENDING'").get()).toEqual({ count: 1 });
      database.close();
    } finally {
      if (priorMemoryHome === undefined) delete process.env.OCTOCODE_MEMORY_HOME;
      else process.env.OCTOCODE_MEMORY_HOME = priorMemoryHome;
      if (priorAgentId === undefined) delete process.env.OCTOCODE_AGENT_ID;
      else process.env.OCTOCODE_AGENT_ID = priorAgentId;
      rmSync(memoryDir, { recursive: true, force: true });
    }
  });

  it('re-audits recursive Stop only when continuation edits add new debt', async () => {
    const memoryDir = mkdtempSync(join(tmpdir(), 'awareness recursive stop '));
    const workspace = join(memoryDir, 'workspace');
    mkdirSync(workspace, { recursive: true });
    const priorMemoryHome = process.env.OCTOCODE_MEMORY_HOME;
    const priorAgentId = process.env.OCTOCODE_AGENT_ID;
    process.env.OCTOCODE_MEMORY_HOME = memoryDir;
    process.env.OCTOCODE_AGENT_ID = 'hook-recursive-agent';
    const stopPayload = { cwd: workspace, session_id: 'recursive-session' };
    const edit = async (id: string, file: string) => {
      const payload = JSON.stringify({ ...stopPayload, tool_name: 'Edit', tool_use_id: id, tool_input: { path: file } });
      expect(await runHookCommand('pre-edit', payload, { host: 'claude' })).toBe(0);
      expect(await runHookCommand('post-edit', payload, { host: 'claude' })).toBe(0);
    };
    try {
      await edit('initial', 'src/initial.ts');
      expect(await runHookCommand('stop-verify', JSON.stringify(stopPayload), { host: 'claude' })).toBe(2);
      await edit('continuation', 'src/continuation.ts');
      expect(await runHookCommand('stop-verify', JSON.stringify({ ...stopPayload, stop_hook_active: true }), { host: 'claude' })).toBe(2);
      const database = connectDb(resolveDbPath(null));
      expect(database.prepare("SELECT COUNT(*) AS count FROM task_runs WHERE origin = 'HOOK' AND status = 'PENDING'").get()).toEqual({ count: 2 });
      expect(await runHookCommand('stop-verify', JSON.stringify({ ...stopPayload, stop_hook_active: true }), { host: 'claude' })).toBe(0);
      expect(auditUnverified(database, { agentId: 'hook-recursive-agent', workspacePath: workspace }).count).toBe(2);
      database.close();
    } finally {
      if (priorMemoryHome === undefined) delete process.env.OCTOCODE_MEMORY_HOME;
      else process.env.OCTOCODE_MEMORY_HOME = priorMemoryHome;
      if (priorAgentId === undefined) delete process.env.OCTOCODE_AGENT_ID;
      else process.env.OCTOCODE_AGENT_ID = priorAgentId;
      rmSync(memoryDir, { recursive: true, force: true });
    }
  });
});
