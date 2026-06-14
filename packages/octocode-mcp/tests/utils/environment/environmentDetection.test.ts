import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import {
  detectEnvironment,
  shouldUseMCPLsp,
  getLspEnvironmentHint,
} from '../../../../octocode-tools-core/src/utils/environment/environmentDetection.js';
import { _resetConfigCache } from 'octocode-shared';

describe('Environment Detection', () => {
  const originalEnv = { ...process.env };

  beforeEach(() => {
    delete process.env.VSCODE_PID;
    delete process.env.VSCODE_IPC_HOOK;
    delete process.env.CURSOR_CHANNEL;
    delete process.env.CURSOR_TRACE_ID;
    _resetConfigCache();
  });

  afterEach(() => {
    process.env = { ...originalEnv };
  });

  describe('detectEnvironment', () => {
    it('should detect VSCode via VSCODE_PID', () => {
      process.env.VSCODE_PID = '12345';
      expect(detectEnvironment()).toBe('vscode');
    });

    it('should detect VSCode via VSCODE_IPC_HOOK', () => {
      process.env.VSCODE_IPC_HOOK = '/tmp/vscode-ipc';
      expect(detectEnvironment()).toBe('vscode');
    });

    it('should detect Cursor via CURSOR_CHANNEL', () => {
      process.env.CURSOR_CHANNEL = 'stable';
      expect(detectEnvironment()).toBe('cursor');
    });

    it('should detect Cursor via CURSOR_TRACE_ID', () => {
      process.env.CURSOR_TRACE_ID = 'abc123';
      expect(detectEnvironment()).toBe('cursor');
    });

    it('should return standalone as default', () => {
      expect(detectEnvironment()).toBe('standalone');
    });

    it('should prioritize VSCode over Cursor', () => {
      process.env.VSCODE_PID = '12345';
      process.env.CURSOR_CHANNEL = 'stable';
      expect(detectEnvironment()).toBe('vscode');
    });
  });

  describe('shouldUseMCPLsp', () => {
    it('should return true when local tools are enabled by default', () => {
      expect(shouldUseMCPLsp()).toBe(true);
    });

    it('should return false when ENABLE_LOCAL is false', () => {
      process.env.ENABLE_LOCAL = 'false';
      _resetConfigCache();
      expect(shouldUseMCPLsp()).toBe(false);
    });

    it('should return true when ENABLE_LOCAL is true', () => {
      process.env.ENABLE_LOCAL = 'true';
      _resetConfigCache();
      expect(shouldUseMCPLsp()).toBe(true);
    });
  });

  describe('getLspEnvironmentHint', () => {
    it('should return null when local tools are enabled by default', () => {
      expect(getLspEnvironmentHint()).toBeNull();
    });

    it('should return hint when local tools are explicitly disabled', () => {
      process.env.ENABLE_LOCAL = 'false';
      _resetConfigCache();
      const hint = getLspEnvironmentHint();
      expect(hint).not.toBeNull();
      expect(hint).toContain('Local tools are disabled');
    });

    it('should return null when local tools are explicitly enabled', () => {
      process.env.ENABLE_LOCAL = 'true';
      _resetConfigCache();
      expect(getLspEnvironmentHint()).toBeNull();
    });
  });

  describe('shouldUseMCPLsp - error handling', () => {
    it('should return false when getConfigSync throws', async () => {
      const mod = await import('octocode-shared');
      const spy = vi.spyOn(mod, 'getConfigSync').mockImplementation(() => {
        throw new Error('Config file corrupted');
      });

      const { shouldUseMCPLsp: freshShouldUseMCPLsp } =
        await import('../../../../octocode-tools-core/src/utils/environment/environmentDetection.js');
      expect(freshShouldUseMCPLsp()).toBe(false);
      spy.mockRestore();
    });
  });
});
