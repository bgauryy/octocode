import { spawnSync } from 'node:child_process';
import { realpathSync } from 'node:fs';
import path from 'node:path';

function canonicalPath(input: string): string {
  const resolved = path.resolve(input);
  try {
    return realpathSync(resolved);
  } catch {
    const missingParts: string[] = [];
    let cursor = resolved;
    while (true) {
      const parent = path.dirname(cursor);
      if (parent === cursor) return resolved;
      missingParts.unshift(path.basename(cursor));
      cursor = parent;
      try {
        return path.join(realpathSync(cursor), ...missingParts);
      } catch {
        continue;
      }
    }
  }
}

function resolveTargetPath(file: string, cwd: string): string {
  return canonicalPath(path.isAbsolute(file) ? file : path.resolve(cwd, file));
}

function isInsidePath(candidate: string, root: string): boolean {
  const rel = path.relative(canonicalPath(root), canonicalPath(candidate));
  return rel === '' || Boolean(rel && !rel.startsWith('..') && !path.isAbsolute(rel));
}

function gitBranchOf(dir: string): string | null {
  try {
    const result = spawnSync('git', ['-C', dir, 'rev-parse', '--abbrev-ref', 'HEAD'], {
      encoding: 'utf8',
      timeout: 5000,
    });
    return result.status === 0 ? String(result.stdout).trim() : null;
  } catch {
    return null;
  }
}

/** Returns a human-readable block reason when a shell hook targets its own skill. */
export function evaluateHarnessGuard(params: {
  targetFiles: string[];
  skillRoot: string | null | undefined;
  cwd: string;
  env?: NodeJS.ProcessEnv;
}): string | null {
  const { targetFiles, skillRoot, cwd } = params;
  const env = params.env ?? process.env;
  if (!skillRoot || targetFiles.length === 0) return null;
  if (!targetFiles.some(file => isInsidePath(resolveTargetPath(file, cwd), skillRoot))) return null;

  if (env.OCTOCODE_ALLOW_HARNESS_APPLY !== '1') {
    return 'octocode-awareness: editing the skill itself is gated. A human must set OCTOCODE_ALLOW_HARNESS_APPLY=1.';
  }

  const branch = gitBranchOf(skillRoot);
  if (branch === 'main' || branch === 'master') {
    return `octocode-awareness: harness self-fix is never allowed on ${branch}. Create a dedicated branch first.`;
  }
  if ((!branch || branch === 'HEAD') && env.OCTOCODE_HARNESS_BRANCH_OK !== '1') {
    return 'octocode-awareness: cannot confirm a dedicated git branch for the skill. Create one, or set OCTOCODE_HARNESS_BRANCH_OK=1 to acknowledge.';
  }

  return null;
}
