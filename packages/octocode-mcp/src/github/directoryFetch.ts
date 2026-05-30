/**
 * Directory content fetching from GitHub via Contents API + download_url.
 *
 * Fetches all files in a directory by:
 * 1. Listing directory contents via `repos.getContent` (1 API call)
 * 2. Fetching each file via its `download_url` (parallel HTTP, not rate-limited)
 * 3. Saving files to disk under ~/.octocode/repos/{owner}/{repo}/{branch}/
 * 4. Writing cache metadata (24h TTL, same as clone tool)
 *
 * This is a lightweight alternative to cloning – no git required, just HTTP.
 *
 * @module github/directoryFetch
 */

import {
  writeFileSync,
  mkdirSync,
  existsSync,
  rmSync,
  readdirSync,
  statSync,
} from 'node:fs';
import { join, dirname, resolve, sep } from 'node:path';
import { getOctocodeDir } from 'octocode-shared';
import type { AuthInfo } from '@modelcontextprotocol/sdk/server/auth/types.js';
import { getOctokit } from './client.js';
import type { GitHubDirectoryFileEntry } from '@octocodeai/octocode-core/extra-types';
import type { DirectoryFetchResult } from '../tools/github_fetch_content/types.js';
import {
  getCloneDir,
  isCacheHit,
  writeCacheMeta,
  createCacheMeta,
  ensureCloneParentDir,
  evictExpiredClones,
} from '../tools/github_clone_repo/cache.js';

/** Maximum number of files to fetch from a directory */
export const MAX_DIRECTORY_FILES = 50;

/** Maximum total size of all files (5 MB) */
export const MAX_TOTAL_SIZE = 5 * 1024 * 1024;

/** Maximum size per individual file (300 KB, matches file fetch limit) */
const MAX_FILE_SIZE = 300 * 1024;

/** Number of concurrent download_url fetches */
const CONCURRENCY = 5;

/** Timeout for individual file fetch (10 seconds) */
const FETCH_TIMEOUT_MS = 10_000;

/** File extensions to skip (binary files) */
const BINARY_EXTENSIONS = new Set([
  '.png',
  '.jpg',
  '.jpeg',
  '.gif',
  '.bmp',
  '.ico',
  '.svg',
  '.webp',
  '.mp3',
  '.mp4',
  '.wav',
  '.avi',
  '.mov',
  '.mkv',
  '.webm',
  '.zip',
  '.tar',
  '.gz',
  '.bz2',
  '.7z',
  '.rar',
  '.xz',
  '.exe',
  '.dll',
  '.so',
  '.dylib',
  '.bin',
  '.pdf',
  '.doc',
  '.docx',
  '.xls',
  '.xlsx',
  '.ppt',
  '.pptx',
  '.woff',
  '.woff2',
  '.ttf',
  '.eot',
  '.otf',
  '.pyc',
  '.class',
  '.o',
  '.obj',
  '.lock',
  '.min.js',
  '.min.css',
]);

interface DirectoryEntry {
  name: string;
  path: string;
  type: string;
  size: number;
  download_url: string | null;
}

/**
 * Fetch all files in a GitHub directory and save them to disk.
 *
 * Uses the same cache layout as the clone tool:
 *   ~/.octocode/repos/{owner}/{repo}/{branch}/{path}/
 *
 * Files are saved with 24-hour cache TTL. If the cache is still valid,
 * the saved directory is returned immediately without any API calls.
 *
 * @returns DirectoryFetchResult with localPath and file list
 */
export async function fetchDirectoryContents(
  owner: string,
  repo: string,
  path: string,
  branch: string,
  authInfo?: AuthInfo,
  forceRefresh = false
): Promise<DirectoryFetchResult> {
  const octocodeDir = getOctocodeDir();
  const cloneDir = getCloneDir(octocodeDir, owner, repo, branch);

  // The actual directory on disk where files will be saved.
  // Validate that path doesn't escape cloneDir via '..' traversal.
  const dirPath = resolve(join(cloneDir, path));
  if (!dirPath.startsWith(cloneDir + sep) && dirPath !== cloneDir) {
    throw new Error(
      `Path "${path}" escapes the repository directory. Path traversal is not allowed.`
    );
  }

  const cacheResult = isCacheHit(cloneDir);
  if (cacheResult.hit) {
    const isCloneCache = cacheResult.meta.source === 'clone';

    if (isCloneCache) {
      // A full/sparse git clone is always a superset of what directoryFetch
      // would produce. Use it unconditionally (even on forceRefresh) to avoid
      // degrading clone content. To refresh a clone, use githubCloneRepo.
      if (existsSync(dirPath)) {
        const cached = scanDirectoryStats(dirPath, cloneDir);
        return {
          localPath: dirPath,
          files: cached.files,
          fileCount: cached.fileCount,
          totalSize: cached.totalSize,
          cached: true,
          expiresAt: cacheResult.meta.expiresAt,
          owner,
          repo,
          branch,
          directoryPath: path,
        };
      }
      throw new Error(
        `Path "${path}" not found in the cloned repository (${owner}/${repo}@${branch}). ` +
          'To refresh the clone, use githubCloneRepo with forceRefresh: true.'
      );
    }

    // directoryFetch cache — use if valid and not force-refreshing
    if (!forceRefresh && existsSync(dirPath)) {
      const cached = scanDirectoryStats(dirPath, cloneDir);
      return {
        localPath: dirPath,
        files: cached.files,
        fileCount: cached.fileCount,
        totalSize: cached.totalSize,
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

  const fileEntries = (data as DirectoryEntry[])
    .filter((item): item is DirectoryEntry & { download_url: string } => {
      if (item.type !== 'file') return false;
      if (!item.download_url) return false;
      if (item.size > MAX_FILE_SIZE) return false;
      const ext = getExtension(item.name);
      if (BINARY_EXTENSIONS.has(ext)) return false;
      return true;
    })
    .slice(0, MAX_DIRECTORY_FILES);

  const token = authInfo?.token;
  const fetchedFiles = await fetchFilesInBatches(
    fileEntries,
    CONCURRENCY,
    token
  );

  let totalSize = 0;
  const filesToSave: Array<{ entry: DirectoryEntry; content: string }> = [];
  for (const { entry, content } of fetchedFiles) {
    if (totalSize + content.length > MAX_TOTAL_SIZE) break;
    totalSize += content.length;
    filesToSave.push({ entry, content });
  }

  // Evict any globally-expired clones before writing new content.
  evictExpiredClones(octocodeDir);
  ensureCloneParentDir(cloneDir);
  if (existsSync(dirPath)) {
    rmSync(dirPath, { recursive: true, force: true });
  }
  mkdirSync(dirPath, { recursive: true, mode: 0o700 });

  const savedFiles: GitHubDirectoryFileEntry[] = [];
  for (const { entry, content } of filesToSave) {
    const filePath = resolve(join(cloneDir, entry.path));
    // Skip files whose resolved path escapes the clone directory
    if (!filePath.startsWith(cloneDir + sep)) continue;
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

  // Mark as 'directoryFetch' so the clone tool knows this is NOT
  // a full/sparse clone and will re-clone instead of trusting it.
  const meta = createCacheMeta(owner, repo, branch, 'directoryFetch');
  writeCacheMeta(cloneDir, meta);

  return {
    localPath: dirPath,
    files: savedFiles,
    fileCount: savedFiles.length,
    totalSize,
    cached: false,
    expiresAt: meta.expiresAt,
    owner,
    repo,
    branch,
    directoryPath: path,
  };
}

/**
 * Fetch files in batches with concurrency control.
 */
async function fetchFilesInBatches(
  entries: DirectoryEntry[],
  concurrency: number,
  token?: string
): Promise<Array<{ entry: DirectoryEntry; content: string }>> {
  const results: Array<{ entry: DirectoryEntry; content: string }> = [];

  for (let i = 0; i < entries.length; i += concurrency) {
    const batch = entries.slice(i, i + concurrency);
    const batchResults = await Promise.allSettled(
      batch.map(async entry => {
        const content = await fetchDownloadUrl(entry.download_url!, token);
        return { entry, content };
      })
    );

    for (const result of batchResults) {
      if (result.status === 'fulfilled') {
        results.push(result.value);
      }
      // Skip failed fetches silently — partial results are acceptable
    }
  }

  return results;
}

/** Allowed hostnames for download_url fetches (SSRF prevention) */
const ALLOWED_DOWNLOAD_HOSTS = new Set([
  'raw.githubusercontent.com',
  'objects.githubusercontent.com',
  'github.com',
]);

/**
 * Fetch raw content from a download_url (raw.githubusercontent.com).
 */
async function fetchDownloadUrl(url: string, token?: string): Promise<string> {
  // Validate URL hostname to prevent SSRF via crafted download_url
  try {
    const parsed = new URL(url);
    if (!ALLOWED_DOWNLOAD_HOSTS.has(parsed.hostname)) {
      throw new Error(
        `Blocked fetch to unexpected host: ${parsed.hostname}. Only GitHub download URLs are allowed.`
      );
    }
  } catch (error: unknown) {
    if (error instanceof TypeError) {
      throw new Error(`Invalid download URL: ${url}`);
    }
    throw error;
  }

  const controller = new AbortController();
  const timeout = setTimeout(() => controller.abort(), FETCH_TIMEOUT_MS);

  try {
    const headers: Record<string, string> = {
      'User-Agent': 'octocode-mcp',
    };
    if (token) {
      headers['Authorization'] = `token ${token}`;
    }
    const response = await fetch(url, {
      signal: controller.signal,
      headers,
    });

    if (!response.ok) {
      throw new Error(`HTTP ${response.status} fetching ${url}`);
    }

    return await response.text();
  } finally {
    clearTimeout(timeout);
  }
}

/**
 * Recursively scan a cached directory to compute fileCount, totalSize,
 * and file entries. Used on cache hit so callers get real metadata
 * instead of stale zeros.
 */
function scanDirectoryStats(
  dirPath: string,
  cloneDir: string
): { files: GitHubDirectoryFileEntry[]; fileCount: number; totalSize: number } {
  const files: GitHubDirectoryFileEntry[] = [];
  let totalSize = 0;

  function walk(current: string): void {
    let entries: string[];
    try {
      entries = readdirSync(current);
    } catch {
      // Directory unreadable (permissions/missing); stop this walk branch.
      return;
    }
    for (const name of entries) {
      if (name.startsWith('.')) continue;
      const full = join(current, name);
      try {
        const st = statSync(full);
        if (st.isDirectory()) {
          walk(full);
        } else if (st.isFile()) {
          const relativePath = full.substring(cloneDir.length + 1);
          totalSize += st.size;
          files.push({ path: relativePath, size: st.size, type: 'file' });
        }
      } catch {
        // stat/read failed for one entry; skip it and continue the directory walk.
      }
    }
  }

  walk(dirPath);
  return { files, fileCount: files.length, totalSize };
}

/**
 * Get file extension (lowercase, with dot).
 */
function getExtension(filename: string): string {
  const lastDot = filename.lastIndexOf('.');
  if (lastDot === -1) return '';
  return filename.substring(lastDot).toLowerCase();
}
