import { agentLog, warnLog, successLog, errorLog } from './colors.js';
import { logRateLimit } from '../index.js';
import { fireAndForgetWithTimeout } from './asyncTimeout.js';


type CircuitState = 'closed' | 'open' | 'half-open';


const MAX_CIRCUITS = 100;
const CIRCUIT_TTL_MS = 3600000;
const CLEANUP_INTERVAL_MS = 600000;

interface CircuitRecord {
  failures: number;
  successes: number;
  lastFailure: number;
  lastAttempt: number;
  state: CircuitState;
  createdAt: number;
}


export interface CircuitBreakerConfig {
  
  failureThreshold: number;
  
  successThreshold: number;
  
  resetTimeoutMs: number;
}


const DEFAULT_CONFIG: CircuitBreakerConfig = {
  failureThreshold: 3,

  successThreshold: 2,

  resetTimeoutMs: 30000,
};


const circuits = new Map<string, CircuitRecord>();
const configs = new Map<string, CircuitBreakerConfig>();


function getCircuit(name: string): CircuitRecord {
  if (!circuits.has(name)) {
    if (circuits.size >= MAX_CIRCUITS) {
      cleanupStaleCircuits();
      if (circuits.size >= MAX_CIRCUITS) {
        const oldestKey = findOldestCircuit();
        if (oldestKey) {
          circuits.delete(oldestKey);
          configs.delete(oldestKey);
          console.log(warnLog(`⚠️ Evicted oldest circuit '${oldestKey}' to make room`));
        }
      }
    }

    circuits.set(name, {
      failures: 0,
      successes: 0,
      lastFailure: 0,
      lastAttempt: 0,
      state: 'closed',
      createdAt: Date.now(),
    });
  }
  return circuits.get(name)!;
}


function findOldestCircuit(): string | null {
  let oldest: string | null = null;
  let oldestTime = Infinity;

  for (const [name, circuit] of circuits) {
    if (circuit.lastAttempt < oldestTime) {
      oldestTime = circuit.lastAttempt;
      oldest = name;
    }
  }

  return oldest;
}


function getConfig(name: string): CircuitBreakerConfig {
  return configs.get(name) || DEFAULT_CONFIG;
}


export function configureCircuit(
  name: string,
  config: Partial<CircuitBreakerConfig>
): void {
  configs.set(name, { ...DEFAULT_CONFIG, ...config });
}

export async function withCircuitBreaker<T>(
  name: string,
  operation: () => Promise<T>,
  fallback?: () => T | Promise<T>
): Promise<T> {
  const circuit = getCircuit(name);
  const config = getConfig(name);
  const now = Date.now();

  circuit.lastAttempt = now;

  if (circuit.state === 'open') {
    if (now - circuit.lastFailure > config.resetTimeoutMs) {
      circuit.state = 'half-open';
      console.log(warnLog(`🟡 Circuit ${name} entering half-open state`));
    } else {
      console.log(
        `🔴 Circuit ${name} is OPEN - ${Math.ceil((circuit.lastFailure + config.resetTimeoutMs - now) / 1000)}s until retry`
      );
      if (fallback) {
        return fallback();
      }
      throw new CircuitOpenError(name, circuit.lastFailure + config.resetTimeoutMs - now);
    }
  }

  try {
    const result = await operation();

    if (circuit.state === 'half-open') {
      circuit.successes++;
      if (circuit.successes >= config.successThreshold) {
        circuit.state = 'closed';
        circuit.failures = 0;
        circuit.successes = 0;
        console.log(successLog(`🟢 Circuit ${name} CLOSED after recovery`));
      }
    } else {
      circuit.failures = 0;
    }

    return result;
  } catch (error) {
    circuit.failures++;
    circuit.lastFailure = now;
    circuit.successes = 0;

    if (circuit.state === 'half-open') {
      circuit.state = 'open';
      console.log(errorLog(`🔴 Circuit ${name} back to OPEN after half-open failure`));
      fireAndForgetWithTimeout(
        () => logRateLimit({
          limit_type: 'secondary',
          api_method: 'circuit_breaker',
          retry_after_seconds: config.resetTimeoutMs / 1000,
          details: `Circuit '${name}' back to OPEN after half-open failure`
        }),
        5000,
        'logRateLimit'
      );
    } else if (circuit.failures >= config.failureThreshold) {
      circuit.state = 'open';
      console.log(
        `🔴 Circuit ${name} OPENED after ${circuit.failures} failures`
      );
      fireAndForgetWithTimeout(
        () => logRateLimit({
          limit_type: 'secondary',
          api_method: 'circuit_breaker',
          retry_after_seconds: config.resetTimeoutMs / 1000,
          details: `Circuit '${name}' OPENED after ${circuit.failures} failures`
        }),
        5000,
        'logRateLimit'
      );
    }

    throw error;
  }
}


export function getCircuitState(name: string): {
  state: CircuitState;
  failures: number;
  lastFailure: number;
  isHealthy: boolean;
} {
  const circuit = getCircuit(name);
  return {
    state: circuit.state,
    failures: circuit.failures,
    lastFailure: circuit.lastFailure,
    isHealthy: circuit.state === 'closed',
  };
}


export function resetCircuit(name: string): void {
  const circuit = getCircuit(name);
  circuit.state = 'closed';
  circuit.failures = 0;
  circuit.successes = 0;
  circuit.lastFailure = 0;
  console.log(agentLog(`🔄 Circuit ${name} manually reset to CLOSED`));
}


export function getAllCircuitStates(): Record<
  string,
  { state: CircuitState; failures: number; isHealthy: boolean }
> {
  const states: Record<
    string,
    { state: CircuitState; failures: number; isHealthy: boolean }
  > = {};

  for (const [name, circuit] of circuits) {
    states[name] = {
      state: circuit.state,
      failures: circuit.failures,
      isHealthy: circuit.state === 'closed',
    };
  }

  return states;
}


export class CircuitOpenError extends Error {
  readonly circuitName: string;
  readonly retryAfterMs: number;

  constructor(name: string, retryAfterMs: number) {
    super(`Circuit breaker '${name}' is open. Retry after ${Math.ceil(retryAfterMs / 1000)}s`);
    this.name = 'CircuitOpenError';
    this.circuitName = name;
    this.retryAfterMs = retryAfterMs;
  }
}


configureCircuit('github:search', {
  failureThreshold: 2,
  successThreshold: 1,
  resetTimeoutMs: 60000,
});

configureCircuit('github:content', {
  failureThreshold: 3,
  successThreshold: 1,
  resetTimeoutMs: 30000,
});

configureCircuit('github:pulls', {
  failureThreshold: 2,
  successThreshold: 1,
  resetTimeoutMs: 60000,
});

configureCircuit('github', {
  failureThreshold: 2,
  successThreshold: 1,
  resetTimeoutMs: 60000,
});

configureCircuit('lsp:navigation', {
  failureThreshold: 3,
  successThreshold: 1,
  resetTimeoutMs: 10000,
});

configureCircuit('lsp:hierarchy', {
  failureThreshold: 2,
  successThreshold: 1,
  resetTimeoutMs: 15000,
});

configureCircuit('lsp', {
  failureThreshold: 3,
  successThreshold: 1,
  resetTimeoutMs: 10000,
});

configureCircuit('local', {
  failureThreshold: 5,
  successThreshold: 1,
  resetTimeoutMs: 5000,
});

configureCircuit('package', {
  failureThreshold: 3,
  successThreshold: 1,
  resetTimeoutMs: 45000,
});


let cleanupIntervalId: ReturnType<typeof setInterval> | null = null;


function cleanupStaleCircuits(): void {
  const now = Date.now();
  const staleCutoff = now - CIRCUIT_TTL_MS;
  let removedCount = 0;

  for (const [name, circuit] of circuits) {
    if (circuit.state !== 'open' && circuit.lastAttempt < staleCutoff) {
      circuits.delete(name);
      configs.delete(name);
      removedCount++;
    }
  }

  if (removedCount > 0) {
    console.log(agentLog(`🧹 Cleaned up ${removedCount} stale circuit(s)`));
  }
}


function startPeriodicCleanup(): void {
  if (cleanupIntervalId) return;

  cleanupIntervalId = setInterval(() => {
    cleanupStaleCircuits();
  }, CLEANUP_INTERVAL_MS);

  if (cleanupIntervalId.unref) {
    cleanupIntervalId.unref();
  }
}


export function stopCircuitCleanup(): void {
  if (cleanupIntervalId) {
    clearInterval(cleanupIntervalId);
    cleanupIntervalId = null;
  }
}

startPeriodicCleanup();


export function clearAllCircuits(): void {
  const count = circuits.size;
  circuits.clear();
  configs.clear();
  console.log(agentLog(`🧹 Cleared ${count} circuit(s)`));
}

