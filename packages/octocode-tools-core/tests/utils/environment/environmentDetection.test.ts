import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const mockGetConfigSync = vi.hoisted(() => vi.fn());

vi.mock('@octocodeai/config', () => ({
  getConfigSync: mockGetConfigSync,
}));

const { detectEnvironment, shouldUseMCPLsp, getLspEnvironmentHint } =
  await import(
    '../../../src/utils/environment/environmentDetection.js'
  );

describe('detectEnvironment', () => {
  beforeEach(() => {
    delete process.env.VSCODE_PID;
    delete process.env.VSCODE_IPC_HOOK;
    delete process.env.CURSOR_CHANNEL;
    delete process.env.CURSOR_TRACE_ID;
  });

  afterEach(() => {
    delete process.env.VSCODE_PID;
    delete process.env.VSCODE_IPC_HOOK;
    delete process.env.CURSOR_CHANNEL;
    delete process.env.CURSOR_TRACE_ID;
  });

  it('returns vscode when VSCODE_PID is set', () => {
    process.env.VSCODE_PID = '1234';
    expect(detectEnvironment()).toBe('vscode');
  });

  it('returns vscode when VSCODE_IPC_HOOK is set', () => {
    process.env.VSCODE_IPC_HOOK = '/tmp/vscode.sock';
    expect(detectEnvironment()).toBe('vscode');
  });

  it('returns cursor when CURSOR_CHANNEL is set', () => {
    process.env.CURSOR_CHANNEL = 'stable';
    expect(detectEnvironment()).toBe('cursor');
  });

  it('returns cursor when CURSOR_TRACE_ID is set', () => {
    process.env.CURSOR_TRACE_ID = 'abc123';
    expect(detectEnvironment()).toBe('cursor');
  });

  it('returns standalone when no env vars are set', () => {
    expect(detectEnvironment()).toBe('standalone');
  });

  it('prefers vscode over cursor when both are set', () => {
    process.env.VSCODE_PID = '1234';
    process.env.CURSOR_CHANNEL = 'stable';
    expect(detectEnvironment()).toBe('vscode');
  });
});

describe('shouldUseMCPLsp', () => {
  it('returns true when local.enabled is true', () => {
    mockGetConfigSync.mockReturnValueOnce({ local: { enabled: true } });
    expect(shouldUseMCPLsp()).toBe(true);
  });

  it('returns false when local.enabled is false', () => {
    mockGetConfigSync.mockReturnValueOnce({ local: { enabled: false } });
    expect(shouldUseMCPLsp()).toBe(false);
  });

  it('returns false when getConfigSync throws', () => {
    mockGetConfigSync.mockImplementationOnce(() => { throw new Error('cfg error'); });
    expect(shouldUseMCPLsp()).toBe(false);
  });
});

describe('getLspEnvironmentHint', () => {
  it('returns null when local is enabled', () => {
    mockGetConfigSync.mockReturnValueOnce({ local: { enabled: true } });
    expect(getLspEnvironmentHint()).toBeNull();
  });

  it('returns a hint string when local is disabled', () => {
    mockGetConfigSync.mockReturnValueOnce({ local: { enabled: false } });
    const hint = getLspEnvironmentHint();
    expect(hint).toContain('ENABLE_LOCAL=false');
  });

  it('returns null when getConfigSync throws', () => {
    mockGetConfigSync.mockImplementationOnce(() => { throw new Error('cfg error'); });
    expect(getLspEnvironmentHint()).toBeNull();
  });
});
