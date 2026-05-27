/**
 * Core spawn utilities for command execution
 * Provides common patterns for timeout handling, output collection, and process management
 */

import { spawn, ChildProcess, SpawnOptions } from 'child_process';

/**
 * Base options for spawn-with-timeout operations
 */
interface SpawnWithTimeoutOptions {
  /** Command timeout in milliseconds (default: 30000) */
  timeout?: number;
  /** Working directory */
  cwd?: string;
  /** Environment variables to merge with process.env */
  env?: Record<string, string | undefined>;
  /** Environment variables to allow from process.env (opt-in) */
  allowEnvVars?: readonly string[];
  /** Maximum output size in bytes before killing process (default: 10MB) */
  maxOutputSize?: number;
}

/**
 * Legacy denylist kept for audit visibility.
 * Process spawning now uses allowlist-only env propagation.
 */
export const SENSITIVE_ENV_VARS = [
  'NODE_OPTIONS',
  'GITHUB_TOKEN',
  'GH_TOKEN',
  'OCTOCODE_TOKEN',
  'GITHUB_PERSONAL_ACCESS_TOKEN',
  'NPM_TOKEN',
  'AWS_SECRET_ACCESS_KEY',
  'AWS_SESSION_TOKEN',
] as const;

/**
 * Default allowlist for environment variables propagated to child processes.
 * All other variables are blocked unless explicitly provided via options.env.
 */
export const CORE_ALLOWED_ENV_VARS = [
  // Command resolution/runtime
  'PATH',
  // Temp directories
  'TMPDIR',
  'TMP',
  'TEMP',
  // Windows command/runtime support
  'SYSTEMROOT',
  'WINDIR',
  'COMSPEC',
  'PATHEXT',
] as const;

/**
 * User profile variables needed by tooling that reads local config/state
 * (e.g., gh auth, npm user config, some language servers).
 */
export const TOOLING_ALLOWED_ENV_VARS = [
  ...CORE_ALLOWED_ENV_VARS,
  'HOME',
  'USERPROFILE',
  'APPDATA',
  'LOCALAPPDATA',
] as const;

/**
 * Proxy env vars are intentionally NOT included in core/default allowlists.
 * Commands that truly need network proxy support must opt in explicitly.
 */
export const PROXY_ENV_VARS = [
  'HTTP_PROXY',
  'HTTPS_PROXY',
  'NO_PROXY',
  'http_proxy',
  'https_proxy',
  'no_proxy',
] as const;

const DEFAULT_MAX_OUTPUT_SIZE_BYTES = 10 * 1024 * 1024;

/**
 * Grace period before escalating from SIGTERM to SIGKILL.
 * 5 seconds matches execa's default forceKillAfterDelay.
 * Processes (especially git) can ignore SIGTERM; SIGKILL is non-catchable.
 */
const SIGKILL_GRACE_MS = 5_000;

export function buildChildProcessEnv(
  envOverrides: Record<string, string | undefined> = {},
  allowEnvVars: readonly string[] = CORE_ALLOWED_ENV_VARS
): typeof process.env {
  const childEnv: Record<string, string | undefined> = {};

  for (const key of allowEnvVars) {
    const value = process.env[key];
    if (value !== undefined) {
      childEnv[key] = value;
    }
  }

  // Explicit caller overrides are gated by the same allowlist — only keys
  // present in allowEnvVars can be set/deleted. This prevents accidentally
  // leaking sensitive variables (e.g. GITHUB_TOKEN) into child processes.
  // Callers that need non-standard env vars must add them to allowEnvVars.
  const allowSet = new Set<string>(allowEnvVars);
  for (const [key, value] of Object.entries(envOverrides)) {
    if (!allowSet.has(key)) continue;
    if (value === undefined) {
      delete childEnv[key];
    } else {
      childEnv[key] = value;
    }
  }

  return childEnv as typeof process.env;
}

/**
 * Result from spawn-with-timeout execution
 */
interface SpawnResult {
  /** Collected stdout */
  stdout: string;
  /** Collected stderr */
  stderr: string;
  /** Exit code (null if process was killed) */
  exitCode: number | null;
  /** Whether the process completed successfully (exit code 0) */
  success: boolean;
  /** Error if spawn failed or process was killed */
  error?: Error;
  /** Whether the process was killed due to timeout */
  timedOut?: boolean;
  /** Whether the process was killed due to output size limit */
  outputLimitExceeded?: boolean;
}

/**
 * Internal state for tracking process execution
 */
interface ProcessState {
  killed: boolean;
  stdoutChunks: string[];
  stderrChunks: string[];
  totalOutputSize: number;
}

/**
 * Spawn a command with timeout and output collection
 * This is the core shared functionality used by both npm/gh execution and local safe execution
 *
 * @param command - The command to spawn
 * @param args - Command arguments
 * @param options - Spawn options including timeout and output limits
 * @returns Promise resolving to SpawnResult
 */
export function spawnWithTimeout(
  command: string,
  args: string[],
  options: SpawnWithTimeoutOptions = {}
): Promise<SpawnResult> {
  const {
    timeout = 30000,
    cwd,
    env = {},
    allowEnvVars = CORE_ALLOWED_ENV_VARS,
    maxOutputSize = DEFAULT_MAX_OUTPUT_SIZE_BYTES,
  } = options;

  return new Promise(resolve => {
    const state: ProcessState = {
      killed: false,
      stdoutChunks: [],
      stderrChunks: [],
      totalOutputSize: 0,
    };

    const getStdout = (): string => state.stdoutChunks.join('');
    const getStderr = (): string => state.stderrChunks.join('');

    // Spawn options — timeout is intentionally NOT passed here.
    // spawnWithTimeout manages its own timeout via setTimeout with
    // SIGTERM → SIGKILL escalation. Passing timeout to spawn() would
    // create a double-timeout race condition where Node's built-in
    // timeout also sends SIGTERM at the same instant.
    const spawnOptions: SpawnOptions = {
      cwd,
      env: buildChildProcessEnv(env, allowEnvVars),
      stdio: ['ignore', 'pipe', 'pipe'],
    };

    let childProcess: ChildProcess;

    try {
      childProcess = spawn(command, args, spawnOptions);
    } catch (error) {
      resolve({
        stdout: '',
        stderr: '',
        exitCode: null,
        success: false,
        error:
          error instanceof Error
            ? error
            : new Error(`Failed to spawn command '${command}'`),
      });
      return;
    }

    // Timeout handling with SIGTERM → SIGKILL escalation.
    // Some processes (e.g. git during network ops) can ignore SIGTERM,
    // so we escalate to non-catchable SIGKILL after a grace period.
    let sigkillHandle: ReturnType<typeof setTimeout> | undefined;
    const timeoutHandle = setTimeout(() => {
      if (!state.killed) {
        state.killed = true;
        try {
          childProcess.kill('SIGTERM');
        } catch {
          // Process may have already exited (ESRCH) — ignore
        }

        // Escalate to SIGKILL if process doesn't exit after grace period
        sigkillHandle = setTimeout(() => {
          try {
            childProcess.kill('SIGKILL');
          } catch {
            // Process may have already exited — ignore
          }
        }, SIGKILL_GRACE_MS);

        resolve({
          stdout: getStdout(),
          stderr: getStderr(),
          exitCode: null,
          success: false,
          error: new Error(`Command timeout after ${timeout}ms`),
          timedOut: true,
        });
      }
    }, timeout);

    /** Clear all pending timers (timeout + SIGKILL escalation) */
    const clearAllTimers = (): void => {
      clearTimeout(timeoutHandle);
      if (sigkillHandle !== undefined) {
        clearTimeout(sigkillHandle);
        sigkillHandle = undefined;
      }
    };

    // Helper to check and handle output size limit
    const checkOutputLimit = (): boolean => {
      if (state.totalOutputSize > maxOutputSize) {
        if (!state.killed) {
          state.killed = true;
          try {
            childProcess.kill('SIGKILL');
          } catch {
            // Process may have already exited — ignore
          }
          clearAllTimers();
          resolve({
            stdout: getStdout(),
            stderr: getStderr(),
            exitCode: null,
            success: false,
            error: new Error('Output size limit exceeded'),
            outputLimitExceeded: true,
          });
        }
        return true;
      }
      return false;
    };

    // Collect stdout
    childProcess.stdout?.on('data', (data: Buffer) => {
      if (state.killed) return;

      const chunk = data.toString();
      state.totalOutputSize += Buffer.byteLength(chunk);

      if (checkOutputLimit()) return;

      state.stdoutChunks.push(chunk);
    });

    // Collect stderr
    childProcess.stderr?.on('data', (data: Buffer) => {
      if (state.killed) return;

      const chunk = data.toString();
      state.totalOutputSize += Buffer.byteLength(chunk);

      if (checkOutputLimit()) return;

      state.stderrChunks.push(chunk);
    });

    // Handle process close
    childProcess.on('close', code => {
      if (state.killed) return;

      clearAllTimers();

      resolve({
        stdout: getStdout(),
        stderr: getStderr(),
        exitCode: code,
        success: code === 0,
      });
    });

    // Handle spawn errors
    childProcess.on('error', error => {
      if (state.killed) return;

      state.killed = true;
      clearAllTimers();

      resolve({
        stdout: getStdout(),
        stderr: getStderr(),
        exitCode: null,
        success: false,
        error,
      });
    });
  });
}

/**
 * Simple spawn that resolves to boolean success (for availability checks)
 * Used for quick command availability verification
 *
 * @param command - The command to check
 * @param args - Command arguments
 * @param timeoutMs - Timeout in milliseconds (default: 10000)
 * @returns Promise resolving to true if command exits with code 0
 */
export function spawnCheckSuccess(
  command: string,
  args: string[],
  timeoutMs: number = 10000,
  options: { allowEnvVars?: readonly string[] } = {}
): Promise<boolean> {
  return new Promise(resolve => {
    let killed = false;
    const { allowEnvVars = CORE_ALLOWED_ENV_VARS } = options;

    let childProcess: ChildProcess;
    try {
      childProcess = spawn(command, args, {
        stdio: ['ignore', 'pipe', 'pipe'],
        timeout: timeoutMs,
        env: buildChildProcessEnv({}, allowEnvVars),
      });
    } catch {
      // spawn() failed before a child existed; treat as command unavailable.
      resolve(false);
      return;
    }

    // SIGTERM → SIGKILL escalation (matches spawnWithTimeout pattern).
    // Processes that ignore SIGTERM (e.g. git during network ops) would
    // become zombies without escalation to the non-catchable SIGKILL.
    let sigkillHandle: ReturnType<typeof setTimeout> | undefined;

    const clearAllTimers = (): void => {
      clearTimeout(timeoutHandle);
      if (sigkillHandle !== undefined) {
        clearTimeout(sigkillHandle);
        sigkillHandle = undefined;
      }
    };

    const timeoutHandle = setTimeout(() => {
      if (!killed) {
        killed = true;
        try {
          childProcess.kill('SIGTERM');
        } catch {
          // Process may have already exited — ignore
        }

        // Escalate to SIGKILL if process doesn't exit after grace period
        sigkillHandle = setTimeout(() => {
          try {
            childProcess.kill('SIGKILL');
          } catch {
            // Process may have already exited — ignore
          }
        }, SIGKILL_GRACE_MS);

        resolve(false);
      }
    }, timeoutMs);

    childProcess.on('close', code => {
      clearAllTimers();
      if (!killed) {
        resolve(code === 0);
      }
    });

    childProcess.on('error', () => {
      clearAllTimers();
      if (!killed) {
        resolve(false);
      }
    });
  });
}

/**
 * Simple spawn that collects stdout and resolves to trimmed string or null
 * Used for commands like `gh auth token` that return a single value
 *
 * @param command - The command to run
 * @param args - Command arguments
 * @param timeoutMs - Timeout in milliseconds (default: 10000)
 * @returns Promise resolving to trimmed stdout or null on failure/empty
 */
/**
 * Default max output size for spawnCollectStdout (1MB).
 * This is intentionally smaller than spawnWithTimeout (10MB) because
 * spawnCollectStdout is used for single-value commands like `gh auth token`.
 */
const COLLECT_STDOUT_MAX_OUTPUT_SIZE = 1 * 1024 * 1024;

export function spawnCollectStdout(
  command: string,
  args: string[],
  timeoutMs: number = 10000,
  options: {
    allowEnvVars?: readonly string[];
    maxOutputSize?: number;
  } = {}
): Promise<string | null> {
  return new Promise(resolve => {
    let killed = false;
    const stdoutChunks: string[] = [];
    let totalOutputSize = 0;
    const {
      allowEnvVars = TOOLING_ALLOWED_ENV_VARS,
      maxOutputSize = COLLECT_STDOUT_MAX_OUTPUT_SIZE,
    } = options;

    let childProcess: ChildProcess;
    try {
      childProcess = spawn(command, args, {
        stdio: ['ignore', 'pipe', 'pipe'],
        timeout: timeoutMs,
        env: buildChildProcessEnv({}, allowEnvVars),
      });
    } catch {
      // spawn() failed; no process to read stdout from.
      resolve(null);
      return;
    }

    // SIGTERM → SIGKILL escalation (matches spawnWithTimeout pattern)
    let sigkillHandle: ReturnType<typeof setTimeout> | undefined;

    const clearAllTimers = (): void => {
      clearTimeout(timeoutHandle);
      if (sigkillHandle !== undefined) {
        clearTimeout(sigkillHandle);
        sigkillHandle = undefined;
      }
    };

    childProcess.stdout?.on('data', (data: Buffer) => {
      if (killed) return;
      const chunk = data.toString();
      totalOutputSize += Buffer.byteLength(chunk);

      // OOM protection: kill process if output exceeds limit
      if (totalOutputSize > maxOutputSize) {
        if (!killed) {
          killed = true;
          try {
            childProcess.kill('SIGKILL');
          } catch {
            // Process may have already exited — ignore
          }
          clearAllTimers();
          resolve(null);
        }
        return;
      }

      stdoutChunks.push(chunk);
    });

    childProcess.stderr?.on('data', () => {
      // Ignore stderr
    });

    const timeoutHandle = setTimeout(() => {
      if (!killed) {
        killed = true;
        try {
          childProcess.kill('SIGTERM');
        } catch {
          // Process may have already exited — ignore
        }

        // Escalate to SIGKILL if process doesn't exit after grace period
        sigkillHandle = setTimeout(() => {
          try {
            childProcess.kill('SIGKILL');
          } catch {
            // Process may have already exited — ignore
          }
        }, SIGKILL_GRACE_MS);

        resolve(null);
      }
    }, timeoutMs);

    childProcess.on('close', code => {
      clearAllTimers();
      if (!killed) {
        if (code === 0) {
          const trimmed = stdoutChunks.join('').trim();
          resolve(trimmed || null);
        } else {
          resolve(null);
        }
      }
    });

    childProcess.on('error', () => {
      clearAllTimers();
      if (!killed) {
        resolve(null);
      }
    });
  });
}

/**
 * Validate command arguments for common security issues
 * @param args - Arguments to validate
 * @param maxLength - Maximum allowed length for each argument (default: 1000)
 * @returns Validation result with error message if invalid
 */
export function validateArgs(
  args: string[],
  maxLength: number = 1000
): { valid: boolean; error?: string } {
  for (const arg of args) {
    if (arg.includes('\0')) {
      return { valid: false, error: 'Null bytes not allowed in arguments' };
    }

    if (arg.length > maxLength) {
      return { valid: false, error: 'Argument too long' };
    }
  }

  return { valid: true };
}
