/**
 * npm and gh CLI execution utilities
 * Uses shared spawn module for process management
 */

import { dirname, join } from 'path';
import {
  TOOLING_ALLOWED_ENV_VARS,
  PROXY_ENV_VARS,
  spawnWithTimeout,
  spawnCheckSuccess,
  validateArgs,
} from './spawn.js';

/**
 * Get the npm script path by looking next to the current node binary.
 * This ensures npm is found even when PATH doesn't include it.
 * On Windows, npm is a batch script (npm.cmd), not a binary.
 */
function getNpmScriptPath(): string {
  const nodeDir = dirname(process.execPath);
  const npmBinary = process.platform === 'win32' ? 'npm.cmd' : 'npm';
  return join(nodeDir, npmBinary);
}

const ALLOWED_NPM_COMMANDS = [
  'view',
  'search',
  'ping',
  'config',
  'whoami',
] as const;

type NpmCommand = (typeof ALLOWED_NPM_COMMANDS)[number];

type NpmExecOptions = {
  timeout?: number;
  cwd?: string;
  env?: Record<string, string>;
};

interface NpmExecResult {
  stdout: string;
  stderr: string;
  error?: Error;
  exitCode?: number;
}

const NETWORK_ALLOWED_ENV_VARS = [
  ...TOOLING_ALLOWED_ENV_VARS,
  ...PROXY_ENV_VARS,
] as const;

/**
 * Check if npm CLI is available by running `npm --version`.
 *
 * Uses process.execPath to invoke the npm script directly, bypassing the
 * shebang (#!/usr/bin/env node). This is critical for GUI-launched
 * environments (e.g. Cursor on macOS) where PATH may not include nvm paths.
 *
 * @param timeoutMs - Timeout in milliseconds (default 10000ms)
 * @returns true if npm CLI is installed and accessible, false otherwise
 */
export async function checkNpmAvailability(
  timeoutMs: number = 10000
): Promise<boolean> {
  return spawnCheckSuccess(
    process.execPath,
    [getNpmScriptPath(), '--version'],
    timeoutMs
  );
}

/**
 * Execute NPM command with security validation using spawn (safer than exec)
 */
export async function executeNpmCommand(
  command: NpmCommand,
  args: string[],
  options: NpmExecOptions = {}
): Promise<NpmExecResult> {
  if (!ALLOWED_NPM_COMMANDS.includes(command)) {
    return {
      stdout: '',
      stderr: '',
      error: new Error(`Command '${command}' is not allowed`),
    };
  }

  const validation = validateArgs(args);
  if (!validation.valid) {
    return {
      stdout: '',
      stderr: '',
      error: new Error(`Invalid arguments: ${validation.error}`),
    };
  }

  const { timeout = 30000, cwd, env } = options;

  // Use process.execPath (node) to invoke the npm script directly.
  // npm's shebang (#!/usr/bin/env node) relies on PATH to find node,
  // which fails in GUI-launched environments (e.g. Cursor on macOS)
  // where PATH may not include nvm/node paths.
  const result = await spawnWithTimeout(
    process.execPath,
    [getNpmScriptPath(), command, ...args],
    {
      timeout,
      cwd,
      env,
      allowEnvVars: NETWORK_ALLOWED_ENV_VARS,
    }
  );

  return {
    stdout: result.stdout,
    stderr: result.stderr,
    exitCode: result.exitCode ?? undefined,
    error: result.error,
  };
}
