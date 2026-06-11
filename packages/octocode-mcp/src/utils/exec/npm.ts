import { existsSync } from 'fs';
import { delimiter, dirname, join } from 'path';
import {
  TOOLING_ALLOWED_ENV_VARS,
  PROXY_ENV_VARS,
  spawnWithTimeout,
  spawnCheckSuccess,
  validateArgs,
} from './spawn.js';

type NpmInvocation = {
  command: string;
  argsPrefix: string[];
};

function npmBinaryName(): string {
  return process.platform === 'win32' ? 'npm.cmd' : 'npm';
}

function getNpmScriptPath(): string {
  const nodeDir = dirname(process.execPath);
  return join(nodeDir, npmBinaryName());
}

function commonNpmSearchDirs(): string[] {
  if (process.platform === 'win32') return [];
  return ['/opt/homebrew/bin', '/usr/local/bin', '/usr/bin'];
}

function uniqueSearchDirs(): string[] {
  const pathDirs = (process.env.PATH ?? '')
    .split(delimiter)
    .map(dir => dir.trim())
    .filter(Boolean);
  return [...new Set([...pathDirs, ...commonNpmSearchDirs()])];
}

function resolveNpmInvocation(): NpmInvocation {
  const siblingNpmScript = getNpmScriptPath();
  if (existsSync(siblingNpmScript)) {
    return {
      command: process.execPath,
      argsPrefix: [siblingNpmScript],
    };
  }

  const npmBinary = npmBinaryName();
  for (const dir of uniqueSearchDirs()) {
    const candidate = join(dir, npmBinary);
    if (existsSync(candidate)) {
      return {
        command: candidate,
        argsPrefix: [],
      };
    }
  }

  return {
    command: npmBinary,
    argsPrefix: [],
  };
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
  const invocation = resolveNpmInvocation();
  return spawnCheckSuccess(
    invocation.command,
    [...invocation.argsPrefix, '--version'],
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
  const invocation = resolveNpmInvocation();

  const result = await spawnWithTimeout(
    invocation.command,
    [...invocation.argsPrefix, command, ...args],
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
