import { afterAll, beforeAll, describe, expect, it } from 'vitest';
import { mkdirSync, mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { runHookCommand } from '../src/hooks/runner.js';
import { connectDb, resolveDbPath } from '../src/db-runtime.js';
import { DEFAULT_AWARENESS_CONFIG, writeAwarenessConfig } from '../src/awareness-config.js';
import { writeWorkspacePolicy } from '../src/workspace-policy.js';

let configHome = '';
const previousOctocodeHome = process.env.OCTOCODE_HOME;
beforeAll(() => {
  configHome = mkdtempSync(join(tmpdir(), 'awareness-generic-hook-config-'));
  process.env.OCTOCODE_HOME = configHome;
  writeAwarenessConfig(DEFAULT_AWARENESS_CONFIG, { path: join(configHome, 'awareness.json') });
});
afterAll(() => {
  if (previousOctocodeHome === undefined) delete process.env.OCTOCODE_HOME;
  else process.env.OCTOCODE_HOME = previousOctocodeHome;
  rmSync(configHome, { recursive: true, force: true });
});

describe('generic hook lifecycle', () => {
  it('keeps reads out of write presence and delivery out of settlement', async () => {
    const memoryDir = mkdtempSync(join(tmpdir(), 'awareness-generic-hooks-'));
    const workspace = join(memoryDir, 'workspace');
    mkdirSync(workspace, { recursive: true });
    writeWorkspacePolicy(workspace, { version: 1, storage: { repository: 'repo', memory: 'repo' }, hooks: { profile: 'full' } });
    const priorMemoryHome = process.env.OCTOCODE_AGENT_DIR;
    const priorAgentId = process.env.OCTOCODE_AGENT_ID;
    process.env.OCTOCODE_AGENT_DIR = memoryDir;
    process.env.OCTOCODE_AGENT_ID = 'generic-hook-agent';
    try {
      const read = { cwd: workspace, session_id: 'generic-session', hook_event_name: 'PreToolUse', tool_name: 'Read', tool_use_id: 'read-1', tool_input: { file_path: 'src/a.ts' } };
      expect(await runHookCommand('pre-edit', JSON.stringify(read), { host: 'codex' })).toBe(0);
      expect(await runHookCommand('post-edit', JSON.stringify({ ...read, hook_event_name: 'PostToolUse' }), { host: 'codex' })).toBe(0);
      const database = connectDb(resolveDbPath(null, { workspace, scope: 'repo' }));
      expect(database.prepare('SELECT COUNT(*) AS count FROM task_runs').get()).toEqual({ count: 0 });
      expect(database.prepare('SELECT COUNT(*) AS count FROM run_files').get()).toEqual({ count: 0 });

      const write = { ...read, hook_event_name: 'PreToolUse', tool_name: 'Write', tool_use_id: 'write-1', tool_input: { file_path: 'src/a.ts' } };
      expect(await runHookCommand('pre-edit', JSON.stringify(write), { host: 'codex' })).toBe(0);
      expect(await runHookCommand('post-edit', JSON.stringify({ ...write, hook_event_name: 'PostToolUse' }), { host: 'codex' })).toBe(0);
      expect(await runHookCommand('notify-deliver', JSON.stringify({ cwd: workspace, session_id: 'generic-session', hook_event_name: 'UserPromptSubmit' }), { host: 'codex' })).toBe(0);
      expect(database.prepare("SELECT COUNT(*) AS count FROM task_runs WHERE origin = 'HOOK' AND status = 'ACTIVE'").get()).toEqual({ count: 1 });
      database.close();
    } finally {
      if (priorMemoryHome === undefined) delete process.env.OCTOCODE_AGENT_DIR;
      else process.env.OCTOCODE_AGENT_DIR = priorMemoryHome;
      if (priorAgentId === undefined) delete process.env.OCTOCODE_AGENT_ID;
      else process.env.OCTOCODE_AGENT_ID = priorAgentId;
      rmSync(memoryDir, { recursive: true, force: true });
    }
  });

  it('does not record nonterminal or partial write results as successful edits', async () => {
    const memoryDir = mkdtempSync(join(tmpdir(), 'awareness-running-hook-'));
    const workspace = join(memoryDir, 'workspace');
    mkdirSync(workspace, { recursive: true });
    writeWorkspacePolicy(workspace, { version: 1, storage: { repository: 'repo', memory: 'repo' }, hooks: { profile: 'full' } });
    const priorMemoryHome = process.env.OCTOCODE_AGENT_DIR;
    const priorAgentId = process.env.OCTOCODE_AGENT_ID;
    process.env.OCTOCODE_AGENT_DIR = memoryDir;
    process.env.OCTOCODE_AGENT_ID = 'running-hook-agent';
    try {
      const write = { cwd: workspace, session_id: 'running-session', hook_event_name: 'PreToolUse', tool_name: 'Write', tool_use_id: 'write-1', tool_input: { file_path: 'src/a.ts' } };
      await runHookCommand('pre-edit', JSON.stringify(write), { host: 'codex' });
      await runHookCommand('post-edit', JSON.stringify({ ...write, hook_event_name: 'PostToolUse', tool_response: { status: 'running' } }), { host: 'codex' });
      const database = connectDb(resolveDbPath(null, { workspace, scope: 'repo' }));
      expect(database.prepare("SELECT COUNT(*) AS count FROM event_outbox WHERE event_type LIKE 'edit.%'").get()).toEqual({ count: 0 });
      expect(database.prepare("SELECT COUNT(*) AS count FROM task_runs WHERE status = 'ACTIVE'").get()).toEqual({ count: 1 });
      await runHookCommand('post-edit', JSON.stringify({ ...write, hook_event_name: 'PostToolUse', tool_response: { status: 'partial' } }), { host: 'codex' });
      expect(database.prepare("SELECT COUNT(*) AS count FROM event_outbox WHERE event_type LIKE 'edit.%'").get()).toEqual({ count: 0 });
      expect(database.prepare("SELECT COUNT(*) AS count FROM task_runs WHERE status = 'ACTIVE'").get()).toEqual({ count: 0 });
      database.close();
    } finally {
      if (priorMemoryHome === undefined) delete process.env.OCTOCODE_AGENT_DIR;
      else process.env.OCTOCODE_AGENT_DIR = priorMemoryHome;
      if (priorAgentId === undefined) delete process.env.OCTOCODE_AGENT_ID;
      else process.env.OCTOCODE_AGENT_ID = priorAgentId;
      rmSync(memoryDir, { recursive: true, force: true });
    }
  });
});
