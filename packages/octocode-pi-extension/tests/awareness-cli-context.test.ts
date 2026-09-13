import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import {
  defaultDbPath,
  AWARENESS_PI_HOST_PROMPT,
} from '@octocodeai/octocode-awareness/host';
import { SYSTEM_PROMPT } from '../src/prompts/system-prompt.js';
import { OCTOCODE_SUPPORT_TOOL_NAMES } from '../src/constants.js';
import type { PiContext } from '../src/types.js';
import {
  buildAwarenessCliEnvironment,
  renderAwarenessCliContext,
} from '../src/tools/awareness-cli-context.js';
import { getAwarenessAgentIdentity } from '../src/tools/awareness-shared.js';
import { buildAwarenessContext } from '../src/tools/awareness-context.js';

beforeEach(() => {
  vi.stubEnv('OCTOCODE_AGENT_NAME', undefined);
  vi.stubEnv('OCTOCODE_AGENT_VENDOR', undefined);
});
afterEach(() => vi.unstubAllEnvs());

describe('canonical Awareness CLI in Pi', () => {
  it('injects the canonical policy behind one native Awareness facade', () => {
    expect(SYSTEM_PROMPT).toContain(AWARENESS_PI_HOST_PROMPT);
    expect([...OCTOCODE_SUPPORT_TOOL_NAMES]).toContain('awareness');
    for (const name of ['memory', 'message', 'lock'])
      expect([...OCTOCODE_SUPPORT_TOOL_NAMES]).not.toContain(name);
  });

  it('binds shell calls to the native store, workspace, identity and local runtime', () => {
    vi.stubEnv('OCTOCODE_STORAGE_MODE', 'persistent');
    vi.stubEnv('OCTOCODE_AGENT_ID', 'pi-cli-test');
    const ctx = { cwd: process.cwd() } as PiContext;
    const env = buildAwarenessCliEnvironment(ctx);
    expect(env.OCTOCODE_AGENT_ID).toBe('pi-cli-test');
    expect(env.OCTOCODE_NODE).toBe(process.execPath);
    expect(env.OCTOCODE_AWARENESS_DB).toBe(defaultDbPath(process.cwd()));
    expect(env.OCTOCODE_AWARENESS_WORKSPACE).toBe(ctx.cwd);
    expect(env.OCTOCODE_AWARENESS_CLI).toMatch(/octocode-awareness\.js$/);
    const rendered = renderAwarenessCliContext(ctx);
    expect(rendered).toContain('$OCTOCODE_AWARENESS_DB');
    expect(rendered).toContain('lacks the native facade');
    expect(rendered).not.toContain('pi-cli-test');
    expect(rendered).not.toContain(ctx.cwd);
  });

  it('does not advertise a durable CLI binding when persistence is disabled', () => {
    vi.stubEnv('OCTOCODE_STORAGE_MODE', 'memory');
    vi.stubEnv('OCTOCODE_AWARENESS_DB', '/stale/awareness.sqlite3');
    vi.stubEnv('OCTOCODE_AWARENESS_CLI', '/stale/cli.js');
    expect(
      buildAwarenessCliEnvironment({ cwd: process.cwd() } as PiContext)
        .OCTOCODE_AWARENESS_DB
    ).toBeUndefined();
    expect(renderAwarenessCliContext()).toContain(
      'Persistent storage is disabled'
    );
  });

  it('honors the explicit inherited database for worktree native calls', () => {
    vi.stubEnv('OCTOCODE_AWARENESS_DB', '/parent/.octocode/awareness.sqlite3');
    const context = buildAwarenessContext({ cwd: '/worktree' } as PiContext);
    expect(context.workspace).toBe('/worktree');
    expect(context.database).toBe('/parent/.octocode/awareness.sqlite3');
  });

  it('exports the native identity and actual worker provider despite stale inherited labels', () => {
    vi.stubEnv('OCTOCODE_STORAGE_MODE', 'persistent');
    vi.stubEnv('OCTOCODE_AGENT_ID', 'worker:123');
    vi.stubEnv('OCTOCODE_AGENT_VENDOR', 'parent-vendor');
    vi.stubEnv('OCTOCODE_AGENT_HOST', 'parent-host');
    const ctx = {
      cwd: process.cwd(),
      model: { provider: 'anthropic' },
      sessionManager: { getSessionName: () => 'Parser review' },
    } as PiContext;
    const nativeIdentity = getAwarenessAgentIdentity(ctx);
    const env = buildAwarenessCliEnvironment(ctx);
    expect(env.OCTOCODE_AGENT_ID).toBe(nativeIdentity.agentId);
    expect(env.OCTOCODE_AGENT_NAME).toBe(nativeIdentity.name);
    expect(env.OCTOCODE_AGENT_VENDOR).toBe(nativeIdentity.metadata.vendor);
    expect(env.OCTOCODE_AGENT_HOST).toBe(nativeIdentity.metadata.host);
    expect(env.OCTOCODE_AGENT_VENDOR).toBe('anthropic');
    expect(env.OCTOCODE_AGENT_HOST).toBe('pi');
  });

  it('reports unknown vendor explicitly without guessing from a routing ID', () => {
    vi.stubEnv('OCTOCODE_STORAGE_MODE', 'persistent');
    vi.stubEnv('OCTOCODE_AGENT_ID', 'openai:opaque-id');
    const ctx = { cwd: process.cwd() } as PiContext;
    expect(
      buildAwarenessCliEnvironment(ctx).OCTOCODE_AGENT_VENDOR
    ).toBeUndefined();
  });
});
