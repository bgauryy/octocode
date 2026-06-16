import { existsSync } from 'fs';
import { join } from 'path';
import { getOctocodeDir } from 'octocode-shared';
import { resolveDefaultBranch } from '../../github/client.js';
import type { AuthInfo } from '@modelcontextprotocol/sdk/server/auth/types.js';
import {
  spawnWithTimeout,
  TOOLING_ALLOWED_ENV_VARS,
} from '../../utils/exec/spawn.js';
import type { WithOptionalMeta } from '../../types/execution.js';
import type { CloneRepoQueryLocalSchema } from './scheme.js';
import type { z } from 'zod';
import type { CloneRepoResult } from './types.js';
import {
  getCloneDir,
  isCacheHit,
  writeCacheMeta,
  createCacheMeta,
  ensureCloneParentDir,
  removeCloneDir,
  evictExpiredClones,
} from './cache.js';

const CLONE_TIMEOUT_MS = 2 * 60 * 1000;

const SPARSE_CHECKOUT_TIMEOUT_MS = 30 * 1000;

const GIT_ALLOWED_ENV_VARS = [
  ...TOOLING_ALLOWED_ENV_VARS,
  'GIT_TERMINAL_PROMPT',
] as const;

export async function cloneRepo(
  query: WithOptionalMeta<z.infer<typeof CloneRepoQueryLocalSchema>>,
  authInfo?: AuthInfo,
  token?: string
): Promise<CloneRepoResult> {
  const owner = query.owner!;
  const repo = query.repo!;
  const { sparsePath, forceRefresh } = query;

  await assertGitAvailable();

  const branch =
    query.branch ?? (await resolveDefaultBranch(owner, repo, authInfo));

  const octocodeDir = getOctocodeDir();
  const cloneDir = getCloneDir(octocodeDir, owner, repo, branch, sparsePath);

  const cacheResult = isCacheHit(cloneDir);
  if (!forceRefresh && cacheResult.hit && cacheResult.meta.source === 'clone') {
    return {
      localPath: cloneDir,
      cached: true,
      owner,
      repo,
      branch,
      ...(sparsePath ? { sparsePath } : {}),
    };
  }

  evictExpiredClones(octocodeDir);
  removeCloneDir(cloneDir);
  ensureCloneParentDir(cloneDir);

  const resolvedToken = pickToken(authInfo, token);

  if (sparsePath) {
    await executeSparseClone(
      owner,
      repo,
      branch,
      cloneDir,
      sparsePath,
      resolvedToken
    );
    if (!existsSync(join(cloneDir, sparsePath))) {
      removeCloneDir(cloneDir); // don't cache a clone missing its sparse dir
      throw new Error(
        `sparsePath "${sparsePath}" does not exist in ${owner}/${repo}@${branch} — nothing was checked out for it. ` +
          'Verify the directory path with ghViewRepoStructure, then retry with the correct sparsePath (or omit it for a full clone).'
      );
    }
  } else {
    await executeFullClone(owner, repo, branch, cloneDir, resolvedToken);
  }

  const newMeta = createCacheMeta(owner, repo, branch, 'clone', sparsePath);
  writeCacheMeta(cloneDir, newMeta);

  return {
    localPath: cloneDir,
    cached: false,
    owner,
    repo,
    branch,
    ...(sparsePath ? { sparsePath } : {}),
  };
}

async function executeFullClone(
  owner: string,
  repo: string,
  branch: string,
  targetDir: string,
  token?: string
): Promise<void> {
  const args = buildAuthArgs(token);
  args.push(
    'clone',
    '--depth',
    '1',
    '--single-branch',
    '--branch',
    branch,
    '--',
    repoUrl(owner, repo),
    targetDir
  );
  await runGit(args, CLONE_TIMEOUT_MS, `full clone of ${owner}/${repo}`, token);
}

async function executeSparseClone(
  owner: string,
  repo: string,
  branch: string,
  targetDir: string,
  sparsePath: string,
  token?: string
): Promise<void> {
  const cloneArgs = buildAuthArgs(token);
  cloneArgs.push(
    'clone',
    '--filter',
    'blob:none',
    '--sparse',
    '--depth',
    '1',
    '--single-branch',
    '--branch',
    branch,
    '--',
    repoUrl(owner, repo),
    targetDir
  );
  await runGit(
    cloneArgs,
    CLONE_TIMEOUT_MS,
    `sparse clone of ${owner}/${repo}`,
    token
  );

  const sparseArgs: string[] = [
    '-C',
    targetDir,
    'sparse-checkout',
    'set',
    '--',
    sparsePath,
  ];
  await runGit(
    sparseArgs,
    SPARSE_CHECKOUT_TIMEOUT_MS,
    `sparse-checkout set ${sparsePath}`,
    undefined
  );
}

function repoUrl(owner: string, repo: string): string {
  return `https://github.com/${owner}/${repo}.git`;
}

function buildAuthArgs(token?: string): string[] {
  if (!token) return [];
  return ['-c', `http.extraHeader=Authorization: Bearer ${token}`];
}

function pickToken(authInfo?: AuthInfo, token?: string): string | undefined {
  if (authInfo?.token && typeof authInfo.token === 'string')
    return authInfo.token;
  return token;
}

async function assertGitAvailable(): Promise<void> {
  try {
    const result = await spawnWithTimeout('git', ['--version'], {
      timeout: 5_000,
      maxOutputSize: 1024,
      allowEnvVars: GIT_ALLOWED_ENV_VARS,
      env: { GIT_TERMINAL_PROMPT: '0' },
    });
    if (!result.success) {
      throw new Error('git --version returned non-zero');
    }
  } catch {
    throw new Error(
      'git is not installed or not on PATH. ' +
        'The ghCloneRepo tool requires git to be available.'
    );
  }
}

function scrubToken(text: string, token?: string): string {
  let scrubbed = text;
  if (token) {
    scrubbed = scrubbed.replaceAll(token, '[REDACTED]');
  }
  scrubbed = scrubbed.replace(
    /Authorization:\s*Bearer\s+\S+/gi,
    'Authorization: Bearer [REDACTED]'
  );
  scrubbed = scrubbed.replace(
    /Authorization:\s*token\s+\S+/gi,
    'Authorization: token [REDACTED]'
  );
  return scrubbed;
}

async function runGit(
  args: string[],
  timeout: number,
  label: string,
  token?: string
): Promise<void> {
  const result = await spawnWithTimeout('git', args, {
    timeout,
    maxOutputSize: 5 * 1024 * 1024,
    allowEnvVars: GIT_ALLOWED_ENV_VARS,
    env: { GIT_TERMINAL_PROMPT: '0' },
  });

  if (!result.success) {
    const stderr = scrubToken(result.stderr?.trim() || '', token);
    const suffix = stderr ? `: ${stderr}` : '';
    throw new Error(`git ${label} failed${suffix}`);
  }
}
