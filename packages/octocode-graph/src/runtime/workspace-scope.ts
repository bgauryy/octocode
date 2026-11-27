// A build rooted inside a workspace sees the workspace's other packages only from outside: as external packages
// (`pkg:<name>` nodes, when the engine resolves them as installed packages) or as imports it left unresolved out of
// the root (a bare package name, or a relative path into a sibling package). Their files are not in the build, so
// those imports never stitch into code. workspaceExternals finds every workspace enclosing the build root
// (package.json `workspaces`, pnpm-workspace.yaml, a Cargo `[workspace]`, go.work), its members outside the build
// root, and the build's imports that name one of them. The build records them in data.json
// `coverage.workspaceExternals`, brief.txt and its stderr summary, with the command that builds the workspace root,
// where those packages are chunks and their imports stitch. The member holding the build root is the build's own
// package: its imports into the rest of it name the build of that package's root instead.
import { globSync, readFileSync } from 'node:fs';
import { basename, dirname, join, relative, resolve, sep } from 'node:path';
import { graphCommand, shellArg } from './invocation.ts';
import { SKIPPED_DIRS } from './lib.ts';
import type { ExternalPackage, LspMode, WorkspaceExternals } from './types.ts';

export type { WorkspaceExternal, WorkspaceExternals } from './types.ts';

/** A workspace member: a name its packages are imported by, and its directory relative to the workspace root. */
interface Member {
  name: string;
  dir: string;
}

const KIND: Record<string, string> = {
  'package.json workspaces': 'npm',
  'pnpm-workspace.yaml': 'pnpm',
  'Cargo.toml [workspace]': 'Cargo',
  'go.work': 'Go',
};

const read = (file: string) => {
  try {
    return readFileSync(file, 'utf8');
  } catch {
    return null;
  }
};
function readJson(file: string): Record<string, unknown> | null {
  const text = read(file);
  if (text === null) return null;
  try {
    const value: unknown = JSON.parse(text);
    return value && typeof value === 'object'
      ? (value as Record<string, unknown>)
      : null;
  } catch {
    return null;
  }
}
const isString = (v: unknown): v is string => typeof v === 'string';
const posix = (p: string) => p.split(sep).join('/') || '.';
const unquote = (v: string) => v.trim().replace(/^(['"])(.*)\1$/, '$2');
const skipped = new Set(SKIPPED_DIRS);

// Member directories (relative, `/`-separated) holding `file` under the include patterns, less the `!` ones.
function expand(ws: string, patterns: string[], file: string) {
  const clean = (p: string) =>
    p.trim().replace(/^\.\//, '').replace(/\/+$/, '') || '.';
  const glob = (pattern: string) =>
    globSync(pattern, {
      cwd: ws,
      exclude: (name: string) => skipped.has(basename(name)),
    }).map(posix);
  const excluded = new Set(
    patterns
      .filter(p => p.startsWith('!'))
      .flatMap(p => glob(clean(p.slice(1))))
  );
  const dirs = patterns
    .filter(p => !p.startsWith('!'))
    .flatMap(p => glob(`${clean(p)}/${file}`))
    .map(hit => posix(dirname(hit)));
  return [...new Set(dirs)].filter(d => !excluded.has(d)).toSorted();
}

function npmMembers(ws: string): Member[] | null {
  const w = readJson(join(ws, 'package.json'))?.workspaces;
  const listed =
    w && typeof w === 'object' && !Array.isArray(w)
      ? (w as { packages?: unknown }).packages
      : w;
  if (!Array.isArray(listed)) return null;
  return expand(ws, listed.filter(isString), 'package.json').flatMap(dir => {
    const name = readJson(join(ws, dir, 'package.json'))?.name;
    return isString(name) ? [{ name, dir }] : [];
  });
}

// The `packages:` list of pnpm-workspace.yaml: block items under the key, or one flow list.
function pnpmPatterns(text: string) {
  const lines = text.split(/\r?\n/);
  const at = lines.findIndex(l => /^packages\s*:/.test(l));
  if (at < 0) return null;
  const flow = lines[at].match(/^packages\s*:\s*\[(.*)\]/);
  if (flow) return flow[1].split(',').map(unquote).filter(Boolean);
  const items: string[] = [];
  for (const line of lines.slice(at + 1)) {
    if (/^\S/.test(line)) break;
    const item = line.match(/^\s*-\s*(.+?)\s*(?:\s#.*)?$/);
    if (item) items.push(unquote(item[1]));
  }
  return items;
}

function pnpmMembers(ws: string): Member[] | null {
  const text = read(join(ws, 'pnpm-workspace.yaml'));
  const patterns = text === null ? null : pnpmPatterns(text);
  if (!patterns) return null;
  return expand(ws, patterns, 'package.json').flatMap(dir => {
    const name = readJson(join(ws, dir, 'package.json'))?.name;
    return isString(name) ? [{ name, dir }] : [];
  });
}

// One TOML table's lines, comments removed: from its `[name]` header to the next table header.
function tomlTable(text: string, name: string) {
  const lines = text.split(/\r?\n/).map(l => l.replace(/\s#.*$|^#.*$/, ''));
  const at = lines.findIndex(l => l.trim() === `[${name}]`);
  if (at < 0) return null;
  const end = lines.findIndex((l, i) => i > at && /^\s*\[/.test(l));
  return lines.slice(at + 1, end < 0 ? undefined : end).join('\n');
}
const tomlArray = (table: string, key: string) =>
  [
    ...(
      table.match(
        new RegExp(`^\\s*${key}\\s*=\\s*\\[([^\\]]*)\\]`, 'm')
      )?.[1] ?? ''
    ).matchAll(/"([^"]*)"|'([^']*)'/g),
  ].map(m => m[1] ?? m[2]);
const tomlString = (table: string | null, key: string) =>
  table?.match(new RegExp(`^\\s*${key}\\s*=\\s*["']([^"']+)["']`, 'm'))?.[1];

function cargoMembers(ws: string): Member[] | null {
  const text = read(join(ws, 'Cargo.toml'));
  const table = text === null ? null : tomlTable(text, 'workspace');
  if (table === null) return null;
  const patterns = [
    ...tomlArray(table, 'members'),
    ...tomlArray(table, 'exclude').map(p => `!${p}`),
  ];
  return expand(ws, patterns, 'Cargo.toml').flatMap(dir => {
    const manifest = read(join(ws, dir, 'Cargo.toml')) ?? '';
    // A crate is imported by its [lib] name, else its package name.
    const names = [
      tomlString(tomlTable(manifest, 'package'), 'name'),
      tomlString(tomlTable(manifest, 'lib'), 'name'),
    ].filter(isString);
    return [...new Set(names)].map(name => ({ name, dir }));
  });
}

function goMembers(ws: string): Member[] | null {
  const text = read(join(ws, 'go.work'))?.replace(/\/\/.*$/gm, '');
  if (text === undefined) return null;
  const dirs = [
    ...[...text.matchAll(/^\s*use\s*\(([^)]*)\)/gm)].flatMap(m =>
      m[1].split(/\s+/)
    ),
    ...[...text.matchAll(/^\s*use\s+([^\s(]+)/gm)].map(m => m[1]),
  ]
    .map(unquote)
    .filter(Boolean);
  return dirs.flatMap(d => {
    const module = read(join(ws, d, 'go.mod'))?.match(
      /^\s*module\s+(\S+)/m
    )?.[1];
    return module
      ? [{ name: unquote(module), dir: posix(relative(ws, resolve(ws, d))) }]
      : [];
  });
}

const READERS: [string, (ws: string) => Member[] | null][] = [
  ['package.json workspaces', npmMembers],
  ['pnpm-workspace.yaml', pnpmMembers],
  ['Cargo.toml [workspace]', cargoMembers],
  ['go.work', goMembers],
];

// Names compare as their ecosystems spell them: case aside, and Cargo's `a-b` is imported as `a_b`.
const norm = (name: string) => name.toLowerCase().replaceAll('-', '_');

// The members an imported name names: the longest member name it equals or lies under (`@s/a/sub`, a Go package
// in a module), else every member module under it (a Go import path the graph shortened to host/owner/repo).
function membersNamed(name: string, members: Member[]) {
  const n = norm(name);
  const within = members.filter(m => {
    const k = norm(m.name);
    return n === k || n.startsWith(`${k}/`);
  });
  if (within.length) {
    const longest = Math.max(...within.map(m => m.name.length));
    return within.filter(m => m.name.length === longest);
  }
  return members.filter(m => norm(m.name).startsWith(`${n}/`));
}

/** An import the build left unresolved out of its root (stitch.ts UnresolvedImport): its root-relative file. */
export interface OutsideImportRow {
  source: string;
  specifier: string | null;
}

/** What the build imported from outside its root: package nodes and unresolved out-of-root imports. */
export interface ScopeImports {
  externals?: readonly ExternalPackage[];
  unresolved?: readonly OutsideImportRow[];
}

// The members an unresolved import names: a relative or absolute path inside a member directory (the deepest), a
// Rust path's crate, or a bare package name.
function membersImported(
  u: OutsideImportRow,
  base: string,
  ws: string,
  members: Member[]
) {
  const spec = u.specifier?.trim();
  if (!spec) return [];
  if (spec.startsWith('.') || spec.startsWith('/')) {
    const target = resolve(base, dirname(u.source), spec);
    const holding = members.filter(m => {
      const dir = resolve(ws, m.dir);
      return target === dir || target.startsWith(dir + sep);
    });
    const deepest = Math.max(...holding.map(m => m.dir.length));
    return holding.filter(m => m.dir.length === deepest);
  }
  const head = spec.includes('::') ? spec.split('::')[0] : spec;
  return ['crate', 'self', 'super', ''].includes(head)
    ? []
    : membersNamed(head, members);
}

/** Build options the workspace build repeats. */
export interface ScopeOptions {
  env?: NodeJS.ProcessEnv;
  /** The build's language-server mode, when not the default. */
  lsp?: LspMode;
  exclude?: string[];
}

// The build's imports that name these members, one row per package name: a package node's counts, joined by the
// unresolved imports naming the same member; most imported first.
function importRows(
  members: Member[],
  { externals = [], unresolved = [] }: ScopeImports,
  base: string,
  ws: string
) {
  const rows = new Map<
    string,
    { id?: string; paths: Set<string>; imports: number; files: Set<string> }
  >();
  const add = (
    name: string,
    named: Member[],
    imports: number,
    files: readonly string[],
    id?: string
  ) => {
    const row = rows.get(name) ?? {
      paths: new Set<string>(),
      imports: 0,
      files: new Set<string>(),
    };
    if (id) row.id = id;
    for (const m of named) row.paths.add(m.dir);
    row.imports += imports;
    for (const f of files) row.files.add(f);
    rows.set(name, row);
  };
  if (members.length) {
    for (const x of externals) {
      const name = x.id.replace(/^pkg:/, '');
      const named = membersNamed(name, members);
      if (named.length) add(name, named, x.count, x.files, x.id);
    }
    for (const u of unresolved) {
      const named = membersImported(u, base, ws, members);
      if (!named.length) continue;
      const external = [...rows].find(([, r]) =>
        named.every(m => r.paths.has(m.dir))
      )?.[0];
      add(external ?? named[0].name, named, 1, [u.source]);
    }
  }
  return [...rows]
    .map(([name, r]) => ({
      ...(r.id && { id: r.id }),
      name,
      paths: [...r.paths].toSorted(),
      imports: r.imports,
      files: r.files.size,
    }))
    .toSorted((a, b) => b.imports - a.imports || a.name.localeCompare(b.name));
}

/**
 * The workspaces enclosing `root` (nearest first) whose members outside `root` the build imported as external
 * packages, with each package's import count and the command that builds the workspace root. A member holding
 * `root` is the build's own package, never a sibling: imports into the rest of it come first, as an entry of
 * scope `package` whose command builds that package's root, the smaller build that stitches them.
 */
export function workspaceExternals(
  root: string,
  imports: ScopeImports,
  { env, lsp, exclude = [] }: ScopeOptions = {}
): WorkspaceExternals[] {
  const base = resolve(root);
  const inside = (dir: string) => dir === base || dir.startsWith(base + sep);
  const holds = (dir: string) => base.startsWith(dir + sep);
  const found: WorkspaceExternals[] = [];
  if (!imports.externals?.length && !imports.unresolved?.length) return found;
  const build = (dir: string) =>
    [
      graphCommand(env),
      'build',
      shellArg(dir),
      ...(lsp ? ['--lsp', lsp] : []),
      ...(exclude.length ? ['--exclude', shellArg(exclude.join(','))] : []),
    ].join(' ');
  const own: WorkspaceExternals[] = [];
  for (let ws = dirname(base); ; ws = dirname(ws)) {
    const manifests: string[] = [];
    const members: Member[] = [];
    const holding: Member[] = [];
    for (const [manifest, readMembers] of READERS) {
      const listed = readMembers(ws);
      if (!listed) continue;
      manifests.push(manifest);
      for (const m of listed) {
        const dir = resolve(ws, m.dir);
        if (holds(dir)) holding.push(m);
        else if (!inside(dir)) members.push(m);
      }
    }
    // The deepest member holding the root is its package; a package already reported by a nearer workspace stays.
    const deepest = Math.max(...holding.map(m => m.dir.length));
    const pkg = holding.filter(m => m.dir.length === deepest);
    const pkgRoot = pkg.length ? resolve(ws, pkg[0].dir) : null;
    if (pkgRoot && !own.some(w => w.root === pkgRoot)) {
      const packages = importRows(pkg, imports, base, ws);
      if (packages.length)
        own.push({
          root: pkgRoot,
          scope: 'package',
          manifests,
          packages,
          command: build(pkgRoot),
        });
    }
    const packages = importRows(members, imports, base, ws);
    if (packages.length)
      found.push({ root: ws, manifests, packages, command: build(ws) });
    if (ws === dirname(ws)) return [...own, ...found];
  }
}

const count = (n: number, word: string) => `${n} ${word}${n === 1 ? '' : 's'}`;

/** One line naming a workspace's packages the build treated as externals and the build that stitches them. */
export function workspaceNote(w: WorkspaceExternals) {
  if (w.scope === 'package') {
    const imports = w.packages.reduce((sum, p) => sum + p.imports, 0);
    return `the build root lies inside package ${w.packages.map(p => p.name).join(', ')} at ${w.root}: ${count(imports, 'import')} leave it into the rest of that package and end there, not stitched. To follow them, build the package root: ${w.command}`;
  }
  const n = w.packages.length;
  const kinds = [...new Set(w.manifests.map(m => KIND[m] ?? m))].join(' and ');
  return `${count(n, 'package')} of the ${kinds} workspace at ${w.root} ${n === 1 ? 'was treated as an external' : 'were treated as externals'}, not stitched: ${w.packages.map(p => `${p.name} (${count(p.imports, 'import')})`).join(', ')}; their imports end at a package node. To follow them into those packages, build the workspace root (a larger build): ${w.command}`;
}
