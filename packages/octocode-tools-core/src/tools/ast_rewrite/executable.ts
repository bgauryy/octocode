import { access, lstat, readFile, realpath } from 'node:fs/promises';
import { constants } from 'node:fs';
import { createHash } from 'node:crypto';
import { delimiter, join } from 'node:path';
import { spawnWithTimeout } from '../../utils/exec/spawn/wrappers.js';

const MINIMUM_VERSION = [0, 40, 0] as const;
const MAXIMUM_VERSION = [0, 45, Number.MAX_SAFE_INTEGER] as const;
const CAPABILITY_CONTRACT = 1;
const REQUIRED_CAPABILITIES = [
  'color',
  'globs',
  'json',
  'lang',
  'pattern',
  'rewrite',
  'threads',
] as const;

export interface AstGrepExecutable {
  path: string;
  version: string;
  sha256: string;
  capabilityContract: 1;
  capabilityDigest: string;
  capabilities: string[];
}

export type ExecutableResolution =
  | { ok: true; executable: AstGrepExecutable }
  | { ok: false; errorCode: string; error: string };

function candidateNames(): string[] {
  return process.platform === 'win32'
    ? ['ast-grep.exe', 'sg.exe', 'ast-grep.cmd', 'sg.cmd']
    : ['ast-grep', 'sg'];
}

async function validateCandidate(path: string): Promise<string | undefined> {
  try {
    await access(path, constants.X_OK);
    const info = await lstat(path);
    if (!info.isFile() && !info.isSymbolicLink()) return undefined;
    const resolved = await realpath(path);
    const resolvedInfo = await lstat(resolved);
    return resolvedInfo.isFile() ? resolved : undefined;
  } catch {
    return undefined;
  }
}

async function discover(explicit?: string): Promise<string | undefined> {
  if (explicit) return validateCandidate(explicit);
  const dirs = (process.env.PATH ?? '')
    .split(delimiter)
    .map(value => value.trim())
    .filter(Boolean);
  const common =
    process.platform === 'win32'
      ? []
      : ['/opt/homebrew/bin', '/usr/local/bin', '/usr/bin'];
  for (const dir of [...new Set([...dirs, ...common])]) {
    for (const name of candidateNames()) {
      const path = await validateCandidate(join(dir, name));
      if (path) return path;
    }
  }
  return undefined;
}

function parseVersion(output: string): {
  raw: string;
  tuple: [number, number, number];
} | null {
  const match = output.match(/(?:^|\s)v?(\d+)\.(\d+)\.(\d+)(?:[-+\s]|$)/);
  if (!match) return null;
  return {
    raw: `${match[1]}.${match[2]}.${match[3]}`,
    tuple: [Number(match[1]), Number(match[2]), Number(match[3])],
  };
}

function versionAtLeast(
  actual: readonly number[],
  minimum: readonly number[]
): boolean {
  for (let index = 0; index < 3; index += 1) {
    const left = actual[index] ?? 0;
    const right = minimum[index] ?? 0;
    if (left !== right) return left > right;
  }
  return true;
}

function versionAtMost(
  actual: readonly number[],
  maximum: readonly number[]
): boolean {
  for (let index = 0; index < 3; index += 1) {
    const left = actual[index] ?? 0;
    const right = maximum[index] ?? 0;
    if (left !== right) return left < right;
  }
  return true;
}

function sha256(value: string | Buffer): string {
  return createHash('sha256').update(value).digest('hex');
}

export async function resolveAstGrepExecutable(options: {
  explicit?: string;
  timeoutMs: number;
}): Promise<ExecutableResolution> {
  const path = await discover(options.explicit);
  if (!path) {
    return {
      ok: false,
      errorCode: 'ast.rewrite.executable_unavailable',
      error:
        'No executable ast-grep binary was found. Install ast-grep or configure an explicit executable.',
    };
  }

  const result = await spawnWithTimeout(path, ['--version'], {
    timeout: options.timeoutMs,
    maxOutputSize: 64 * 1024,
  });
  const parsed = parseVersion(`${result.stdout}\n${result.stderr}`);
  if (!result.success || !parsed) {
    return {
      ok: false,
      errorCode: 'ast.rewrite.version_unreadable',
      error:
        'The discovered ast-grep executable did not report a valid version.',
    };
  }
  if (
    !versionAtLeast(parsed.tuple, MINIMUM_VERSION) ||
    !versionAtMost(parsed.tuple, MAXIMUM_VERSION)
  ) {
    return {
      ok: false,
      errorCode: 'ast.rewrite.version_incompatible',
      error: `ast-grep ${parsed.raw} is outside the tested 0.40.x–0.45.x compatibility window.`,
    };
  }
  const help = await spawnWithTimeout(path, ['run', '--help'], {
    timeout: options.timeoutMs,
    maxOutputSize: 256 * 1024,
  });
  if (!help.success) {
    return {
      ok: false,
      errorCode: 'ast.rewrite.capability_unreadable',
      error:
        'The discovered ast-grep executable did not expose run capabilities.',
    };
  }
  const helpText = `${help.stdout}\n${help.stderr}`;
  const capabilities: string[] = REQUIRED_CAPABILITIES.filter(capability =>
    helpText.includes(`--${capability}`)
  );
  const missing = REQUIRED_CAPABILITIES.filter(
    capability => !capabilities.includes(capability)
  );
  if (missing.length > 0) {
    return {
      ok: false,
      errorCode: 'ast.rewrite.capability_incompatible',
      error: `ast-grep ${parsed.raw} is missing required run capabilities: ${missing.join(', ')}.`,
    };
  }
  const scanHelp = await spawnWithTimeout(path, ['scan', '--help'], {
    timeout: options.timeoutMs,
    maxOutputSize: 256 * 1024,
  });
  if (
    scanHelp.success &&
    `${scanHelp.stdout}\n${scanHelp.stderr}`.includes('--inline-rules')
  ) {
    capabilities.push('inline-rules');
  }
  let executableBytes: Buffer;
  try {
    executableBytes = await readFile(path);
  } catch {
    return {
      ok: false,
      errorCode: 'ast.rewrite.executable_unreadable',
      error: 'The discovered ast-grep executable could not be attested.',
    };
  }
  const executableSha256 = sha256(executableBytes);
  const capabilityDigest = sha256(
    JSON.stringify({
      contract: CAPABILITY_CONTRACT,
      version: parsed.raw,
      executableSha256,
      capabilities,
    })
  );
  return {
    ok: true,
    executable: {
      path,
      version: parsed.raw,
      sha256: executableSha256,
      capabilityContract: CAPABILITY_CONTRACT,
      capabilityDigest,
      capabilities,
    },
  };
}
