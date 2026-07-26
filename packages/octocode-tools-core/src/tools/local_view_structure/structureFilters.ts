import { checkRegexSafety } from '../../utils/core/safeRegex.js';

export interface DirectoryEntry {
  name: string;
  path?: string;
  type: 'file' | 'directory' | 'symlink';
  size?: string;

  sizeBytes?: number;
  modified?: string;
  permissions?: string;
  extension?: string;
  depth?: number;
}

type EntryFilterQuery = {
  pattern?: string;
};

export function applyEntryFilters(
  entries: DirectoryEntry[],
  query: EntryFilterQuery
): DirectoryEntry[] {
  let filtered = entries;

  if (query.pattern) {
    const pattern = query.pattern;

    const isGlob =
      pattern.includes('*') || pattern.includes('?') || pattern.includes('[');

    if (isGlob) {
      let regexPattern = pattern.replace(/[.+^${}()|[\]\\*?]/g, '\\$&');

      regexPattern = regexPattern
        .replace(/\\\*/g, '.*')
        .replace(/\\\?/g, '.')
        .replace(/\\\[!/g, '[^')
        .replace(/\\\[/g, '[')
        .replace(/\\\]/g, ']');

      try {
        const fullPattern = `^${regexPattern}$`;
        const safety = checkRegexSafety(fullPattern);
        if (!safety.safe) {
          filtered = filtered.filter(e => {
            const filename = e.name.includes('/')
              ? e.name.split('/').pop()!
              : e.name;
            return filename.includes(pattern);
          });
          return filtered;
        }
        const regex = new RegExp(fullPattern, 'i');
        filtered = filtered.filter(e => {
          const filename = e.name.includes('/')
            ? e.name.split('/').pop()!
            : e.name;
          return regex.test(filename);
        });
      } catch {
        filtered = filtered.filter(e => {
          const filename = e.name.includes('/')
            ? e.name.split('/').pop()!
            : e.name;
          return filename.includes(pattern);
        });
      }
    } else {
      filtered = filtered.filter(e => {
        const filename = e.name.includes('/')
          ? e.name.split('/').pop()!
          : e.name;
        return filename.includes(pattern);
      });
    }
  }

  return filtered;
}

interface EntryOutput {
  type: 'file' | 'dir' | 'link';
  depth?: number;
  size?: string;
  modified?: string;
  permissions?: string;
}

export function toGroupedLists(entries: DirectoryEntry[]): {
  files?: string[];
  folders?: string[];
  links?: string[];
} {
  const files: string[] = [];
  const folders: string[] = [];
  const links: string[] = [];
  for (const entry of entries) {
    if (entry.type === 'directory') folders.push(entry.name);
    else if (entry.type === 'symlink') links.push(entry.name);
    else files.push(entry.size ? `${entry.name} (${entry.size})` : entry.name);
  }
  return {
    ...(files.length > 0 && { files }),
    ...(folders.length > 0 && { folders }),
    ...(links.length > 0 && { links }),
  };
}

export function toEntryObject(entry: DirectoryEntry): EntryOutput {
  const obj: EntryOutput = {
    type:
      entry.type === 'directory'
        ? 'dir'
        : entry.type === 'symlink'
          ? 'link'
          : 'file',
  };
  if (entry.depth !== undefined && entry.depth > 0) obj.depth = entry.depth;
  if (entry.size && entry.type === 'file') obj.size = entry.size;
  if (entry.modified) obj.modified = entry.modified;
  if (entry.permissions) obj.permissions = entry.permissions;
  return obj;
}
