import type { z } from 'zod';
import type { ViewStructureQuerySchema } from '@octocodeai/octocode-core/schemas';

type ViewStructureQuery = z.infer<typeof ViewStructureQuerySchema>;
import { checkRegexSafety } from '../../utils/core/safeRegex.js';

export interface DirectoryEntry {
  name: string;
  type: 'file' | 'directory' | 'symlink';
  size?: string;

  sizeBytes?: number;
  modified?: string;
  permissions?: string;
  extension?: string;
  depth?: number;
}

type EntryFilterQuery = Pick<
  Partial<ViewStructureQuery>,
  'pattern' | 'extension' | 'extensions' | 'directoriesOnly' | 'filesOnly'
>;

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

  if (query.extension) {
    filtered = filtered.filter(
      e => e.type === 'directory' || e.extension === query.extension
    );
  }

  if (query.extensions && query.extensions.length > 0) {
    const extensions = query.extensions;
    filtered = filtered.filter(
      e =>
        e.type === 'directory' ||
        (e.extension && extensions.includes(e.extension))
    );
  }

  if (query.directoriesOnly) {
    filtered = filtered.filter(e => e.type === 'directory');
  }

  if (query.filesOnly) {
    filtered = filtered.filter(e => e.type === 'file');
  }

  return filtered;
}

export function formatEntryString(
  entry: DirectoryEntry,
  indent: number = 0
): string {
  const indentation = '  '.repeat(indent);
  const typeMarker =
    entry.type === 'directory'
      ? '[DIR] '
      : entry.type === 'symlink'
        ? '[LINK]'
        : '[FILE]';
  const nameDisplay =
    entry.type === 'directory' ? `${entry.name}/` : entry.name;
  const dateStr = entry.modified ? ` ${entry.modified.split('T')[0]}` : '';
  const permStr = entry.permissions ? ` ${entry.permissions}` : '';

  if (entry.type === 'file' && entry.size) {
    const extStr = entry.extension ? ` .${entry.extension}` : '';
    return `${indentation}${typeMarker}${permStr} ${nameDisplay} (${entry.size})${dateStr}${extStr}`;
  } else {
    return `${indentation}${typeMarker}${permStr}${dateStr} ${nameDisplay}`;
  }
}

interface EntryOutput {
  name: string;
  type: 'file' | 'dir' | 'link';
  depth?: number;
  size?: string;
  modified?: string;
  permissions?: string;
}

export function toEntryObject(entry: DirectoryEntry): EntryOutput {
  const obj: EntryOutput = {
    name: entry.type === 'directory' ? `${entry.name}/` : entry.name,
    type:
      entry.type === 'directory'
        ? 'dir'
        : entry.type === 'symlink'
          ? 'link'
          : 'file',
  };
  if (entry.depth !== undefined && entry.depth > 0) obj.depth = entry.depth;
  if (entry.size) obj.size = entry.size;
  if (entry.modified) obj.modified = entry.modified;
  if (entry.permissions) obj.permissions = entry.permissions;
  return obj;
}
