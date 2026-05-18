/**
 * Core logic for cloning / partially fetching a GitHub repository.
 *
 * ## Full clone (default)
 *   git clone --depth 1 --single-branch --branch {branch} {url} {dir}
 *
 * ## Partial fetch (sparse_path provided)
 *   git clone --filter=blob:none --sparse --depth 1 --single-branch \
 *       --branch {branch} {url} {dir}
 *   cd {dir} && git sparse-checkout set -- {sparse_path}
 *
 * Authentication is passed via `http.extraHeader` so no token
 * is persisted in the remote URL or on-disk git config.
 *
 * Results are cached for 24 hours under ~/.octocode/repos/.
 *
 * SECURITY NOTES:
 * - owner/repo/branch/sparse_path are validated by Zod schema (scheme.ts)
 *   to reject path-traversal patterns (.., /, \) and flag injection (leading -)
 * - Token is NEVER passed to local-only commands (sparse-checkout)
 * - Token is scrubbed from any error messages before surfacing
 * - `--` separator is used before user-controlled paths in git args
 */

import { getOctocodeDir } from 'octocode-shared';
import { resolveDefaultBranch } from '../../github/client.js';
import type { AuthInfo } from '@modelcontextprotocol/sdk/server/auth/types.js';
import {
  spawnWithTimeout,
  TOOLING_ALLOWED_ENV_VARS,
} from '../../utils/exec/spawn.js';
import type { CloneRepoQuery } from '@octocodeai/octocode-core';
import type { WithOptionalMeta } from '../../types/execution.js';
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

/** Timeout for the git clone operation (2 min) */
const CLONE_TIMEOUT_MS = 2 * 60 * 1000;

/** Timeout for git sparse-checkout set (30 s) */
const SPARSE_CHECKOUT_TIMEOUT_MS = 30 * 1000;

/** Env vars allowed for git child processes */
const GIT_ALLOWED_ENV_VARS = [
  ...TOOLING_ALLOWED_ENV_VARS,
  'GIT_TERMINAL_PROMPT',
] as const;

/**
 * Clone (or return from cache) a GitHub repository.
 *
 * @param query  - Clone parameters (owner, repo, branch, sparse_path)
 * @param authInfo - MCP OAuth auth info (preferred)
 * @param token  - Fallback token from provider config
 * @returns      - Local path, cache status, expiry
 */
export async function cloneRepo(
  query: WithOptionalMeta<CloneRepoQuery>,
  authInfo?: AuthInfo,
  token?: string
): Promise<CloneRepoResult> {
  // owner and repo are required by Zod schema at the call site; assert here for TypeScript.
  const owner = query.owner!;
  const repo = query.repo!;
  const { sparse_path, forceRefresh } = query;

  await assertGitAvailable();

  const branch =
    query.branch ?? (await resolveDefaultBranch(owner, repo, authInfo));

  const octocodeDir = getOctocodeDir();
  const cloneDir = getCloneDir(octocodeDir, owner, repo, branch, sparse_path);

  // Skip the cache entirely when the caller requests a forced refresh.
  // Only accept cache from a real clone, not from a directoryFetch.
  const cacheResult = isCacheHit(cloneDir);
  if (!forceRefresh && cacheResult.hit && cacheResult.meta.source === 'clone') {
    return {
      localPath: cloneDir,
      cached: true,
      owner,
      repo,
      branch,
      ...(sparse_path ? { sparse_path } : {}),
    };
  }

  evictExpiredClones(octocodeDir);
  removeCloneDir(cloneDir);
  ensureCloneParentDir(cloneDir);

  const resolvedToken = pickToken(authInfo, token);

  if (sparse_path) {
    await executeSparseClone(
      owner,
      repo,
      branch,
      cloneDir,
      sparse_path,
      resolvedToken
    );
  } else {
    await executeFullClone(owner, repo, branch, cloneDir, resolvedToken);
  }

  const newMeta = createCacheMeta(owner, repo, branch, 'clone', sparse_path);
  writeCacheMeta(cloneDir, newMeta);

  return {
    localPath: cloneDir,
    cached: false,
    owner,
    repo,
    branch,
    ...(sparse_path ? { sparse_path } : {}),
  };
}

/**
 * Full shallow clone: `git clone --depth 1 --single-branch`
 */
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
    '--', // end of flags — URL and dir are positional
    repoUrl(owner, repo),
    targetDir
  );
  await runGit(args, CLONE_TIMEOUT_MS, `full clone of ${owner}/${repo}`, token);
}

/**
 * Sparse clone: blob-less clone + sparse-checkout for a specific path.
 *
 *   git clone --filter=blob:none --sparse --depth 1 ...
 *   git -C {dir} sparse-checkout set -- {path}
 */
async function executeSparseClone(
  owner: string,
  repo: string,
  branch: string,
  targetDir: string,
  sparsePath: string,
  token?: string
): Promise<void> {
  // Step 1 – blob-less shallow clone (downloads metadata only)
  // Auth IS needed here (network operation).
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
    '--', // end of flags
    repoUrl(owner, repo),
    targetDir
  );
  await runGit(
    cloneArgs,
    CLONE_TIMEOUT_MS,
    `sparse clone of ${owner}/${repo}`,
    token
  );

  // Step 2 – check out only the requested subtree
  // NOTE: No auth token here — sparse-checkout is a local-only operation.
  const sparseArgs: string[] = [
    '-C',
    targetDir,
    'sparse-checkout',
    'set',
    '--', // SECURITY: prevent flag injection from sparsePath
    sparsePath,
  ];
  await runGit(
    sparseArgs,
    SPARSE_CHECKOUT_TIMEOUT_MS,
    `sparse-checkout set ${sparsePath}`,
    undefined // no token for local operation
  );
}

function repoUrl(owner: string, repo: string): string {
  return `https://github.com/${owner}/${repo}.git`;
}

/**
 * Build the `git -c http.extraHeader=…` prefix for auth.
 * Returns an empty array when no token is available (public repos).
 */
function buildAuthArgs(token?: string): string[] {
  if (!token) return [];
  return ['-c', `http.extraHeader=Authorization: Bearer ${token}`];
}

/**
 * Pick the best available token.
 * Priority: authInfo.token > explicit token > none (public repo).
 */
function pickToken(authInfo?: AuthInfo, token?: string): string | undefined {
  if (authInfo?.token && typeof authInfo.token === 'string')
    return authInfo.token;
  return token;
}

/**
 * Verify that git is available on PATH.
 * Throws a clear error message if not.
 */
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
        'The githubCloneRepo tool requires git to be available.'
    );
  }
}

/**
 * Scrub auth tokens from git error output before surfacing.
 * Prevents token leakage via stderr in error messages.
 */
function scrubToken(text: string, token?: string): string {
  let scrubbed = text;
  // Remove the specific token value if known
  if (token) {
    scrubbed = scrubbed.replaceAll(token, '[REDACTED]');
  }
  // Also scrub any Authorization headers in case git formats them differently
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

/**
 * Execute a git command with timeout and env restrictions.
 * Throws on non-zero exit. Token is scrubbed from error messages.
 */
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
