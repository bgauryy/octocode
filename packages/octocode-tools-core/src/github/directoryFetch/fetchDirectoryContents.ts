import { writeFileSync, existsSync, rmSync, readdirSync } from 'node:fs';
import { join, resolve, relative, sep } from 'node:path';
import { getOctocodeDir } from '../../shared/paths.js';
import type { AuthInfo } from '@modelcontextprotocol/server';
import { getOctokit } from '../client.js';
import { fetchCachedRawGitHubFileContent } from '../fileContentRaw/cache.js';
import type { GitHubDirectoryFileEntry } from '@octocodeai/octocode-core/extra-types';
import type { DirectoryFetchResult } from '../../tools/github_fetch_content/types.js';
import {
  isCacheHit,
  createCacheMeta,
  evictExpiredTrees,
} from '../../tools/github_clone_repo/cache.js';
import { getTreeDir } from '../../tools/github_clone_repo/cachePaths.js';
import { getDiscoveryExtension } from '@octocodeai/octocode-engine/security';
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
import {
  currentTreeSnapshot,
  publishTreeSnapshot,
  withTreeLock,
  ensureSnapshotDirectory,
} from './snapshot.js';
import { resolveMaterializationRef } from './refResolution.js';
import {
  readDirectoryCacheMetadata,
  writeDirectoryCacheMetadata,
} from './cacheMetadata.js';

export async function fetchDirectoryContents(
  owner: string,
  repo: string,
  path: string,
  branch: string,
  authInfo?: AuthInfo,
  forceRefresh = false
): Promise<DirectoryFetchResult> {
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

    let dirPath = resolve(join(treeRoot, path));
    if (!dirPath.startsWith(treeRoot + sep) && dirPath !== treeRoot) {
      throw new Error(
        `Path "${path}" escapes the repository directory. Path traversal is not allowed.`
      );
    }

    const cacheResult = isCacheHit(cacheRoot);
    if (cacheResult.hit && cacheResult.meta.commitSha === commitSha) {
      if (!forceRefresh && previousRoot && existsSync(dirPath)) {
        const cached = scanDirectoryStats(dirPath, treeRoot);
        const metadata = readDirectoryCacheMetadata(dirPath, commitSha);
        const skipped = metadata?.skipped ?? emptyDirectorySkipCounts();
        const cachedFiles = new Map(
          cached.files.map(file => [file.path, file.size])
        );
        const complete =
          metadata?.complete === true &&
          directoryFetchComplete(skipped) &&
          metadata.files.length === cached.fileCount &&
          metadata.files.every(
            file => cachedFiles.get(file.path) === file.size
          );
        return {
          localPath: dirPath,
          repoRoot: treeRoot,
          files: cached.files,
          fileCount: cached.fileCount,
          totalSize: cached.totalSize,
          complete,
          verified: false,
          commitSha,
          ...(skipped.nonFile > 0 ? { hasSubdirectories: true } : {}),
          directoryEntryCount:
            metadata?.directoryEntryCount ?? cached.fileCount,
          eligibleFileCount: metadata?.eligibleFileCount ?? cached.fileCount,
          savedFileCount: cached.fileCount,
          skipped,
          limits: DIRECTORY_FETCH_LIMITS,
          warnings: directoryFetchWarnings(complete, false),
          cached: true,
          expiresAt: cacheResult.meta.expiresAt,
          owner,
          repo,
          branch: resolvedRef,
          directoryPath: path,
        };
      }
    }

    const octokit = await getOctokit(authInfo);

    const { data } = await octokit.rest.repos.getContent({
      owner,
      repo,
      path,
      ref: commitSha,
    });

    if (!Array.isArray(data)) {
      throw new Error(
        `Path "${path}" is not a directory. Use type "file" to fetch file content.`
      );
    }

    const directoryEntries = data as DirectoryEntry[];
    const skipped = emptyDirectorySkipCounts();
    const eligibleEntries: DirectoryEntry[] = [];

    for (const item of directoryEntries) {
      if (item.type !== 'file') {
        skipped.nonFile += 1;
        continue;
      }
      if (item.size > MAX_FILE_SIZE) {
        skipped.oversized += 1;
        continue;
      }
      const ext = getDiscoveryExtension(item.name, {
        lowercase: true,
        leadingDot: true,
      });
      if (BINARY_EXTENSIONS.has(ext)) {
        skipped.binary += 1;
        continue;
      }
      eligibleEntries.push(item);
    }

    skipped.fileLimit = Math.max(
      0,
      eligibleEntries.length - MAX_DIRECTORY_FILES
    );
    const fileEntries = eligibleEntries.slice(0, MAX_DIRECTORY_FILES);

    const fetchedFiles = await fetchFilesInBatches(
      fileEntries,
      CONCURRENCY,
      async entry => {
        const { rawResult } = await fetchCachedRawGitHubFileContent(
          {
            owner,
            repo,
            path: entry.path,
            branch: commitSha,
            type: 'file',
            fullContent: true,
            minify: 'none',
            contextLines: 0,
            forceRefresh,
          },
          authInfo
        );
        if (!('data' in rawResult) || !rawResult.data)
          throw new Error('Directory file read failed');
        return rawResult.data.rawContent;
      }
    );
    skipped.fetchFailed = fileEntries.length - fetchedFiles.length;

    let totalSize = 0;
    const filesToSave: Array<{ entry: DirectoryEntry; content: string }> = [];
    for (let i = 0; i < fetchedFiles.length; i += 1) {
      const { entry, content } = fetchedFiles[i]!;
      const size = Buffer.byteLength(content, 'utf8');
      if (totalSize + size > MAX_TOTAL_SIZE) {
        skipped.totalSizeLimit = fetchedFiles.length - i;
        break;
      }
      totalSize += size;
      filesToSave.push({ entry, content });
    }

    const savedFiles: GitHubDirectoryFileEntry[] = [];
    const safeFiles = filesToSave.filter(({ entry }) => {
      const filePath = resolve(join(treeRoot, entry.path));
      const local = relative(dirPath, filePath);
      if (
        !local ||
        local.startsWith('..') ||
        local.includes(sep) ||
        entry.name === '.octocode-directory-meta.json'
      ) {
        skipped.pathTraversal += 1;
        return false;
      }
      return true;
    });
    totalSize = 0;
    for (const { entry, content } of safeFiles) {
      const size = Buffer.byteLength(content, 'utf8');
      totalSize += size;
      savedFiles.push({ path: entry.path, size, type: 'file' });
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
    const complete = directoryFetchComplete(skipped);
    const verified = complete;
    const hasSubdirectories = skipped.nonFile > 0;
    treeRoot = publishTreeSnapshot(
      octocodeDir,
      cacheRoot,
      previousRoot,
      meta,
      root => {
        const destination = ensureSnapshotDirectory(root, path);
        // This generation's direct files come only from this pinned API read.
        // Keep real child directories materialized by separate requests, but
        // drop copied files and symlinks (including dangling links) first.
        for (const existing of readdirSync(destination, {
          withFileTypes: true,
        })) {
          if (!existing.isDirectory())
            rmSync(join(destination, existing.name), { force: true });
        }
        for (const { entry, content } of safeFiles) {
          const filePath = join(root, entry.path);
          // These are direct files; preserve already-materialized child directories.
          if (existsSync(filePath))
            rmSync(filePath, { recursive: true, force: true });
          writeFileSync(filePath, content, 'utf-8');
        }
        writeDirectoryCacheMetadata(destination, {
          commitSha,
          complete,
          directoryEntryCount: directoryEntries.length,
          eligibleFileCount: eligibleEntries.length,
          skipped,
          files: savedFiles,
        });
      }
    );
    dirPath = join(treeRoot, path);

    return {
      localPath: dirPath,
      repoRoot: treeRoot,
      files: savedFiles,
      fileCount: savedFiles.length,
      totalSize,
      complete,
      verified,
      commitSha,
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
      branch: resolvedRef,
      directoryPath: path,
    };
  });
}
