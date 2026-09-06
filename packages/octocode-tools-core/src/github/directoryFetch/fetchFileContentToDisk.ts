import {
  writeFileSync,
  existsSync,
  statSync,
  lstatSync,
  rmSync,
} from 'node:fs';
import { join, dirname, resolve, sep } from 'node:path';
import { getOctocodeDir } from '../../shared/paths.js';
import type { AuthInfo } from '@modelcontextprotocol/server';
import type { FileMaterializationResult } from '../../tools/github_fetch_content/types.js';
import { fetchCachedRawGitHubFileContent } from '../fileContentRaw/cache.js';
import {
  isCacheHit,
  createCacheMeta,
  evictExpiredTrees,
} from '../../tools/github_clone_repo/cache.js';
import { getTreeDir } from '../../tools/github_clone_repo/cachePaths.js';
import { safeFileSize } from './helpers.js';
import {
  currentTreeSnapshot,
  publishTreeSnapshot,
  withTreeLock,
  ensureSnapshotDirectory,
} from './snapshot.js';
import { resolveMaterializationRef } from './refResolution.js';

export async function fetchFileContentToDisk(
  owner: string,
  repo: string,
  path: string,
  branch: string,
  authInfo?: AuthInfo,
  forceRefresh = false
): Promise<FileMaterializationResult> {
  const octocodeDir = getOctocodeDir();
  const { commitSha, resolvedRef } = await resolveMaterializationRef(
    owner,
    repo,
    branch,
    authInfo,
    forceRefresh
  );
  const cacheRoot = getTreeDir(octocodeDir, owner, repo, commitSha);
  evictExpiredTrees(octocodeDir);
  return withTreeLock(octocodeDir, cacheRoot, async () => {
    const previousRoot = currentTreeSnapshot(cacheRoot);
    let treeRoot = previousRoot ?? cacheRoot;
    let filePath = resolve(join(treeRoot, path));
    if (!filePath.startsWith(treeRoot + sep)) {
      throw new Error(
        `Path "${path}" escapes the repository directory. Path traversal is not allowed.`
      );
    }

    const cacheResult = isCacheHit(cacheRoot);
    if (
      !forceRefresh &&
      cacheResult.hit &&
      cacheResult.meta.commitSha === commitSha &&
      previousRoot &&
      existsSync(filePath) &&
      statSync(filePath).isFile()
    ) {
      return {
        localPath: filePath,
        repoRoot: treeRoot,
        path,
        size: safeFileSize(filePath),
        cached: true,
        expiresAt: cacheResult.meta.expiresAt,
        owner,
        repo,
        branch: resolvedRef,
        commitSha,
      };
    }

    const { rawResult } = await fetchCachedRawGitHubFileContent(
      {
        owner,
        repo,
        path,
        type: 'file',
        branch: commitSha,
        fullContent: true,
        contextLines: 0,
        minify: 'none',
        goal: `Save ${owner}/${repo}/${path} locally`,
        reasoning: 'GitHub file materialization',
      },
      authInfo
    );

    if (!('data' in rawResult) || !rawResult.data) {
      const error = 'error' in rawResult ? rawResult.error : undefined;
      throw new Error(error || `Failed to fetch ${owner}/${repo}/${path}`);
    }

    const meta = createCacheMeta(
      owner,
      repo,
      resolvedRef,
      'treeFetch',
      undefined,
      undefined,
      commitSha
    );
    treeRoot = publishTreeSnapshot(
      octocodeDir,
      cacheRoot,
      previousRoot,
      meta,
      root => {
        const parent = dirname(path);
        ensureSnapshotDirectory(root, parent === '.' ? '' : parent);
        const destination = join(root, path);
        if (lstatSync(destination, { throwIfNoEntry: false }))
          rmSync(destination, { recursive: true, force: true });
        writeFileSync(destination, rawResult.data.rawContent, 'utf8');
      }
    );
    filePath = join(treeRoot, path);

    return {
      localPath: filePath,
      repoRoot: treeRoot,
      path,
      size: Buffer.byteLength(rawResult.data.rawContent, 'utf8'),
      cached: false,
      expiresAt: meta.expiresAt,
      owner,
      repo,
      branch: resolvedRef,
      commitSha,
    };
  });
}
