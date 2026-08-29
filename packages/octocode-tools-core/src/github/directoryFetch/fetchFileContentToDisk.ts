import { writeFileSync, mkdirSync, existsSync } from 'node:fs';
import { join, dirname, resolve, sep } from 'node:path';
import { getOctocodeDir } from '../../shared/index.js';
import type { AuthInfo } from '@modelcontextprotocol/server';
import type { FileMaterializationResult } from '../../tools/github_fetch_content/types.js';
import { fetchRawGitHubFileContent } from '../fileContentRaw.js';
import {
  getTreeDir,
  isCacheHit,
  writeCacheMeta,
  createCacheMeta,
  ensureCloneParentDir,
  evictExpiredTrees,
} from '../../tools/github_clone_repo/cache.js';
import { safeFileSize } from './helpers.js';
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
  const treeRoot = getTreeDir(octocodeDir, owner, repo, commitSha);
  const filePath = resolve(join(treeRoot, path));
  if (!filePath.startsWith(treeRoot + sep) && filePath !== treeRoot) {
    throw new Error(
      `Path "${path}" escapes the repository directory. Path traversal is not allowed.`
    );
  }

  const cacheResult = isCacheHit(treeRoot);
  if (
    !forceRefresh &&
    cacheResult.hit &&
    cacheResult.meta.commitSha === commitSha &&
    existsSync(filePath)
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

  const rawResult = await fetchRawGitHubFileContent(
    {
      owner,
      repo,
      path,
      type: 'file',
      branch: commitSha,
      fullContent: true,
      contextLines: 0,
      minify: 'none',
      mainResearchGoal: 'Materialize GitHub file content for local research',
      researchGoal: `Save ${owner}/${repo}/${path} locally`,
      reasoning: 'GitHub file materialization',
    },
    authInfo
  );

  if (!('data' in rawResult) || !rawResult.data) {
    const error = 'error' in rawResult ? rawResult.error : undefined;
    throw new Error(error || `Failed to fetch ${owner}/${repo}/${path}`);
  }

  evictExpiredTrees(octocodeDir);
  ensureCloneParentDir(treeRoot);
  const fileDir = dirname(filePath);
  if (!existsSync(fileDir)) {
    mkdirSync(fileDir, { recursive: true, mode: 0o700 });
  }
  writeFileSync(filePath, rawResult.data.rawContent, 'utf-8');

  const meta = createCacheMeta(
    owner,
    repo,
    resolvedRef,
    'treeFetch',
    undefined,
    undefined,
    commitSha
  );
  writeCacheMeta(treeRoot, meta);

  return {
    localPath: filePath,
    repoRoot: treeRoot,
    path,
    size: rawResult.data.rawContent.length,
    cached: false,
    expiresAt: meta.expiresAt,
    owner,
    repo,
    branch: resolvedRef,
    commitSha,
  };
}
