import { dirname, join } from 'path';
import {
  TOOLING_ALLOWED_ENV_VARS,
  PROXY_ENV_VARS,
  spawnWithTimeout,
  spawnCheckSuccess,
  validateArgs,
} from './spawn.js';

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

export async function checkNpmAvailability(
  timeoutMs: number = 10000
): Promise<boolean> {
  return spawnCheckSuccess(
    process.execPath,
    [getNpmScriptPath(), '--version'],
    timeoutMs
  );
}

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
