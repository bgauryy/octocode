import { describe, it, expect, beforeEach } from 'vitest';
import {
  assertCircuitAvailable,
  recordCircuitFailure,
  recordCircuitSuccess,
  resetCircuitBreaker,
  configureCircuitBreaker,
  CircuitOpenError,
  DEFAULT_CIRCUIT_FAILURE_THRESHOLD,
} from '../../../src/utils/http/circuitBreaker.js';

const URL_A = 'https://api.github.com/repos/x';
const URL_B = 'https://registry.npmjs.org/x';

describe('circuit breaker (#T13)', () => {
  beforeEach(() => resetCircuitBreaker());

  it('stays closed below the failure threshold', () => {
    for (let i = 0; i < DEFAULT_CIRCUIT_FAILURE_THRESHOLD - 1; i++) {
      recordCircuitFailure(URL_A, 1000);
    }
    expect(() => assertCircuitAvailable(URL_A, 1000)).not.toThrow();
  });

  it('opens after threshold consecutive failures and fails fast', () => {
    for (let i = 0; i < DEFAULT_CIRCUIT_FAILURE_THRESHOLD; i++) {
      recordCircuitFailure(URL_A, 1000);
    }
    expect(() => assertCircuitAvailable(URL_A, 1000)).toThrowError(
      CircuitOpenError
    );
  });

  it('is per-host (one host opening does not affect another)', () => {
    for (let i = 0; i < DEFAULT_CIRCUIT_FAILURE_THRESHOLD; i++) {
      recordCircuitFailure(URL_A, 1000);
    }
    expect(() => assertCircuitAvailable(URL_A, 1000)).toThrow();
    expect(() => assertCircuitAvailable(URL_B, 1000)).not.toThrow();
  });

  it('half-opens after the cooldown, then closes on a successful trial', () => {
    configureCircuitBreaker({ failureThreshold: 2, cooldownMs: 5000 });
    recordCircuitFailure(URL_A, 0);
    recordCircuitFailure(URL_A, 0);
    expect(() => assertCircuitAvailable(URL_A, 1000)).toThrow(); // still open
    // after cooldown: half-open trial allowed
    expect(() => assertCircuitAvailable(URL_A, 6000)).not.toThrow();
    recordCircuitSuccess(URL_A);
    expect(() => assertCircuitAvailable(URL_A, 7000)).not.toThrow(); // closed
  });

  it('re-opens if the half-open trial fails', () => {
    configureCircuitBreaker({ failureThreshold: 2, cooldownMs: 5000 });
    recordCircuitFailure(URL_A, 0);
    recordCircuitFailure(URL_A, 0);
    assertCircuitAvailable(URL_A, 6000); // → half-open
    recordCircuitFailure(URL_A, 6000); // trial fails → re-open
    expect(() => assertCircuitAvailable(URL_A, 6500)).toThrow();
  });

  it('a success resets the failure count', () => {
    recordCircuitFailure(URL_A, 0);
    recordCircuitFailure(URL_A, 0);
    recordCircuitSuccess(URL_A);
    for (let i = 0; i < DEFAULT_CIRCUIT_FAILURE_THRESHOLD - 1; i++) {
      recordCircuitFailure(URL_A, 0);
    }
    expect(() => assertCircuitAvailable(URL_A, 0)).not.toThrow();
  });
});
