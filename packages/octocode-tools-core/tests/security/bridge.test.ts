import type { ServerContext } from '@modelcontextprotocol/server';
import { beforeEach, describe, expect, it, vi } from 'vitest';

const engineSecurity = vi.hoisted(() => ({
  withSecurityValidation: vi.fn(),
  withBasicSecurityValidation: vi.fn(),
}));

vi.mock('@octocodeai/octocode-engine/security', () => engineSecurity);

import {
  withBasicSecurityValidation,
  withSecurityValidation,
} from '../../src/security/bridge.js';

const result = {
  content: [{ type: 'text' as const, text: 'ok' }],
};

describe('MCP v2 security context bridge', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('maps ServerContext auth, session, and cancellation into engine context', async () => {
    const inner = vi.fn().mockResolvedValue(result);
    engineSecurity.withSecurityValidation.mockReturnValue(inner);
    const handler = withSecurityValidation('testTool', vi.fn());
    const signal = new AbortController().signal;
    const authInfo = { token: 'token' };

    await handler({ query: 'value' }, {
      http: { authInfo },
      sessionId: 'session-1',
      mcpReq: { signal },
    } as ServerContext);

    expect(inner).toHaveBeenCalledWith(
      { query: 'value' },
      { authInfo, sessionId: 'session-1', signal }
    );
  });

  it('maps the MCP cancellation signal for basic tools', async () => {
    const inner = vi.fn().mockResolvedValue(result);
    engineSecurity.withBasicSecurityValidation.mockReturnValue(inner);
    const handler = withBasicSecurityValidation(vi.fn(), 'testTool');
    const signal = new AbortController().signal;

    await handler({}, {
      mcpReq: { signal },
    } as ServerContext);

    expect(inner).toHaveBeenCalledWith({}, { signal });
  });

  it('forwards a tool-specific timeout to the engine security wrapper', () => {
    engineSecurity.withSecurityValidation.mockReturnValue(vi.fn());

    withSecurityValidation('slowTool', vi.fn(), { timeoutMs: 120_000 });

    expect(engineSecurity.withSecurityValidation).toHaveBeenCalledWith(
      'slowTool',
      expect.any(Function),
      { timeoutMs: 120_000 }
    );
  });
});
