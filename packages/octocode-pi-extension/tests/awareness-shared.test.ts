import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { PiContext } from '../src/types.js';

const context = (id: string): PiContext => ({
  sessionManager: { getSessionId: () => id },
}) as PiContext;

describe('Awareness session identity', () => {
  beforeEach(() => {
    vi.resetModules();
    vi.stubEnv('OCTOCODE_AGENT_ID', undefined);
    vi.stubEnv('OCTOCODE_AGENT_NAME', undefined);
    vi.stubEnv('OCTOCODE_AGENT_VENDOR', undefined);
  });
  afterEach(() => vi.unstubAllEnvs());

  it('derives the one restart-stable Pi identity used by routing and event cursors', async () => {
    const { resolveAwarenessSessionAgentId } = await import('../src/tools/awareness-shared.js');
    expect(resolveAwarenessSessionAgentId(context('session-1'))).toBe('pi:session-1');
    expect(resolveAwarenessSessionAgentId({
      sessionManager: { getSessionFile: () => '/tmp/sessions/../sessions/one.jsonl' },
    } as PiContext)).toMatch(/^pi:file:[a-f0-9]{24}$/);
    expect(resolveAwarenessSessionAgentId({ sessionManager: {} } as PiContext)).toBeUndefined();
  });

  it('refreshes generated identities for new and forked sessions and restores resumed identity', async () => {
    const { getAwarenessAgentId } = await import('../src/tools/awareness-shared.js');
    expect(getAwarenessAgentId(context('first'))).toBe('pi:first');
    expect(getAwarenessAgentId()).toBe('pi:first');
    expect(getAwarenessAgentId(context('second'))).toBe('pi:second');
    expect(process.env.OCTOCODE_AGENT_ID).toBe('pi:second');
    expect(getAwarenessAgentId(context('fork'))).toBe('pi:fork');
    expect(getAwarenessAgentId(context('first'))).toBe('pi:first');
  });

  it('replaces the process fallback once Pi establishes a session', async () => {
    const { getAwarenessAgentId } = await import('../src/tools/awareness-shared.js');
    expect(getAwarenessAgentId()).toBe(`pi:${process.pid}`);
    expect(getAwarenessAgentId(context('established'))).toBe('pi:established');
  });

  it('preserves explicit user and inherited worker identities across sessions', async () => {
    vi.stubEnv('OCTOCODE_AGENT_ID', 'lead:worker:123');
    const { getAwarenessAgentId } = await import('../src/tools/awareness-shared.js');
    expect(getAwarenessAgentId(context('first'))).toBe('lead:worker:123');
    expect(getAwarenessAgentId(context('second'))).toBe('lead:worker:123');
  });

  it('honors an explicit override applied after an identity was generated', async () => {
    const { getAwarenessAgentId } = await import('../src/tools/awareness-shared.js');
    getAwarenessAgentId(context('first'));
    process.env.OCTOCODE_AGENT_ID = 'configured';
    expect(getAwarenessAgentId(context('second'))).toBe('configured');
  });

  it('distinguishes session files with the same basename without exposing private paths', async () => {
    const { getAwarenessAgentId } = await import('../src/tools/awareness-shared.js');
    const fileContext = (file: string) => ({ sessionManager: { getSessionFile: () => file } }) as PiContext;
    const first = getAwarenessAgentId(fileContext('/first/same.jsonl'));
    const second = getAwarenessAgentId(fileContext('/second/same.jsonl'));
    expect(first).not.toBe(second);
    expect(first).not.toContain('/first');
    expect(getAwarenessAgentId(fileContext('/first/same.jsonl'))).toBe(first);
  });

  it('keeps independent session IDs and reports each actual provider and session name', async () => {
    const { getAwarenessAgentIdentity } = await import('../src/tools/awareness-shared.js');
    vi.stubEnv('OCTOCODE_AGENT_VENDOR', 'inherited-parent-provider');
    const first = getAwarenessAgentIdentity({ ...context('first'), model: { provider: 'openai' }, sessionManager: { getSessionId: () => 'first', getSessionName: () => 'API review' } });
    const second = getAwarenessAgentIdentity({ ...context('second'), model: { provider: 'anthropic' }, sessionManager: { getSessionId: () => 'second', getSessionName: () => 'Test worker' } });
    expect(first).toEqual({ agentId: 'pi:first', name: 'API review', metadata: { vendor: 'openai', host: 'pi' } });
    expect(second).toEqual({ agentId: 'pi:second', name: 'Test worker', metadata: { vendor: 'anthropic', host: 'pi' } });
  });

  it('uses explicit identity labels without inferring unknown vendors from IDs', async () => {
    const { getAwarenessAgentIdentity } = await import('../src/tools/awareness-shared.js');
    vi.stubEnv('OCTOCODE_AGENT_ID', 'anthropic-looking-id');
    expect(getAwarenessAgentIdentity(context('first'))).toEqual({ agentId: 'anthropic-looking-id', name: 'anthropic-looking-id', metadata: { vendor: null, host: 'pi' } });
    vi.stubEnv('OCTOCODE_AGENT_NAME', '  Review lead  ');
    vi.stubEnv('OCTOCODE_AGENT_VENDOR', '  custom-provider  ');
    expect(getAwarenessAgentIdentity(context('first'))).toEqual({ agentId: 'anthropic-looking-id', name: 'Review lead', metadata: { vendor: 'custom-provider', host: 'pi' } });
  });
});
