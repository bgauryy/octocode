/** Filesystem resolution and the protocol's frozen caseless lease namespace. */
import { lstatSync, readlinkSync, statSync } from 'node:fs';
import { dirname, isAbsolute, parse, sep } from 'node:path';
import { ValidationError } from './errors.js';
import { normalizePathComponent } from './unicode16.js';

const MAX_LINKS = 40;

/** Paths split on the platform's separators: `\` is an ordinary filename character on POSIX. */
const SEPARATORS = sep === '\\' ? /[\\/]/ : /\//;

/** A path split into its anchor (root) and its components, without empty or `.` parts. */
function split(value: string): { anchor: string; parts: string[] } {
  const anchor = parse(value).root;
  return { anchor, parts: value.slice(anchor.length).split(SEPARATORS).filter(part => part !== '' && part !== '.') };
}

/** `left/right` joined textually: `..` stays a component, because it must be resolved against real directories. */
function append(left: string, part: string): string {
  return left === '' ? part : left.endsWith(sep) ? left + part : left + sep + part;
}

function missing(error: unknown): boolean {
  return (error as NodeJS.ErrnoException).code === 'ENOENT';
}

/** Whether `path` names a directory (following symlinks); false when it does not exist. */
export function isDirectory(path: string): boolean {
  try {
    return statSync(path).isDirectory();
  } catch {
    return false;
  }
}

export function isFile(path: string): boolean {
  try {
    return statSync(path).isFile();
  } catch {
    return false;
  }
}

export function lexists(path: string): boolean {
  try {
    lstatSync(path);
    return true;
  } catch {
    return false;
  }
}

/** Absolute location of `path` below `base`, following symlinks physically; missing components are kept as written. */
export function resolvePath(base: string, path: string): string {
  let links = 0;
  const resolve = (value: string): string => {
    const { anchor, parts } = split(value);
    let resolved = anchor;
    for (const part of parts) {
      if (part === '..') {
        resolved = dirname(resolved);
        continue;
      }
      resolved = append(resolved, part);
      let symlink: boolean;
      try {
        symlink = lstatSync(resolved).isSymbolicLink();
      } catch (error) {
        if (missing(error)) continue;
        throw error;
      }
      if (!symlink) continue;
      links += 1;
      if (links > MAX_LINKS) throw new ValidationError('Too many symbolic links');
      const target = readlinkSync(resolved);
      resolved = resolve(isAbsolute(target) ? target : append(dirname(resolved), target));
    }
    return resolved;
  };
  return resolve(isAbsolute(path) ? path : append(base, path));
}

/**
 * An absolute path with every symlink resolved. A strict call fails when a component is
 * missing; otherwise the missing tail is kept as written.
 */
export function realpath(value: string, strict = false): string {
  const absolute = isAbsolute(value) ? value : append(process.cwd(), value);
  const first = split(absolute);
  const pending = first.parts.reverse();
  let resolved = first.anchor;
  let links = 0;
  for (let part = pending.pop(); part !== undefined; part = pending.pop()) {
    if (part === '..') {
      resolved = dirname(resolved);
      continue;
    }
    const candidate = append(resolved, part);
    let symlink = false;
    try {
      symlink = lstatSync(candidate).isSymbolicLink();
    } catch (error) {
      if (strict) throw error;
    }
    if (!symlink) {
      resolved = candidate;
      continue;
    }
    links += 1;
    if (links > MAX_LINKS) throw new ValidationError('Too many symbolic links');
    const target = split(readlinkSync(candidate));
    if (target.anchor) resolved = target.anchor;
    pending.push(...target.parts.reverse());
  }
  return resolved;
}

/** `/` plus each component's canonical caseless form; the workspace root is the empty key. */
export function leaseKey(path: string): string {
  return split(path).parts.map(component => '/' + normalizePathComponent(component)).join('');
}

/** Every proper prefix of a key that ends before a `/`, shortest first. */
export function ancestors(key: string): string[] {
  const result: string[] = [];
  for (let index = 0; index < key.length; index += 1) if (key[index] === '/') result.push(key.slice(0, index));
  return result;
}

export function keysOverlap(a: string, ak: string, b: string, bk: string): boolean {
  return a === b || (ak === 'tree' && b.startsWith(a + '/')) || (bk === 'tree' && a.startsWith(b + '/'));
}

export function overlap(a: string, ak: string, b: string, bk: string): boolean {
  return keysOverlap(leaseKey(a), ak, leaseKey(b), bk);
}
