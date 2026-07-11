import { readdirSync, statSync } from 'node:fs';
import { join } from 'node:path';
import type { GitHubDirectoryFileEntry } from '@octocodeai/octocode-core/extra-types';
import type { DirectoryFetchResult } from '../../tools/github_fetch_content/types.js';

export const MAX_DIRECTORY_FILES = 50;

export const MAX_TOTAL_SIZE = 5 * 1024 * 1024;

export const MAX_FILE_SIZE = 300 * 1024;

export const CONCURRENCY = 5;

export const FETCH_TIMEOUT_MS = 10_000;

export const BINARY_EXTENSIONS = new Set([
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

export interface DirectoryEntry {
  name: string;
  path: string;
  type: string;
  size: number;
  download_url: string | null;
}

export type DirectorySkipCounts = DirectoryFetchResult['skipped'];

export const DIRECTORY_FETCH_LIMITS = {
  maxDirectoryFiles: MAX_DIRECTORY_FILES,
  maxTotalSize: MAX_TOTAL_SIZE,
  maxFileSize: MAX_FILE_SIZE,
};

export function emptyDirectorySkipCounts(): DirectorySkipCounts {
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

export function directoryFetchComplete(skipped: DirectorySkipCounts): boolean {
  return Object.values(skipped).every(count => count === 0);
}

export function directoryFetchWarnings(
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

export async function fetchFilesInBatches(
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

export async function fetchDownloadUrl(
  url: string,
  token?: string
): Promise<string> {
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

export function scanDirectoryStats(
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

export function safeFileSize(path: string): number {
  try {
    return statSync(path).size;
  } catch {
    return 0;
  }
}
