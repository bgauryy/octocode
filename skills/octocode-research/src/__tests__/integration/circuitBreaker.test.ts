import { describe, it, expect, beforeEach, vi } from 'vitest';
import {
  withCircuitBreaker,
  resetCircuit,
  getCircuitState,
  configureCircuit,
  CircuitOpenError,
} from '../../utils/circuitBreaker.js';

describe('Circuit Breaker', () => {
  beforeEach(() => {
    resetCircuit('test');
    resetCircuit('lsp');
    resetCircuit('github');
    resetCircuit('local');
  });

  describe('Basic Functionality', () => {
    it('allows requests when circuit is closed', async () => {
      const result = await withCircuitBreaker('test', () => Promise.resolve('ok'));
      expect(result).toBe('ok');
      expect(getCircuitState('test').state).toBe('closed');
    });

    it('passes through function return values', async () => {
      const expected = { data: 'test', count: 42 };
      const result = await withCircuitBreaker('test', () => Promise.resolve(expected));
      expect(result).toEqual(expected);
    });

    it('propagates errors while circuit is closed', async () => {
      const error = new Error('test error');
      await expect(
        withCircuitBreaker('test', () => Promise.reject(error))
      ).rejects.toThrow('test error');
    });
  });

  describe('State Transitions', () => {
    it('opens after reaching failure threshold', async () => {
      configureCircuit('test', {
        failureThreshold: 2,
        successThreshold: 1,
        resetTimeoutMs: 1000,
      });

      const failingFn = () => Promise.reject(new Error('fail'));

      await withCircuitBreaker('test', failingFn).catch(() => {});
      expect(getCircuitState('test').state).toBe('closed');

      await withCircuitBreaker('test', failingFn).catch(() => {});
      expect(getCircuitState('test').state).toBe('open');
    });

    it('rejects immediately when circuit is open', async () => {
      configureCircuit('test', {
        failureThreshold: 1,
        successThreshold: 1,
        resetTimeoutMs: 10000,
      });

      await withCircuitBreaker('test', () => Promise.reject(new Error('fail'))).catch(() => {});

      await expect(
        withCircuitBreaker('test', () => Promise.resolve('ok'))
      ).rejects.toThrow(CircuitOpenError);
    });

    it('transitions to half-open after reset timeout', async () => {
      vi.useFakeTimers();

      configureCircuit('test', {
        failureThreshold: 1,
        successThreshold: 1,
        resetTimeoutMs: 1000,
      });

      await withCircuitBreaker('test', () => Promise.reject(new Error())).catch(() => {});
      expect(getCircuitState('test').state).toBe('open');

      vi.advanceTimersByTime(1100);

      const result = await withCircuitBreaker('test', () => Promise.resolve('recovered'));
      expect(result).toBe('recovered');
      expect(getCircuitState('test').state).toBe('closed');

      vi.useRealTimers();
    });

    it('closes after success in half-open state', async () => {
      vi.useFakeTimers();

      configureCircuit('test', {
        failureThreshold: 1,
        successThreshold: 1,
        resetTimeoutMs: 100,
      });

      await withCircuitBreaker('test', () => Promise.reject(new Error())).catch(() => {});
      
      vi.advanceTimersByTime(150);

      await withCircuitBreaker('test', () => Promise.resolve('ok'));
      expect(getCircuitState('test').state).toBe('closed');

      vi.useRealTimers();
    });

    it('reopens if failure occurs in half-open state', async () => {
      vi.useFakeTimers();

      configureCircuit('test', {
        failureThreshold: 1,
        successThreshold: 1,
        resetTimeoutMs: 100,
      });

      await withCircuitBreaker('test', () => Promise.reject(new Error())).catch(() => {});
      
      vi.advanceTimersByTime(150);

      await withCircuitBreaker('test', () => Promise.reject(new Error())).catch(() => {});
      expect(getCircuitState('test').state).toBe('open');

      vi.useRealTimers();
    });
  });

  describe('Pre-configured Circuits', () => {
    it('has LSP circuit configured', () => {
      expect(getCircuitState('lsp').state).toBe('closed');
    });

    it('has GitHub circuit configured', () => {
      expect(getCircuitState('github').state).toBe('closed');
    });
  });

  describe('CircuitOpenError', () => {
    it('includes circuit name and retry time', async () => {
      configureCircuit('test', {
        failureThreshold: 1,
        successThreshold: 1,
        resetTimeoutMs: 5000,
      });

      await withCircuitBreaker('test', () => Promise.reject(new Error())).catch(() => {});

      try {
        await withCircuitBreaker('test', () => Promise.resolve());
        expect.fail('Should have thrown');
      } catch (e) {
        expect(e).toBeInstanceOf(CircuitOpenError);
        const error = e as CircuitOpenError;
        expect(error.circuitName).toBe('test');
        expect(error.retryAfterMs).toBeGreaterThan(0);
        expect(error.retryAfterMs).toBeLessThanOrEqual(5000);
      }
    });
  });

  describe('Reset Functionality', () => {
    it('resets circuit to closed state', async () => {
      configureCircuit('test', {
        failureThreshold: 1,
        successThreshold: 1,
        resetTimeoutMs: 10000,
      });

      await withCircuitBreaker('test', () => Promise.reject(new Error())).catch(() => {});
      expect(getCircuitState('test').state).toBe('open');

      resetCircuit('test');
      expect(getCircuitState('test').state).toBe('closed');

      const result = await withCircuitBreaker('test', () => Promise.resolve('ok'));
      expect(result).toBe('ok');
    });
  });
});
