import { isAbsolute, join, relative, resolve, sep } from 'node:path';

/**
 * A source-map `sources` entry as a safe relative file path: scheme, drive letters, empty,
 * `.` and `..` segments are dropped and other characters become `_`, so the result can never
 * leave the folder it is joined to. An entry with nothing left becomes `source-<index>`.
 */
export function sanitizeSourcePath(source: string, index: number): string {
  const withoutScheme = source.replace(/^[a-z][a-z0-9+.-]*:(\/\/)?/i, '');
  const parts = withoutScheme
    .split(/[\\/]+/)
    .map(part => part.replace(/[^\w.@+~-]/g, '_'))
    .filter(
      part => part && part !== '.' && part !== '..' && !/^\.+$/.test(part)
    );
  return parts.length ? parts.join('/') : `source-${index}`;
}

/** The absolute destination for a sanitized path, or null when it would fall outside `root`. */
export function confinedPath(
  root: string,
  safeRelative: string
): string | null {
  const base = resolve(root);
  const target = resolve(join(base, safeRelative));
  const rel = relative(base, target);
  if (!rel || rel.startsWith(`..${sep}`) || rel === '..' || isAbsolute(rel))
    return null;
  return target;
}

/** A name safe for one file: the last URL path segment without separators, or the fallback. */
export function fileStem(url: string, fallback: string): string {
  let last = url;
  try {
    last = new URL(url).pathname;
  } catch {
    // not a URL: judge the text as written
  }
  const stem = (last.split(/[\\/]/).filter(Boolean).pop() ?? '')
    .replace(/[^\w.@+~-]/g, '_')
    .replace(/^\.+/, '');
  return stem || fallback;
}
