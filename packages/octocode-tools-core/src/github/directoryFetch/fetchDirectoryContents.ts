import { writeFileSync, mkdirSync, existsSync, rmSync } from 'node:fs';
import { join, dirname, resolve, sep } from 'node:path';
import { getOctocodeDir } from '../../shared/index.js';
import type { AuthInfo } from '@modelcontextprotocol/sdk/server/auth/types.js';
import { getOctokit } from '../client.js';
import type { GitHubDirectoryFileEntry } from '@octocodeai/octocode-core/extra-types';
import type { DirectoryFetchResult } from '../../tools/github_fetch_content/types.js';
import {
  getTreeDir,
  isCacheHit,
  writeCacheMeta,
  createCacheMeta,
  ensureCloneParentDir,
  evictExpiredTrees,
} from '../../tools/github_clone_repo/cache.js';
import { getExtension } from '../../utils/file/filters.js';
import {
  MAX_DIRECTORY_FILES,
  MAX_TOTAL_SIZE,
  MAX_FILE_SIZE,
  CONCURRENCY,
  BINARY_EXTENSIONS,
  DIRECTORY_FETCH_LIMITS,
  emptyDirectorySkipCounts,
  directoryFetchComplete,
  directoryFetchWarnings,
  fetchFilesInBatches,
  scanDirectoryStats,
  type DirectoryEntry,
} from './helpers.js';

export async function fetchDirectoryContents(
  owner: string,
  repo: string,
  path: string,
  branch: string,
  authInfo?: AuthInfo,
  forceRefresh = false
): Promise<DirectoryFetchResult> {
  const octocodeDir = getOctocodeDir();
  const treeRoot = getTreeDir(octocodeDir, owner, repo, branch);

  const dirPath = resolve(join(treeRoot, path));
  if (!dirPath.startsWith(treeRoot + sep) && dirPath !== treeRoot) {
    throw new Error(
      `Path "${path}" escapes the repository directory. Path traversal is not allowed.`
    );
  }

  const cacheResult = isCacheHit(treeRoot);
  if (cacheResult.hit) {
    if (!forceRefresh && existsSync(dirPath)) {
      const cached = scanDirectoryStats(dirPath, treeRoot);
      const skipped = emptyDirectorySkipCounts();
      return {
        localPath: dirPath,
        repoRoot: treeRoot,
        files: cached.files,
        fileCount: cached.fileCount,
        totalSize: cached.totalSize,
        complete: true,
        verified: false,
        ...(cacheResult.meta.commitSha
          ? { commitSha: cacheResult.meta.commitSha }
          : {}),
        directoryEntryCount: cached.fileCount,
        eligibleFileCount: cached.fileCount,
        savedFileCount: cached.fileCount,
        skipped,
        limits: DIRECTORY_FETCH_LIMITS,
        warnings: directoryFetchWarnings(true, false),
        cached: true,
        expiresAt: cacheResult.meta.expiresAt,
        owner,
        repo,
        branch,
        directoryPath: path,
      };
    }
  }

  const octokit = await getOctokit(authInfo);

  // Resolve the branch-tip SHA before fetching so we can record it in cache
  // meta. Agents can compare this to the current SHA on cache hits to detect
  // branch drift within the 24 h TTL window.
  let commitSha: string | undefined;
  try {
    const branchData = await octokit.rest.repos.getBranch({
      owner,
      repo,
      branch,
    });
    commitSha = branchData.data.commit.sha;
  } catch {
    // Non-fatal — proceed without SHA (legacy behaviour)
  }

  const { data } = await octokit.rest.repos.getContent({
    owner,
    repo,
    path,
    ref: branch,
  });

  if (!Array.isArray(data)) {
    throw new Error(
      `Path "${path}" is not a directory. Use type "file" to fetch file content.`
    );
  }

  const directoryEntries = data as DirectoryEntry[];
  const skipped = emptyDirectorySkipCounts();
  const eligibleEntries: Array<DirectoryEntry & { download_url: string }> = [];

  for (const item of directoryEntries) {
    if (item.type !== 'file') {
      skipped.nonFile += 1;
      continue;
    }
    if (!item.download_url) {
      skipped.missingDownloadUrl += 1;
      continue;
    }
    if (item.size > MAX_FILE_SIZE) {
      skipped.oversized += 1;
      continue;
    }
    const ext = getExtension(item.name, {
      lowercase: true,
      leadingDot: true,
    });
    if (BINARY_EXTENSIONS.has(ext)) {
      skipped.binary += 1;
      continue;
    }
    eligibleEntries.push(item as DirectoryEntry & { download_url: string });
  }

  skipped.fileLimit = Math.max(0, eligibleEntries.length - MAX_DIRECTORY_FILES);
  const fileEntries = eligibleEntries.slice(0, MAX_DIRECTORY_FILES);

  const token = authInfo?.token;
  const fetchedFiles = await fetchFilesInBatches(
    fileEntries,
    CONCURRENCY,
    token
  );
  skipped.fetchFailed = fileEntries.length - fetchedFiles.length;

  let totalSize = 0;
  const filesToSave: Array<{ entry: DirectoryEntry; content: string }> = [];
  for (let i = 0; i < fetchedFiles.length; i += 1) {
    const { entry, content } = fetchedFiles[i]!;
    if (totalSize + content.length > MAX_TOTAL_SIZE) {
      skipped.totalSizeLimit = fetchedFiles.length - i;
      break;
    }
    totalSize += content.length;
    filesToSave.push({ entry, content });
  }

  evictExpiredTrees(octocodeDir);
  ensureCloneParentDir(treeRoot);
  if (existsSync(dirPath)) {
    rmSync(dirPath, { recursive: true, force: true });
  }
  mkdirSync(dirPath, { recursive: true, mode: 0o700 });

  const savedFiles: GitHubDirectoryFileEntry[] = [];
  for (const { entry, content } of filesToSave) {
    const filePath = resolve(join(treeRoot, entry.path));
    if (!filePath.startsWith(treeRoot + sep)) {
      skipped.pathTraversal += 1;
      continue;
    }
    const fileDir = dirname(filePath);
    if (!existsSync(fileDir)) {
      mkdirSync(fileDir, { recursive: true, mode: 0o700 });
    }
    writeFileSync(filePath, content, 'utf-8');
    savedFiles.push({
      path: entry.path,
      size: content.length,
      type: 'file',
    });
  }

  const meta = createCacheMeta(
    owner,
    repo,
    branch,
    'treeFetch',
    undefined,
    undefined,
    commitSha
  );
  writeCacheMeta(treeRoot, meta);
  const complete = directoryFetchComplete(skipped);
  const verified = complete;
  const hasSubdirectories = skipped.nonFile > 0;

  return {
    localPath: dirPath,
    repoRoot: treeRoot,
    files: savedFiles,
    fileCount: savedFiles.length,
    totalSize,
    complete,
    verified,
    ...(commitSha ? { commitSha } : {}),
    ...(hasSubdirectories ? { hasSubdirectories: true } : {}),
    directoryEntryCount: directoryEntries.length,
    eligibleFileCount: eligibleEntries.length,
    savedFileCount: savedFiles.length,
    skipped,
    limits: DIRECTORY_FETCH_LIMITS,
    warnings: directoryFetchWarnings(complete, verified),
    cached: false,
    expiresAt: meta.expiresAt,
    owner,
    repo,
    branch,
    directoryPath: path,
  };
}
