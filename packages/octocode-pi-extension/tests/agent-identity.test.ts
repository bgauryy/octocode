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
    const { resolveSessionAgentId } = await import('../src/tools/agent-identity.js');
    expect(resolveSessionAgentId(context('session-1'))).toBe('pi:session-1');
    expect(resolveSessionAgentId({
      sessionManager: { getSessionFile: () => '/tmp/sessions/../sessions/one.jsonl' },
    } as PiContext)).toMatch(/^pi:file:[a-f0-9]{24}$/);
    expect(resolveSessionAgentId({ sessionManager: {} } as PiContext)).toBeUndefined();
  });

  it('refreshes generated identities for new and forked sessions and restores resumed identity', async () => {
    const { getAgentId } = await import('../src/tools/agent-identity.js');
    expect(getAgentId(context('first'))).toBe('pi:first');
    expect(getAgentId()).toBe('pi:first');
    expect(getAgentId(context('second'))).toBe('pi:second');
    expect(process.env.OCTOCODE_AGENT_ID).toBe('pi:second');
    expect(getAgentId(context('fork'))).toBe('pi:fork');
    expect(getAgentId(context('first'))).toBe('pi:first');
  });

  it('replaces the process fallback once Pi establishes a session', async () => {
    const { getAgentId } = await import('../src/tools/agent-identity.js');
    expect(getAgentId()).toBe(`pi:${process.pid}`);
    expect(getAgentId(context('established'))).toBe('pi:established');
  });

  it('preserves explicit user and inherited worker identities across sessions', async () => {
    vi.stubEnv('OCTOCODE_AGENT_ID', 'lead:worker:123');
    const { getAgentId } = await import('../src/tools/agent-identity.js');
    expect(getAgentId(context('first'))).toBe('lead:worker:123');
    expect(getAgentId(context('second'))).toBe('lead:worker:123');
  });

  it('honors an explicit override applied after an identity was generated', async () => {
    const { getAgentId } = await import('../src/tools/agent-identity.js');
    getAgentId(context('first'));
    process.env.OCTOCODE_AGENT_ID = 'configured';
    expect(getAgentId(context('second'))).toBe('configured');
  });

  it('distinguishes session files with the same basename without exposing private paths', async () => {
    const { getAgentId } = await import('../src/tools/agent-identity.js');
    const fileContext = (file: string) => ({ sessionManager: { getSessionFile: () => file } }) as PiContext;
    const first = getAgentId(fileContext('/first/same.jsonl'));
    const second = getAgentId(fileContext('/second/same.jsonl'));
    expect(first).not.toBe(second);
    expect(first).not.toContain('/first');
    expect(getAgentId(fileContext('/first/same.jsonl'))).toBe(first);
  });

  it('keeps independent session IDs and reports each actual provider and session name', async () => {
    const { getAgentIdentity } = await import('../src/tools/agent-identity.js');
    vi.stubEnv('OCTOCODE_AGENT_VENDOR', 'inherited-parent-provider');
    const first = getAgentIdentity({ ...context('first'), model: { provider: 'openai' }, sessionManager: { getSessionId: () => 'first', getSessionName: () => 'API review' } });
    const second = getAgentIdentity({ ...context('second'), model: { provider: 'anthropic' }, sessionManager: { getSessionId: () => 'second', getSessionName: () => 'Test worker' } });
    expect(first).toEqual({ agentId: 'pi:first', name: 'API review', metadata: { vendor: 'openai', host: 'pi' } });
    expect(second).toEqual({ agentId: 'pi:second', name: 'Test worker', metadata: { vendor: 'anthropic', host: 'pi' } });
  });

  it('uses explicit identity labels without inferring unknown vendors from IDs', async () => {
    const { getAgentIdentity } = await import('../src/tools/agent-identity.js');
    vi.stubEnv('OCTOCODE_AGENT_ID', 'anthropic-looking-id');
    expect(getAgentIdentity(context('first'))).toEqual({ agentId: 'anthropic-looking-id', name: 'anthropic-looking-id', metadata: { vendor: null, host: 'pi' } });
    vi.stubEnv('OCTOCODE_AGENT_NAME', '  Review lead  ');
    vi.stubEnv('OCTOCODE_AGENT_VENDOR', '  custom-provider  ');
    expect(getAgentIdentity(context('first'))).toEqual({ agentId: 'anthropic-looking-id', name: 'Review lead', metadata: { vendor: 'custom-provider', host: 'pi' } });
  });
});
