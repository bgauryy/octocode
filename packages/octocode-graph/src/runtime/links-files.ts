// Manifest and config → file links (F5 of the graph end-to-end RFC), from the fields whose meaning the tool defines
// and nothing else: package.json `bin`, `main`, `module`, `types`, `exports`, `imports`; Cargo.toml `[lib] path`,
// `[[bin]] path` and `package.build`; tsconfig `extends` and `references`. Each value is a path relative to its
// manifest, cited at the line that holds it. A path that exists is a link. One under a build output directory
// (dist/, out/, build/, target/) is also `build-output`, with the sources of the same name as candidates, never a
// guessed source (no source map or build entry list says which); a path that is neither is `no-such-file`.
// Configs are read, never run.
import { existsSync, readFileSync } from 'node:fs';
import { join, posix } from 'node:path';
import type { Link, UnresolvedLink } from './types.ts';

const MANIFEST_FIELDS = [
  'bin',
  'main',
  'module',
  'types',
  'typings',
  'exports',
  'imports',
];
const BUILD_OUTPUT = /^(?:dist|out|build|target)\//;

// Every string leaf of a JSON value (conditional exports nest objects), with no glob patterns.
function leaves(value: unknown): string[] {
  if (typeof value === 'string') return value.includes('*') ? [] : [value];
  if (Array.isArray(value)) return value.flatMap(leaves);
  if (value && typeof value === 'object')
    return Object.values(value).flatMap(leaves);
  return [];
}

// The line of a manifest that holds a value: its first quoted occurrence after the field's key.
function lineOf(lines: string[], key: string, value: string, from = 0) {
  const keyAt = lines.findIndex((l, i) => i >= from && l.includes(`"${key}"`));
  const start = keyAt < 0 ? from : keyAt;
  const at = lines.findIndex((l, i) => i >= start && l.includes(`"${value}"`));
  return at < 0 ? Math.max(1, keyAt + 1) : at + 1;
}

interface Found {
  links: Link[];
  unresolved: UnresolvedLink[];
}

// Sources of a build output's name under its package, outside the build output directories.
function sameNamed(files: Set<string>, dir: string, local: string) {
  const stem = posix.basename(local).replace(/(\.d)?\.[cm]?[jt]sx?$/, '');
  return [...files].filter(
    f =>
      f.startsWith(dir ? `${dir}/` : '') &&
      !BUILD_OUTPUT.test(dir ? f.slice(dir.length + 1) : f) &&
      posix.basename(f).replace(/\.[cm]?[jt]sx?$/, '') === stem
  );
}

function target(
  root: string,
  manifest: string,
  value: string,
  line: number,
  rule: 'manifest' | 'tsconfig',
  files: Set<string>,
  out: Found
) {
  const dir = posix.dirname(manifest) === '.' ? '' : posix.dirname(manifest);
  const path = posix.normalize(posix.join(dir, value)).replace(/^\.\//, '');
  if (path.startsWith('..')) return;
  const local = dir ? path.slice(dir.length + 1) : path;
  const output = BUILD_OUTPUT.test(local);
  const exists = files.has(path) || existsSync(join(root, path));
  if (exists)
    out.links.push({
      source: manifest,
      line,
      target: path,
      relation: 'references',
      rule,
      check: 'ok',
    });
  // A build output names no source: with no source map or build entry list, its source is an open question with
  // the same-named sources as candidates (an agent settles it with a link), whether or not the output is built.
  if (output) {
    const candidates = sameNamed(files, dir, local);
    out.unresolved.push({
      source: manifest,
      line,
      text: value,
      rule,
      reason: 'build-output',
      ...(candidates.length && { candidates }),
    });
  } else if (!exists)
    out.unresolved.push({
      source: manifest,
      line,
      text: value,
      rule,
      reason: 'no-such-file',
    });
}

function packageJson(
  root: string,
  file: string,
  files: Set<string>,
  out: Found
) {
  const text = readFileSync(join(root, file), 'utf8');
  let pkg: Record<string, unknown>;
  try {
    pkg = JSON.parse(text);
  } catch {
    return;
  }
  const lines = text.split('\n');
  for (const field of MANIFEST_FIELDS)
    for (const value of leaves(pkg[field]))
      if (field !== 'imports' || value.startsWith('.'))
        target(
          root,
          file,
          value,
          lineOf(lines, field, value),
          'manifest',
          files,
          out
        );
}

// The few Cargo.toml keys that name files: `path` in [lib] and [[bin]], `build` in [package].
function cargoToml(root: string, file: string, files: Set<string>, out: Found) {
  const lines = readFileSync(join(root, file), 'utf8').split('\n');
  let section = '';
  lines.forEach((raw, i) => {
    const line = raw.replace(/#.*$/, '').trim();
    const header = /^\[\[?([^\]]+)\]\]?$/.exec(line);
    if (header) {
      section = header[1].trim();
      return;
    }
    const kv = /^(path|build)\s*=\s*"([^"]+)"/.exec(line);
    if (!kv) return;
    if (
      (kv[1] === 'path' && (section === 'lib' || section === 'bin')) ||
      (kv[1] === 'build' && section === 'package')
    )
      target(root, file, kv[2], i + 1, 'manifest', files, out);
  });
}

// tsconfig is JSON with comments and trailing commas: read the two fields by their quoted values.
function tsconfig(root: string, file: string, files: Set<string>, out: Found) {
  const lines = readFileSync(join(root, file), 'utf8').split('\n');
  lines.forEach((line, i) => {
    const ext = /"extends"\s*:\s*"([^"]+)"/.exec(line);
    if (ext && (ext[1].startsWith('.') || ext[1].startsWith('/')))
      target(
        root,
        file,
        ext[1].endsWith('.json') ? ext[1] : `${ext[1]}.json`,
        i + 1,
        'tsconfig',
        files,
        out
      );
    const ref = /"path"\s*:\s*"([^"]+)"/.exec(line);
    if (ref) {
      const p = ref[1].endsWith('.json') ? ref[1] : `${ref[1]}/tsconfig.json`;
      target(root, file, p, i + 1, 'tsconfig', files, out);
    }
  });
}

/** Links from every manifest and tsconfig in the listing. */
export function fileLinks(root: string, listing: readonly string[]): Found {
  const files = new Set(listing);
  const out: Found = { links: [], unresolved: [] };
  for (const file of listing) {
    const name = posix.basename(file);
    try {
      if (name === 'package.json') packageJson(root, file, files, out);
      else if (name === 'Cargo.toml') cargoToml(root, file, files, out);
      else if (/^tsconfig(\..+)?\.json$/.test(name))
        tsconfig(root, file, files, out);
    } catch {
      // An unreadable manifest names nothing.
    }
  }
  return out;
}
