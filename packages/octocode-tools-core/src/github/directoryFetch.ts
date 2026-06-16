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
          'To refresh the clone, use ghCloneRepo with forceRefresh: true.'
      );
    }

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

  evictExpiredClones(octocodeDir);
  ensureCloneParentDir(cloneDir);
  if (existsSync(dirPath)) {
    rmSync(dirPath, { recursive: true, force: true });
  }
  mkdirSync(dirPath, { recursive: true, mode: 0o700 });

  const savedFiles: GitHubDirectoryFileEntry[] = [];
  for (const { entry, content } of filesToSave) {
    const filePath = resolve(join(cloneDir, entry.path));
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
  cloneDir: string
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
          const relativePath = full.substring(cloneDir.length + 1);
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

function getExtension(filename: string): string {
  const lastDot = filename.lastIndexOf('.');
  if (lastDot === -1) return '';
  return filename.substring(lastDot).toLowerCase();
}
