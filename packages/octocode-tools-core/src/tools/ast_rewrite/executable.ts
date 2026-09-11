import { access, lstat, realpath } from 'node:fs/promises';
import { constants } from 'node:fs';
import { delimiter, join } from 'node:path';
import { spawnWithTimeout } from '../../utils/exec/spawn/wrappers.js';

const MINIMUM_VERSION = [0, 30, 0] as const;
const MAXIMUM_MAJOR_EXCLUSIVE = 1;

export interface AstGrepExecutable {
  path: string;
  version: string;
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
    parsed.tuple[0] >= MAXIMUM_MAJOR_EXCLUSIVE ||
    !versionAtLeast(parsed.tuple, MINIMUM_VERSION)
  ) {
    return {
      ok: false,
      errorCode: 'ast.rewrite.version_incompatible',
      error: `ast-grep ${parsed.raw} is incompatible; expected >=0.30.0 and <1.0.0.`,
    };
  }
  return { ok: true, executable: { path, version: parsed.raw } };
}
