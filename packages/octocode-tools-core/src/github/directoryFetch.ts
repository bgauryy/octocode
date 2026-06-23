import {
  writeFileSync,
  mkdirSync,
  existsSync,
  rmSync,
  readdirSync,
  statSync,
} from 'node:fs';
import { join, dirname, resolve, sep } from 'node:path';
import { getOctocodeDir } from '../shared/index.js';
import type { AuthInfo } from '@modelcontextprotocol/sdk/server/auth/types.js';
import { getOctokit } from './client.js';
import type { GitHubDirectoryFileEntry } from '@octocodeai/octocode-core/extra-types';
import type {
  DirectoryFetchResult,
  FileMaterializationResult,
} from '../tools/github_fetch_content/types.js';
import { fetchRawGitHubFileContent } from './fileContentRaw.js';
import {
  getTreeDir,
  isCacheHit,
  writeCacheMeta,
  createCacheMeta,
  ensureCloneParentDir,
  evictExpiredTrees,
} from '../tools/github_clone_repo/cache.js';
import { getExtension } from '../utils/file/filters.js';

export const MAX_DIRECTORY_FILES = 50;

export const MAX_TOTAL_SIZE = 5 * 1024 * 1024;

const MAX_FILE_SIZE = 300 * 1024;

const CONCURRENCY = 5;

const FETCH_TIMEOUT_MS = 10_000;

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

type DirectorySkipCounts = DirectoryFetchResult['skipped'];

const DIRECTORY_FETCH_LIMITS = {
  maxDirectoryFiles: MAX_DIRECTORY_FILES,
  maxTotalSize: MAX_TOTAL_SIZE,
  maxFileSize: MAX_FILE_SIZE,
};

function emptyDirectorySkipCounts(): DirectorySkipCounts {
  return {
    nonFile: 0,
    missingDownloadUrl: 0,
    oversized: 0,
    binary: 0,
    fileLimit: 0,
    fetchFailed: 0,
    totalSizeLimit: 0,
    pathTraversal: 0,
  };
}

function directoryFetchComplete(skipped: DirectorySkipCounts): boolean {
  return Object.values(skipped).every(count => count === 0);
}

function directoryFetchWarnings(
  complete: boolean,
  verified: boolean
): string[] | undefined {
  if (!verified && complete) {
    return [
      'Cannot verify completeness against remote tree; use forceRefresh or ghCloneRepo if completeness matters.',
    ];
  }
  if (!complete) {
    return [
      'Directory materialization is partial; inspect skipped counts or use ghCloneRepo before repo-wide reachability/dead-code conclusions.',
    ];
  }
  return undefined;
}

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

export async function fetchFileContentToDisk(
  owner: string,
  repo: string,
  path: string,
  branch: string,
  authInfo?: AuthInfo,
  forceRefresh = false
): Promise<FileMaterializationResult> {
  const octocodeDir = getOctocodeDir();
  const treeRoot = getTreeDir(octocodeDir, owner, repo, branch);
  const filePath = resolve(join(treeRoot, path));
  if (!filePath.startsWith(treeRoot + sep) && filePath !== treeRoot) {
    throw new Error(
      `Path "${path}" escapes the repository directory. Path traversal is not allowed.`
    );
  }

  const cacheResult = isCacheHit(treeRoot);
  if (!forceRefresh && cacheResult.hit && existsSync(filePath)) {
    return {
      localPath: filePath,
      repoRoot: treeRoot,
      path,
      size: safeFileSize(filePath),
      cached: true,
      expiresAt: cacheResult.meta.expiresAt,
      owner,
      repo,
      branch,
    };
  }

  const rawResult = await fetchRawGitHubFileContent(
    {
      owner,
      repo,
      path,
      type: 'file',
      branch,
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

  const resolvedBranch = rawResult.data.branch || branch;
  const meta = createCacheMeta(owner, repo, resolvedBranch, 'treeFetch');
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
    branch: resolvedBranch,
  };
}

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
    }
  }

  return results;
}

const ALLOWED_DOWNLOAD_HOSTS = new Set([
  'raw.githubusercontent.com',
  'objects.githubusercontent.com',
  'github.com',
]);

async function fetchDownloadUrl(url: string, token?: string): Promise<string> {
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

function scanDirectoryStats(
  dirPath: string,
  repoRoot: string
): { files: GitHubDirectoryFileEntry[]; fileCount: number; totalSize: number } {
  const files: GitHubDirectoryFileEntry[] = [];
  let totalSize = 0;

  function walk(current: string): void {
    let entries: string[];
    try {
      entries = readdirSync(current);
    } catch {
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
          const relativePath = full.substring(repoRoot.length + 1);
          totalSize += st.size;
          files.push({ path: relativePath, size: st.size, type: 'file' });
        }
      } catch {
        void 0;
      }
    }
  }

  walk(dirPath);
  return { files, fileCount: files.length, totalSize };
}

function safeFileSize(path: string): number {
  try {
    return statSync(path).size;
  } catch {
    return 0;
  }
}
