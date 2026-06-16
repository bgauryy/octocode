import { describe, it, expect, vi, beforeEach } from 'vitest';

vi.mock('../../utils/circuitBreaker.js', () => {
  const circuitStates: Record<string, string> = {};
  return {
    configureCircuit: vi.fn(),
    withCircuitBreaker: vi.fn(async (name: string, fn: () => unknown) => {
      circuitStates[name] = 'used';
      return fn();
    }),
    getCircuitState: vi.fn().mockReturnValue({ state: 'closed', failures: 0 }),
    resetCircuit: vi.fn(),
    getAllCircuitStates: vi.fn().mockReturnValue({}),
    clearAllCircuits: vi.fn(),
    stopCircuitCleanup: vi.fn(),
    CircuitOpenError: class CircuitOpenError extends Error {
      circuitName: string;
      constructor(name: string) { super(`Circuit ${name} is open`); this.circuitName = name; }
    },
    _getUsedCircuits: () => circuitStates,
  };
});

vi.mock('../../utils/retry.js', () => ({
  withRetry: vi.fn(async (fn: () => unknown) => fn()),
  RETRY_CONFIGS: {
    github: { maxAttempts: 3, baseDelayMs: 1000, maxDelayMs: 30000 },
    local: { maxAttempts: 2, baseDelayMs: 200, maxDelayMs: 1000 },
    lsp: { maxAttempts: 3, baseDelayMs: 500, maxDelayMs: 5000 },
    package: { maxAttempts: 3, baseDelayMs: 500, maxDelayMs: 15000 },
  },
}));

vi.mock('../../utils/asyncTimeout.js', () => ({
  fireAndForgetWithTimeout: vi.fn(),
  withTimeout: vi.fn(async (fn: () => unknown) => fn()),
}));

import {
  withGitHubResilience,
  withLocalResilience,
  withLspResilience,
  withPackageResilience,
} from '../../utils/resilience.js';
import { withCircuitBreaker } from '../../utils/circuitBreaker.js';
import { withRetry } from '../../utils/retry.js';
import { withTimeout } from '../../utils/asyncTimeout.js';

describe('Resilience Wrappers', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe('withGitHubResilience', () => {
    it('executes operation successfully', async () => {
      const result = await withGitHubResilience(
        async () => ({ data: 'github-result' }),
        'ghSearchCode'
      );
      expect(result).toEqual({ data: 'github-result' });
    });

    it('uses timeout wrapper', async () => {
      await withGitHubResilience(async () => 'ok', 'ghSearchCode');
      expect(withTimeout).toHaveBeenCalled();
    });

    it('uses circuit breaker', async () => {
      await withGitHubResilience(async () => 'ok', 'ghSearchCode');
      expect(withCircuitBreaker).toHaveBeenCalled();
    });

    it('uses retry', async () => {
      await withGitHubResilience(async () => 'ok', 'ghSearchCode');
      expect(withRetry).toHaveBeenCalled();
    });

    it('maps tool to correct circuit', async () => {
      await withGitHubResilience(async () => 'ok', 'ghSearchCode');
      expect(withCircuitBreaker).toHaveBeenCalledWith(
        'github:search',
        expect.any(Function)
      );
    });

    it('maps content tools to github:content circuit', async () => {
      await withGitHubResilience(async () => 'ok', 'ghGetFileContent');
      expect(withCircuitBreaker).toHaveBeenCalledWith(
        'github:content',
        expect.any(Function)
      );
    });

    it('maps PR tools to github:pulls circuit', async () => {
      await withGitHubResilience(async () => 'ok', 'ghSearchPRs');
      expect(withCircuitBreaker).toHaveBeenCalledWith(
        'github:pulls',
        expect.any(Function)
      );
    });

    it('propagates errors from operation', async () => {
      vi.mocked(withRetry).mockRejectedValueOnce(new Error('API error'));
      await expect(
        withGitHubResilience(async () => { throw new Error('API error'); }, 'ghSearchCode')
      ).rejects.toThrow('API error');
    });
  });

  describe('withLocalResilience', () => {
    it('executes local operations', async () => {
      const result = await withLocalResilience(
        async () => ({ files: ['a.ts'] }),
        'localSearchCode'
      );
      expect(result).toEqual({ files: ['a.ts'] });
    });

    it('maps local tools to local circuit', async () => {
      await withLocalResilience(async () => 'ok', 'localSearchCode');
      expect(withCircuitBreaker).toHaveBeenCalledWith(
        'local',
        expect.any(Function)
      );
    });
  });

  describe('withLspResilience', () => {
    it('executes LSP operations', async () => {
      const result = await withLspResilience(
        async () => ({ definition: 'found' }),
        'lspGetSemantics'
      );
      expect(result).toEqual({ definition: 'found' });
    });

    it('maps lspGetSemantics to lsp:navigation circuit', async () => {
      await withLspResilience(async () => 'ok', 'lspGetSemantics');
      expect(withCircuitBreaker).toHaveBeenCalledWith(
        'lsp:navigation',
        expect.any(Function)
      );
    });
  });

  describe('withPackageResilience', () => {
    it('executes package operations', async () => {
      const result = await withPackageResilience(
        async () => ({ packages: [] }),
        'npmSearch'
      );
      expect(result).toEqual({ packages: [] });
    });

    it('maps to package circuit', async () => {
      await withPackageResilience(async () => 'ok', 'npmSearch');
      expect(withCircuitBreaker).toHaveBeenCalledWith(
        'package',
        expect.any(Function)
      );
    });
  });

  describe('composition order', () => {
    it('wraps in timeout -> circuit -> retry order', async () => {
      const callOrder: string[] = [];
      vi.mocked(withTimeout).mockImplementation(async (fn) => {
        callOrder.push('timeout');
        return fn();
      });
      vi.mocked(withCircuitBreaker).mockImplementation(async (_name, fn) => {
        callOrder.push('circuit');
        return fn();
      });
      vi.mocked(withRetry).mockImplementation(async (fn) => {
        callOrder.push('retry');
        return fn();
      });

      await withGitHubResilience(async () => 'ok', 'ghSearchCode');
      expect(callOrder).toEqual(['timeout', 'circuit', 'retry']);
    });
  });
});
