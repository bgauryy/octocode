import { existsSync, mkdirSync, renameSync, rmSync } from 'fs';
import { basename, dirname, join } from 'path';
import { getOctocodeDir } from '../../shared/paths.js';
import { resolveDefaultBranch } from '../../github/client.js';
import { getServerConfig } from '../../serverConfig.js';
import type { AuthInfo } from '@modelcontextprotocol/server';
import type { WithOptionalMeta } from '../../types/execution.js';
import type { CloneRepoQueryLocalSchema } from './scheme.js';
import type { z } from 'zod';
import type { CloneRepoResult } from './types.js';
import { getCloneDir, getCloneLockDir } from './cachePaths.js';
import {
  assertGitAvailable,
  executeCommitClone,
  executeFullClone,
  executeSparseClone,
  readHeadCommit,
} from './gitCheckout.js';
import {
  isCacheHit,
  writeCacheMeta,
  createCacheMeta,
  ensureCloneParentDir,
  removeCloneDir,
  evictExpiredClones,
} from './cache.js';
import {
  writeCloneLockMeta,
  tryRecoverStaleCloneLock,
} from './cacheArtifacts.js';

const CLONE_LOCK_TIMEOUT_MS = 5 * 60 * 1000;

const CLONE_LOCK_POLL_MS = 100;

const CLONE_TEMP_DIR = 'clone-tmp';

export async function cloneRepo(
  query: WithOptionalMeta<z.infer<typeof CloneRepoQueryLocalSchema>>,
  authInfo?: AuthInfo,
  token?: string
): Promise<CloneRepoResult> {
  const owner = query.owner!;
  const repo = query.repo!;
  const { sparsePath, forceRefresh } = query;
  const url = repoUrl(owner, repo);

  await assertGitAvailable();

  const branch =
    query.branch ?? (await resolveDefaultBranch(owner, repo, authInfo));
  const pinnedCommit = /^[a-f0-9]{40}$/i.test(branch)
    ? branch.toLowerCase()
    : undefined;

  const octocodeDir = getOctocodeDir();
  const cloneDir = getCloneDir(
    octocodeDir,
    owner,
    repo,
    branch,
    sparsePath,
    url
  );

  return withCloneLock(octocodeDir, cloneDir, async () => {
    const cacheResult = isCacheHit(cloneDir);
    if (
      !forceRefresh &&
      cacheResult.hit &&
      cacheResult.meta.source === 'clone'
    ) {
      let commitSha: string | undefined;
      try {
        commitSha = await readHeadCommit(cloneDir);
      } catch {
        /* An invalid checkout is rebuilt under the same cache lock. */
      }
      if (commitSha && (!pinnedCommit || commitSha === pinnedCommit)) {
        return {
          localPath: cloneDir,
          cached: true,
          commitSha,
          verified: false,
          owner,
          repo,
          branch,
          ...(sparsePath ? { sparsePath } : {}),
        };
      }
    }

    evictExpiredClones(octocodeDir);
    ensureCloneParentDir(cloneDir);

    const resolvedToken = pickToken(authInfo, token);
    const tempDir = temporaryCloneDir(octocodeDir, cloneDir);
    removeCloneDir(tempDir);

    try {
      if (pinnedCommit) {
        await executeCommitClone(
          pinnedCommit,
          tempDir,
          sparsePath,
          url,
          resolvedToken
        );
      } else if (sparsePath) {
        await executeSparseClone(
          owner,
          repo,
          branch,
          tempDir,
          sparsePath,
          url,
          resolvedToken
        );
      } else {
        await executeFullClone(
          owner,
          repo,
          branch,
          tempDir,
          url,
          resolvedToken
        );
      }

      if (sparsePath && !existsSync(join(tempDir, sparsePath))) {
        throw new Error(
          `sparsePath "${sparsePath}" does not exist in ${owner}/${repo}@${branch} — nothing was checked out for it. ` +
            'Verify the path with ghSearch operation:"tree", then retry with the correct sparsePath (or omit it for a full clone).'
        );
      }

      const commitSha = await readHeadCommit(tempDir);
      if (pinnedCommit && commitSha !== pinnedCommit) {
        throw new Error(
          `Checkout HEAD ${commitSha} does not match requested commit ${branch}.`
        );
      }

      const newMeta = createCacheMeta(owner, repo, branch, 'clone', sparsePath);
      newMeta.commitSha = commitSha;
      writeCacheMeta(tempDir, newMeta);
      promoteCloneDir(tempDir, cloneDir);

      return {
        localPath: cloneDir,
        cached: false,
        commitSha,
        verified: true,
        owner,
        repo,
        branch,
        ...(sparsePath ? { sparsePath } : {}),
      };
    } catch (error) {
      removeCloneDir(tempDir);
      throw error;
    }
  });
}

async function withCloneLock<T>(
  octocodeDir: string,
  cloneDir: string,
  run: () => Promise<T>
): Promise<T> {
  ensureCloneParentDir(cloneDir);
  const lockDir = getCloneLockDir(octocodeDir, cloneDir);
  mkdirSync(dirname(lockDir), {
    recursive: true,
    mode: 0o700,
  });
  const started = Date.now();

  while (true) {
    try {
      mkdirSync(lockDir, { mode: 0o700 });
      writeCloneLockMeta(lockDir);
      break;
    } catch (error) {
      const code = (error as NodeJS.ErrnoException).code;
      if (code !== 'EEXIST') throw error;
      if (tryRecoverStaleCloneLock(lockDir)) continue;
      if (Date.now() - started > CLONE_LOCK_TIMEOUT_MS) {
        throw new Error(`Timed out waiting for clone cache lock '${lockDir}'.`);
      }
      await sleep(CLONE_LOCK_POLL_MS);
    }
  }

  try {
    return await run();
  } finally {
    rmSync(lockDir, { recursive: true, force: true });
  }
}

function temporaryCloneDir(octocodeDir: string, cloneDir: string): string {
  const suffix = `${process.pid}-${Date.now()}-${Math.random()
    .toString(36)
    .slice(2)}`;
  const tmpBase = join(octocodeDir, 'tmp', CLONE_TEMP_DIR);
  mkdirSync(tmpBase, { recursive: true, mode: 0o700 });
  return join(
    tmpBase,
    `${basename(getCloneLockDir(octocodeDir, cloneDir))}-${suffix}`
  );
}

function promoteCloneDir(tempDir: string, cloneDir: string): void {
  ensureCloneParentDir(cloneDir);
  const previousDir = `${tempDir}.previous`;
  const hadPrevious = existsSync(cloneDir);
  if (hadPrevious) renameSync(cloneDir, previousDir);
  try {
    renameSync(tempDir, cloneDir);
  } catch (error) {
    if (hadPrevious) {
      try {
        renameSync(previousDir, cloneDir);
      } catch (restoreError) {
        throw new AggregateError(
          [error, restoreError],
          `Clone publication failed; the previous checkout is preserved at ${previousDir}.`
        );
      }
    }
    throw error;
  }
  if (hadPrevious) removeCloneDir(previousDir);
}

function sleep(ms: number): Promise<void> {
  return new Promise(resolve => setTimeout(resolve, ms));
}

function repoUrl(owner: string, repo: string): string {
  const configured = getServerConfig().githubApiUrl || 'https://api.github.com';
  let endpoint: URL;
  try {
    endpoint = new URL(configured);
  } catch {
    throw unsupportedEndpoint();
  }
  if (
    endpoint.protocol !== 'https:' ||
    endpoint.username ||
    endpoint.password ||
    endpoint.search ||
    endpoint.hash
  ) {
    throw unsupportedEndpoint();
  }
  const path = endpoint.pathname.replace(/\/$/, '');
  if (endpoint.origin === 'https://api.github.com' && !path) {
    return `https://github.com/${owner}/${repo}.git`;
  }
  if (!path.endsWith('/api/v3')) throw unsupportedEndpoint();
  return `${endpoint.origin}${path.slice(0, -'/api/v3'.length)}/${owner}/${repo}.git`;
}

function unsupportedEndpoint(): Error {
  return new Error(
    'ghCloneRepo requires an HTTPS GitHub API endpoint: https://api.github.com or a GitHub Enterprise endpoint ending in /api/v3, without credentials or query parameters. Use ghGetFileContent for other API proxies.'
  );
}

function pickToken(authInfo?: AuthInfo, token?: string): string | undefined {
  if (authInfo?.token && typeof authInfo.token === 'string')
    return authInfo.token;
  return token;
}
