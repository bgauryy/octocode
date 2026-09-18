import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { agentHost, agentId, agentVendor, INTERNAL_HOOK_HOST } from '../src/hooks/payload.js';
import { registerHookAgent } from '../src/hooks/peers.js';
import { runHookCommand } from '../src/hooks/runner.js';
import { connectDb } from '../src/db-runtime.js';
import { listAgents, registerAgent } from '../src/agents.js';
import { DEFAULT_AWARENESS_CONFIG, writeAwarenessConfig } from '../src/awareness-config.js';

let directory: string;
let database: ReturnType<typeof connectDb>;
beforeEach(() => {
  directory = mkdtempSync(join(tmpdir(), 'awareness-hook-identity-'));
  database = connectDb(join(directory, 'awareness.sqlite3'));
  vi.stubEnv('OCTOCODE_HOME', directory);
  vi.stubEnv('OCTOCODE_HOOK_PROFILE', 'full');
  for (const key of ['OCTOCODE_AGENT_ID', 'OCTOCODE_AGENT_NAME', 'OCTOCODE_AGENT_VENDOR', 'OCTOCODE_AGENT_HOST']) vi.stubEnv(key, undefined);
});
afterEach(() => {
  database.close();
  vi.unstubAllEnvs();
  rmSync(directory, { recursive: true, force: true });
});

describe('hook identity labels in the shared registry', () => {
  it('uses the same configured routing ID and labels as direct CLI registration', () => {
    vi.stubEnv('OCTOCODE_AGENT_ID', 'codex:session');
    vi.stubEnv('OCTOCODE_AGENT_NAME', 'Review lead');
    vi.stubEnv('OCTOCODE_AGENT_VENDOR', 'openai');
    vi.stubEnv('OCTOCODE_AGENT_HOST', 'codex');
    registerAgent(database, { agentId: 'codex:session', agentName: 'Review lead', agentVendor: 'openai', agentHost: 'codex', workspacePath: directory });
    const payload = { cwd: directory, session_id: 'host-session' };
    expect(agentId(payload)).toBe('codex:session');
    registerHookAgent(database, payload, 'notify-deliver');
    const rows = listAgents(database, { workspacePath: directory });
    expect(rows.agents).toHaveLength(1);
    expect(rows.agents[0]).toMatchObject({ agent_id: 'codex:session', agent_name: 'Review lead', agent_vendor: 'openai', agent_host: 'codex' });
  });

  it('keeps explicit child identity and provider instead of inherited parent labels', () => {
    vi.stubEnv('OCTOCODE_AGENT_ID', 'parent');
    vi.stubEnv('OCTOCODE_AGENT_NAME', 'Parent name');
    vi.stubEnv('OCTOCODE_AGENT_VENDOR', 'openai');
    vi.stubEnv('OCTOCODE_AGENT_HOST', 'codex');
    const payload = { agent_id: 'child', agent_name: 'Test worker', agent_vendor: 'anthropic', [INTERNAL_HOOK_HOST]: 'opencode', cwd: directory };
    registerHookAgent(database, payload, 'pre-edit');
    expect(listAgents(database, { workspacePath: directory }).agents[0]).toMatchObject({ agent_id: 'child', agent_name: 'Test worker', agent_vendor: 'anthropic', agent_host: 'opencode' });
  });

  it('does not turn host-protocol heuristics, names or opaque IDs into vendor facts', () => {
    const payload = { agent_id: 'openai:claude', agent_name: 'Gemini agent', hook_event_name: 'PreToolUse', cwd: directory };
    expect(agentVendor(payload)).toBeNull();
    expect(agentHost(payload)).toBeNull();
    registerHookAgent(database, payload, 'pre-edit');
    expect(listAgents(database, { workspacePath: directory }).agents[0]).toMatchObject({ agent_vendor: null, agent_host: null });
  });

  it('preserves known registry labels on later events that omit identity metadata', () => {
    registerAgent(database, { agentId: 'session', agentName: 'Reviewer', agentVendor: 'google', agentHost: 'gemini', workspacePath: directory });
    registerHookAgent(database, { agent_id: 'session', cwd: directory }, 'post-edit');
    expect(listAgents(database, { workspacePath: directory }).agents[0]).toMatchObject({ agent_name: 'Reviewer', agent_vendor: 'google', agent_host: 'gemini' });
  });

  it('accepts explicit camelCase labels and model provider with whitespace normalized', () => {
    expect(agentVendor({ agentVendor: ' custom ' })).toBe('custom');
    expect(agentVendor({ model: { provider: ' anthropic ' } })).toBe('anthropic');
    expect(agentHost({ agentHost: ' custom-host ' })).toBe('custom-host');
  });

  it('rejects two anonymous participants instead of merging them into one host/workspace ID', async () => {
    writeAwarenessConfig(DEFAULT_AWARENESS_CONFIG, { path: join(directory, 'awareness.json') });
    const error = vi.spyOn(process.stderr, 'write').mockImplementation(() => true);
    try {
      for (const agent_name of ['First participant', 'Second participant']) {
        const payload = { agent_name, host: 'codex', cwd: directory };
        expect(() => agentId(payload)).toThrow(/stable agent_id\/session_id or OCTOCODE_AGENT_ID/);
        registerHookAgent(database, payload, 'notify-deliver');
        expect(await runHookCommand('notify-deliver', JSON.stringify(payload))).toBe(1);
      }
      expect(listAgents(database, { workspacePath: directory }).agents).toEqual([]);
      expect(error).toHaveBeenCalledWith(expect.stringContaining('hook identity error'));
    } finally { error.mockRestore(); }
  });

  it('leaves tool events without mutation targets inert without an identity', async () => {
    writeAwarenessConfig(DEFAULT_AWARENESS_CONFIG, { path: join(directory, 'awareness.json') });
    for (const command of ['pre-edit', 'post-edit']) {
      expect(await runHookCommand(command, JSON.stringify({ cwd: directory, tool_name: 'Bash', tool_input: { command: 'pwd' } }))).toBe(0);
    }
    expect(listAgents(database, { workspacePath: directory }).agents).toEqual([]);
  });

});
