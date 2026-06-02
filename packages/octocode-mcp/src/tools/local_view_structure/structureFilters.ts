import type { z } from 'zod/v4';
import type { ViewStructureQuerySchema } from '@octocodeai/octocode-core/schemas';

type ViewStructureQuery = z.infer<typeof ViewStructureQuerySchema>;
import { checkRegexSafety } from '../../utils/core/safeRegex.js';

/**
 * Internal directory entry for processing
 */
export interface DirectoryEntry {
  name: string;
  type: 'file' | 'directory' | 'symlink';
  size?: string;
  /** Raw size in bytes — used for numeric sort to avoid parseFileSize round-trip loss. */
  sizeBytes?: number;
  modified?: string;
  permissions?: string;
  extension?: string;
  depth?: number;
}

/** Subset of ViewStructureQuery fields used by applyEntryFilters */
type EntryFilterQuery = Pick<
  Partial<ViewStructureQuery>,
  'pattern' | 'extension' | 'extensions' | 'directoriesOnly' | 'filesOnly'
>;

/**
 * Apply query filters to entry list
 * Used by both CLI and recursive paths to ensure consistent filtering
 */
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
      // First escape regex metacharacters INCLUDING glob characters (* and ?)
      // so they can be converted to regex patterns in the next step
      let regexPattern = pattern.replace(/[.+^${}()|[\]\\*?]/g, '\\$&');

      // Convert escaped glob characters to regex equivalents
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
          // Fall back to literal match for unsafe patterns
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
          // For recursive mode, entry.name is the relative path (e.g., "subdir/file.ts")
          // Pattern should match the filename part only for consistency
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
        // For recursive mode, entry.name is the relative path (e.g., "subdir/file.ts")
        // Pattern should match the filename part only for consistency
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

/**
 * Format directory entry as compact string
 * Format: [TYPE] [permissions] name (size) date .ext
 */
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
