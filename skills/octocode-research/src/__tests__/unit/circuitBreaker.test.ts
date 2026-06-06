import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import {
  withCircuitBreaker,
  getCircuitState,
  resetCircuit,
  configureCircuit,
  CircuitOpenError,
  getAllCircuitStates,
} from '../../utils/circuitBreaker.js';

describe('withCircuitBreaker', () => {
  beforeEach(() => {
    resetCircuit('test');
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('executes operation successfully when circuit is closed', async () => {
    const operation = vi.fn().mockResolvedValue('success');

    const result = await withCircuitBreaker('test', operation);

    expect(result).toBe('success');
    expect(operation).toHaveBeenCalledTimes(1);
    expect(getCircuitState('test').state).toBe('closed');
  });

  it('opens circuit after failure threshold', async () => {
    configureCircuit('test', { failureThreshold: 3 });
    const operation = vi.fn().mockRejectedValue(new Error('fail'));

    for (let i = 0; i < 3; i++) {
      await expect(withCircuitBreaker('test', operation)).rejects.toThrow('fail');
    }

    expect(getCircuitState('test').state).toBe('open');
  });

  it('uses fallback when circuit is open', async () => {
    configureCircuit('test', { failureThreshold: 1, resetTimeoutMs: 10000 });
    const operation = vi.fn().mockRejectedValue(new Error('fail'));
    const fallback = vi.fn().mockReturnValue('fallback');

    await expect(withCircuitBreaker('test', operation)).rejects.toThrow();

    const result = await withCircuitBreaker('test', operation, fallback);

    expect(result).toBe('fallback');
    expect(fallback).toHaveBeenCalled();
  });

  it('throws CircuitOpenError when open and no fallback', async () => {
    configureCircuit('test', { failureThreshold: 1, resetTimeoutMs: 10000 });
    const operation = vi.fn().mockRejectedValue(new Error('fail'));

    await expect(withCircuitBreaker('test', operation)).rejects.toThrow();

    await expect(withCircuitBreaker('test', operation)).rejects.toBeInstanceOf(
      CircuitOpenError
    );
  });

  it('enters half-open state after reset timeout', async () => {
    configureCircuit('test', { failureThreshold: 1, resetTimeoutMs: 1000 });
    const operation = vi
      .fn()
      .mockRejectedValueOnce(new Error('fail'))
      .mockResolvedValue('success');

    await expect(withCircuitBreaker('test', operation)).rejects.toThrow();
    expect(getCircuitState('test').state).toBe('open');

    vi.advanceTimersByTime(1001);

    const result = await withCircuitBreaker('test', operation);
    expect(result).toBe('success');
  });

  it('closes circuit after success threshold in half-open', async () => {
    configureCircuit('test', {
      failureThreshold: 1,
      successThreshold: 2,
      resetTimeoutMs: 1000,
    });

    const failOp = vi.fn().mockRejectedValue(new Error('fail'));
    const successOp = vi.fn().mockResolvedValue('success');

    await expect(withCircuitBreaker('test', failOp)).rejects.toThrow();

    vi.advanceTimersByTime(1001);

    await withCircuitBreaker('test', successOp);
    expect(getCircuitState('test').state).toBe('half-open');

    await withCircuitBreaker('test', successOp);
    expect(getCircuitState('test').state).toBe('closed');
  });

  it('reopens circuit if half-open attempt fails', async () => {
    configureCircuit('test', { failureThreshold: 1, resetTimeoutMs: 1000 });

    const operation = vi.fn().mockRejectedValue(new Error('fail'));

    await expect(withCircuitBreaker('test', operation)).rejects.toThrow();

    vi.advanceTimersByTime(1001);

    await expect(withCircuitBreaker('test', operation)).rejects.toThrow();
    expect(getCircuitState('test').state).toBe('open');
  });

  it('resets failures on success in closed state', async () => {
    configureCircuit('test', { failureThreshold: 3 });

    const failOp = vi.fn().mockRejectedValue(new Error('fail'));
    const successOp = vi.fn().mockResolvedValue('success');

    await expect(withCircuitBreaker('test', failOp)).rejects.toThrow();
    await expect(withCircuitBreaker('test', failOp)).rejects.toThrow();
    expect(getCircuitState('test').failures).toBe(2);

    await withCircuitBreaker('test', successOp);
    expect(getCircuitState('test').failures).toBe(0);

    expect(getCircuitState('test').state).toBe('closed');
  });
});

describe('getCircuitState', () => {
  beforeEach(() => {
    resetCircuit('test');
  });

  it('returns initial state for new circuit', () => {
    const state = getCircuitState('new-circuit');

    expect(state.state).toBe('closed');
    expect(state.failures).toBe(0);
    expect(state.isHealthy).toBe(true);
  });

  it('returns accurate failure count', async () => {
    configureCircuit('test', { failureThreshold: 5 });
    const operation = vi.fn().mockRejectedValue(new Error('fail'));

    await expect(withCircuitBreaker('test', operation)).rejects.toThrow();
    await expect(withCircuitBreaker('test', operation)).rejects.toThrow();

    const state = getCircuitState('test');
    expect(state.failures).toBe(2);
  });

  it('reports unhealthy when circuit is open', async () => {
    configureCircuit('test', { failureThreshold: 1 });
    const operation = vi.fn().mockRejectedValue(new Error('fail'));

    await expect(withCircuitBreaker('test', operation)).rejects.toThrow();

    const state = getCircuitState('test');
    expect(state.isHealthy).toBe(false);
  });
});

describe('resetCircuit', () => {
  it('resets circuit to closed state', async () => {
    configureCircuit('test', { failureThreshold: 1 });
    const operation = vi.fn().mockRejectedValue(new Error('fail'));

    await expect(withCircuitBreaker('test', operation)).rejects.toThrow();
    expect(getCircuitState('test').state).toBe('open');

    resetCircuit('test');

    expect(getCircuitState('test').state).toBe('closed');
    expect(getCircuitState('test').failures).toBe(0);
  });
});

describe('getAllCircuitStates', () => {
  beforeEach(() => {
    resetCircuit('circuit-a');
    resetCircuit('circuit-b');
  });

  it('returns states for all known circuits', async () => {
    configureCircuit('circuit-a', { failureThreshold: 1 });
    const failOp = vi.fn().mockRejectedValue(new Error('fail'));
    const successOp = vi.fn().mockResolvedValue('success');

    await expect(withCircuitBreaker('circuit-a', failOp)).rejects.toThrow();

    await withCircuitBreaker('circuit-b', successOp);

    const states = getAllCircuitStates();

    expect(states['circuit-a'].state).toBe('open');
    expect(states['circuit-a'].isHealthy).toBe(false);
    expect(states['circuit-b'].state).toBe('closed');
    expect(states['circuit-b'].isHealthy).toBe(true);
  });
});

describe('CircuitOpenError', () => {
  it('contains circuit name and retry info', () => {
    const error = new CircuitOpenError('test', 5000);

    expect(error.circuitName).toBe('test');
    expect(error.retryAfterMs).toBe(5000);
    expect(error.message).toContain('test');
    expect(error.message).toContain('5s');
  });
});
