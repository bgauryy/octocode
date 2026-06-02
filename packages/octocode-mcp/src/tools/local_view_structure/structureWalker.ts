import fs from 'fs';
import path from 'path';
import { getExtension } from '../../utils/file/filters.js';
import { formatFileSize } from '../../utils/file/size.js';
import type { DirectoryEntry } from './structureFilters.js';

export interface WalkStats {
  skipped: number;
  permissionDenied: number;
  /** Set when the root path itself could not be read (e.g. ENOENT, ENOTDIR). */
  rootError?: { code: string; message: string };
}

interface WalkDirectoryOptions {
  basePath: string;
  currentPath: string;
  depth: number;
  maxDepth: number;
  entries: DirectoryEntry[];
  maxEntries?: number;
  showHidden?: boolean;
  showModified?: boolean;
  stats: WalkStats;
  showDetails?: boolean;
}

/**
 * Format Unix file mode bits as rwx permission string (e.g. "rw-r--r--")
 */
function formatPermissions(mode: number): string {
  const chars = ['---', '--x', '-w-', '-wx', 'r--', 'r-x', 'rw-', 'rwx'];
  const owner = chars[(mode >> 6) & 7]!;
  const group = chars[(mode >> 3) & 7]!;
  const other = chars[mode & 7]!;
  return `${owner}${group}${other}`;
}

export async function walkDirectory(
  options: WalkDirectoryOptions
): Promise<void> {
  const {
    basePath,
    currentPath,
    depth,
    maxDepth,
    entries,
    stats,
    maxEntries = 10000,
    showHidden = false,
    showModified = false,
    showDetails = false,
  } = options;

  if (depth >= maxDepth) return;
  if (entries.length >= maxEntries) return;

  try {
    const items = await fs.promises.readdir(currentPath);

    for (const item of items) {
      // Skip hidden files if not requested
      if (!showHidden && item.startsWith('.')) continue;

      const fullPath = path.join(currentPath, item);
      const relativePath = path.relative(basePath, fullPath);

      try {
        const fileStats = await fs.promises.lstat(fullPath);

        let type: 'file' | 'directory' | 'symlink' = 'file';
        if (fileStats.isDirectory()) type = 'directory';
        else if (fileStats.isSymbolicLink()) type = 'symlink';

        const entry: DirectoryEntry = {
          name: relativePath,
          type,
          size: formatFileSize(fileStats.size),
          sizeBytes: fileStats.size,
          extension: getExtension(item),
          depth,
        };
        if (showDetails || showModified) {
          entry.modified = fileStats.mtime.toISOString();
        }
        if (showDetails) {
          entry.permissions = formatPermissions(fileStats.mode);
        }
        entries.push(entry);

        if (type === 'directory') {
          await walkDirectory({
            basePath,
            currentPath: fullPath,
            depth: depth + 1,
            maxDepth,
            entries,
            maxEntries,
            showHidden,
            showModified,
            stats,
            showDetails,
          });
        }
      } catch (err) {
        stats.skipped++;
        if (
          err &&
          typeof err === 'object' &&
          'code' in err &&
          err.code === 'EACCES'
        ) {
          stats.permissionDenied++;
        }
      }
    }
  } catch (err) {
    const code =
      err && typeof err === 'object' && 'code' in err
        ? String(err.code)
        : 'UNKNOWN';
    const message =
      err instanceof Error ? err.message : 'Unknown error reading directory';

    // Record root-level failure separately so callers can surface a clear error
    // (e.g. ENOENT → "directory not found") rather than a generic skip warning.
    if (depth === 0) {
      stats.rootError = { code, message };
    } else {
      stats.skipped++;
      if (code === 'EACCES') {
        stats.permissionDenied++;
      }
    }
  }
}
