import { spawn, ChildProcess, SpawnOptions } from 'child_process';
import {
  buildChildProcessEnv,
  CORE_ALLOWED_ENV_VARS,
  TOOLING_ALLOWED_ENV_VARS,
} from './env.js';

interface SpawnWithTimeoutOptions {
  timeout?: number;

  cwd?: string;

  env?: Record<string, string | undefined>;

  allowEnvVars?: readonly string[];

  maxOutputSize?: number;
}

interface SpawnResult {
  stdout: string;

  stderr: string;

  exitCode: number | null;

  success: boolean;

  error?: Error;

  timedOut?: boolean;

  outputLimitExceeded?: boolean;
}

interface ProcessState {
  killed: boolean;
  stdoutChunks: string[];
  stderrChunks: string[];
  totalOutputSize: number;
}

const DEFAULT_MAX_OUTPUT_SIZE_BYTES = 10 * 1024 * 1024;

const SIGKILL_GRACE_MS = 5_000;

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

    let sigkillHandle: ReturnType<typeof setTimeout> | undefined;
    const timeoutHandle = setTimeout(() => {
      if (!state.killed) {
        state.killed = true;
        try {
          childProcess.kill('SIGTERM');
        } catch {
          void 0;
        }

        sigkillHandle = setTimeout(() => {
          try {
            childProcess.kill('SIGKILL');
          } catch {
            void 0;
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

    const clearAllTimers = (): void => {
      clearTimeout(timeoutHandle);
      if (sigkillHandle !== undefined) {
        clearTimeout(sigkillHandle);
        sigkillHandle = undefined;
      }
    };

    const checkOutputLimit = (): boolean => {
      if (state.totalOutputSize > maxOutputSize) {
        if (!state.killed) {
          state.killed = true;
          try {
            childProcess.kill('SIGKILL');
          } catch {
            void 0;
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

    childProcess.stdout?.on('data', (data: Buffer) => {
      if (state.killed) return;

      const chunk = data.toString();
      state.totalOutputSize += Buffer.byteLength(chunk);

      if (checkOutputLimit()) return;

      state.stdoutChunks.push(chunk);
    });

    childProcess.stderr?.on('data', (data: Buffer) => {
      if (state.killed) return;

      const chunk = data.toString();
      state.totalOutputSize += Buffer.byteLength(chunk);

      if (checkOutputLimit()) return;

      state.stderrChunks.push(chunk);
    });

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
      resolve(false);
      return;
    }

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
          void 0;
        }

        sigkillHandle = setTimeout(() => {
          try {
            childProcess.kill('SIGKILL');
          } catch {
            void 0;
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
      resolve(null);
      return;
    }

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

      if (totalOutputSize > maxOutputSize) {
        if (!killed) {
          killed = true;
          try {
            childProcess.kill('SIGKILL');
          } catch {
            void 0;
          }
          clearAllTimers();
          resolve(null);
        }
        return;
      }

      stdoutChunks.push(chunk);
    });

    childProcess.stderr?.on('data', () => {});

    const timeoutHandle = setTimeout(() => {
      if (!killed) {
        killed = true;
        try {
          childProcess.kill('SIGTERM');
        } catch {
          void 0;
        }

        sigkillHandle = setTimeout(() => {
          try {
            childProcess.kill('SIGKILL');
          } catch {
            void 0;
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
