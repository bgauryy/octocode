/**
 * Per-host circuit breaker for external HTTP (#T13).
 *
 * `fetchWithRetries` already backs off and retries, but with no breaker a fully
 * down dependency causes every call to grind through its full retry budget
 * (slow) and hammers the failing host. The breaker trips after a host accrues
 * `failureThreshold` consecutive *fetch-level* failures (retry-exhaustion /
 * network — NOT 4xx client errors or aborts), then fails fast for `cooldownMs`.
 * After the cooldown it half-opens: the next call is allowed as a trial; success
 * closes the circuit, another failure re-opens it.
 *
 * State is per-host and process-global; call `resetCircuitBreaker()` between
 * tests to avoid cross-test leakage.
 */

export interface CircuitBreakerOptions {
  failureThreshold?: number;
  cooldownMs?: number;
}

type CircuitState = 'closed' | 'open' | 'half-open';

interface HostCircuit {
  failures: number;
  state: CircuitState;
  openedAt: number;
}

export const DEFAULT_CIRCUIT_FAILURE_THRESHOLD = 5;
export const DEFAULT_CIRCUIT_COOLDOWN_MS = 30_000;

const circuits = new Map<string, HostCircuit>();
let failureThreshold = DEFAULT_CIRCUIT_FAILURE_THRESHOLD;
let cooldownMs = DEFAULT_CIRCUIT_COOLDOWN_MS;

function hostKey(url: string): string {
  try {
    return new URL(url).host || url;
  } catch {
    return url;
  }
}

/** Thrown when a request is short-circuited because the host's circuit is open. */
export class CircuitOpenError extends Error {
  /** Non-retryable: callers must not retry a fast-fail. */
  readonly retryable = false;
  readonly host: string;
  readonly retryAfterMs: number;
  constructor(host: string, retryAfterMs: number) {
    super(
      `Circuit open for ${host}: too many recent failures — failing fast, retry in ~${Math.ceil(
        retryAfterMs / 1000
      )}s.`
    );
    this.name = 'CircuitOpenError';
    this.host = host;
    this.retryAfterMs = retryAfterMs;
  }
}

/**
 * Throw `CircuitOpenError` when the host circuit is open and the cooldown has
 * not elapsed. When the cooldown has elapsed, transition to half-open and allow
 * the call through as a trial.
 */
export function assertCircuitAvailable(
  url: string,
  now: number = Date.now()
): void {
  const circuit = circuits.get(hostKey(url));
  if (!circuit || circuit.state === 'closed') return;
  if (circuit.state === 'open') {
    const elapsed = now - circuit.openedAt;
    if (elapsed < cooldownMs) {
      throw new CircuitOpenError(hostKey(url), cooldownMs - elapsed);
    }
    circuit.state = 'half-open';
  }
}

/** Record a successful call — closes the circuit and clears the failure count. */
export function recordCircuitSuccess(url: string): void {
  const circuit = circuits.get(hostKey(url));
  if (circuit) {
    circuit.failures = 0;
    circuit.state = 'closed';
  }
}

/** Record a fetch-level failure (retry-exhaustion / network) for the host. */
export function recordCircuitFailure(
  url: string,
  now: number = Date.now()
): void {
  const key = hostKey(url);
  const circuit = circuits.get(key) ?? {
    failures: 0,
    state: 'closed' as CircuitState,
    openedAt: 0,
  };
  if (circuit.state === 'half-open') {
    // The trial failed — re-open immediately.
    circuit.state = 'open';
    circuit.openedAt = now;
  } else {
    circuit.failures += 1;
    if (circuit.failures >= failureThreshold) {
      circuit.state = 'open';
      circuit.openedAt = now;
    }
  }
  circuits.set(key, circuit);
}

/** Test helper: clear all circuits and restore default thresholds. */
export function resetCircuitBreaker(): void {
  circuits.clear();
  failureThreshold = DEFAULT_CIRCUIT_FAILURE_THRESHOLD;
  cooldownMs = DEFAULT_CIRCUIT_COOLDOWN_MS;
}

/** Override thresholds (e.g. from config). */
export function configureCircuitBreaker(options: CircuitBreakerOptions): void {
  if (typeof options.failureThreshold === 'number') {
    failureThreshold = options.failureThreshold;
  }
  if (typeof options.cooldownMs === 'number') {
    cooldownMs = options.cooldownMs;
  }
}
