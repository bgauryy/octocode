import fs from 'fs';
import path from 'path';
import { getExtension } from '../../utils/file/filters.js';
import { formatFileSize, parseFileSize } from '../../utils/file/size.js';
import type { DirectoryEntry } from './structureFilters.js';

export async function parseLsSimple(
  output: string,
  basePath: string,
  showModified: boolean = false
): Promise<DirectoryEntry[]> {
  const lines = output.split('\n').filter(line => line.trim());

  const statPromises = lines.map(async name => {
    const fullPath = path.join(basePath, name);
    try {
      const stats = await fs.promises.lstat(fullPath);
      const entry: DirectoryEntry = {
        name,
        type: stats.isDirectory()
          ? ('directory' as const)
          : stats.isSymbolicLink()
            ? ('symlink' as const)
            : ('file' as const),
        size: formatFileSize(stats.size),
        extension: getExtension(name),
      };
      if (showModified) {
        entry.modified = stats.mtime.toISOString();
      }
      return entry;
    } catch {
      return {
        name,
        type: 'file' as const,
        extension: getExtension(name),
      };
    }
  });

  return await Promise.all(statPromises);
}

export function parseLsLongFormat(
  output: string,
  showModified: boolean = false
): DirectoryEntry[] {
  const lines = output.split('\n').filter(line => line.trim());
  const entries: DirectoryEntry[] = [];

  for (const line of lines) {
    if (line.startsWith('total ')) continue;

    const match = line.match(
      /^([\w-]+[@+]?)\s+\d+\s+\w+\s+\w+\s+([\d.]+[KMGT]?)\s+(\w+\s+\d+\s+[\d:]+)\s+(.+)$/
    );

    if (match && match[1] && match[2] && match[3] && match[4]) {
      const permissions = match[1];
      const sizeStr = match[2];
      const modified = match[3];
      const name = match[4];

      let size = 0;
      if (/^\d+$/.test(sizeStr)) {
        size = parseInt(sizeStr, 10);
      } else {
        size = parseFileSize(sizeStr);
      }

      let type: 'file' | 'directory' | 'symlink' = 'file';
      if (permissions.startsWith('d')) type = 'directory';
      else if (permissions.startsWith('l')) type = 'symlink';

      const entry: DirectoryEntry = {
        name,
        type,
        size: formatFileSize(size),
        permissions,
        extension: getExtension(name),
      };
      if (showModified) {
        entry.modified = modified;
      }
      entries.push(entry);
    }
  }

  return entries;
}
