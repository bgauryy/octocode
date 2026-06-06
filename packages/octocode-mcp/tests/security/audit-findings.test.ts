import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';

import {
  initializeSession,
  resetSessionManager,
  logToolCall,
} from '../../src/session.js';
import { initialize, cleanup } from '../../src/serverConfig.js';
import {
  buildChildProcessEnv,
  SENSITIVE_ENV_VARS,
} from '../../src/utils/exec/spawn.js';

describe('Finding 2 — Telemetry excludes sensitive data', () => {
  let savedLog: string | undefined;

  beforeEach(async () => {
    savedLog = process.env.LOG;
    process.env.LOG = 'true';
    cleanup();
    vi.clearAllMocks();
    resetSessionManager();
    vi.mocked(fetch).mockResolvedValue(new Response('ok'));
    await initialize();
    initializeSession();
  });

  afterEach(() => {
    if (savedLog === undefined) delete process.env.LOG;
    else process.env.LOG = savedLog;
    cleanup();
    resetSessionManager();
  });

  it('payload contains NONE of mainResearchGoal / researchGoal / reasoning', async () => {
    await logToolCall(
      'githubSearchCode',
      ['facebook/react'],
      'SECRET BUSINESS GOAL',
      'find vulnerable endpoints',
      'because the CEO told me to'
    );

    expect(vi.mocked(fetch)).toHaveBeenCalled();
    const call = vi.mocked(fetch).mock.calls[0];
    const bodyStr = String((call?.[1] as RequestInit | undefined)?.body ?? '');

    expect(bodyStr).not.toContain('SECRET BUSINESS GOAL');
    expect(bodyStr).not.toContain('find vulnerable endpoints');
    expect(bodyStr).not.toContain('because the CEO told me to');
  });

  it('redacts repo names for non-local tools', async () => {
    await logToolCall(
      'githubSearchCode',
      ['wix-private/billing-service', 'wix-private/payments-core'],
      'g',
      'r',
      'r'
    );

    const parsed = JSON.parse(
      (vi.mocked(fetch).mock.calls[0]?.[1] as RequestInit).body as string
    ) as Record<string, any>;
    const data = parsed.data;
    expect(data.repos).toEqual(['[redacted]', '[redacted]']);
  });

  it('sends empty repos for local tools', async () => {
    await logToolCall('localSearchCode', ['/Users/me/secret'], 'g', 'r', 'r');

    const parsed = JSON.parse(
      (vi.mocked(fetch).mock.calls[0]?.[1] as RequestInit).body as string
    ) as Record<string, any>;
    const data = parsed.data;
    expect(data.repos).toEqual([]);
  });

  it('LOG=false blocks tool telemetry but init always fires', async () => {
    cleanup();
    process.env.LOG = 'false';
    await initialize();
    resetSessionManager();
    vi.mocked(fetch).mockClear();

    const session = initializeSession();
    await session.logInit();
    expect(vi.mocked(fetch)).toHaveBeenCalledTimes(1);
    const payload = JSON.parse(
      (vi.mocked(fetch).mock.calls[0]![1] as RequestInit).body as string
    );
    expect(payload.intent).toBe('init');

    vi.mocked(fetch).mockClear();
    await logToolCall('githubSearchCode', ['repo'], 'g', 'r', 'r');
    expect(vi.mocked(fetch)).not.toHaveBeenCalled();
  });
});

describe('Finding 6 — No secret values leak to child env', () => {
  const savedEnv: Record<string, string | undefined> = {};

  beforeEach(() => {
    for (const key of [...SENSITIVE_ENV_VARS, 'PATH']) {
      savedEnv[key] = process.env[key];
    }
    for (const v of SENSITIVE_ENV_VARS) {
      process.env[v] = `LEAK_${v}_LEAK`;
    }
  });

  afterEach(() => {
    for (const [key, value] of Object.entries(savedEnv)) {
      if (value === undefined) delete process.env[key];
      else process.env[key] = value;
    }
  });

  it('no SENSITIVE_ENV_VARS value appears in any child env entry', () => {
    const env = buildChildProcessEnv();
    const allValues = Object.values(env).filter(Boolean).join('\n');

    for (const v of SENSITIVE_ENV_VARS) {
      expect(allValues, `value of ${v} leaked`).not.toContain(`LEAK_${v}_LEAK`);
    }
  });

  it('non-allowlisted override is silently rejected', () => {
    const env = buildChildProcessEnv({ GITHUB_TOKEN: 'injected-token' });
    expect(env.GITHUB_TOKEN).toBeUndefined();
    expect(Object.values(env).join('\n')).not.toContain('injected-token');
  });
});
