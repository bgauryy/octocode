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

export class CircuitOpenError extends Error {
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

export function recordCircuitSuccess(url: string): void {
  const circuit = circuits.get(hostKey(url));
  if (circuit) {
    circuit.failures = 0;
    circuit.state = 'closed';
  }
}

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

export function resetCircuitBreaker(): void {
  circuits.clear();
  failureThreshold = DEFAULT_CIRCUIT_FAILURE_THRESHOLD;
  cooldownMs = DEFAULT_CIRCUIT_COOLDOWN_MS;
}

export function configureCircuitBreaker(options: CircuitBreakerOptions): void {
  if (typeof options.failureThreshold === 'number') {
    failureThreshold = options.failureThreshold;
  }
  if (typeof options.cooldownMs === 'number') {
    cooldownMs = options.cooldownMs;
  }
}

export function isCircuitOpen(url: string, now: number = Date.now()): boolean {
  const circuit = circuits.get(hostKey(url));
  if (!circuit || circuit.state === 'closed') return false;
  if (circuit.state === 'open') return now - circuit.openedAt < cooldownMs;
  return false;
}
