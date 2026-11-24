// Chunk membership and stitching: one root-relative graph from separately built chunk snapshots.
import { existsSync, readFileSync } from 'node:fs';
import { dirname, join, posix, relative, sep } from 'node:path';
import { degrees, isInteger, topGroup } from './lib.ts';
import { chunkCoverage } from './coverage.ts';
import type {
  CallEdge,
  Chunk,
  CycleRow,
  Declaration,
  ExternalPackage,
  FileNode,
  GraphData,
  ImportEdge,
  Insights,
  OutsideImport,
  SectionData,
  SymbolNode,
  UnresolvedCall,
  UseEdge,
} from './types.ts';
import { fileOf, prefixOf } from '../ids.ts';
import type { Load } from '../loads.ts';

/** A chunk as stitching reads it: its id, path, workspace package and the directory names it excludes. */
export type StitchChunk = Pick<Chunk, 'id' | 'path' | 'package' | 'exclude'>;

/** How Rust paths resolve: the stitched root's module path inside its crate, and the workspace crates by name. */
export interface RustScope {
  cratePrefix?: string[] | null;
  crates?: ReadonlyMap<string, string | null> | null;
}

/** An import between chunks: a relative or crate-path import of a file, or an import of a workspace package. */
export interface CrossImport {
  /** The importing file; null for a package import an older build counted without naming its importers. */
  source: string | null;
  target: string;
  line?: number;
  test?: boolean;
  via: 'relative' | 'crate-path' | 'package';
  /** The import's kinds as the importing chunk recorded them (static-import, type-import, …); absent when unknown. */
  kinds?: string[];
  /** The load the importing chunk's engine stated per kind, when it did. */
  loads?: Record<string, Load>;
  crate?: string;
  package?: string;
  count?: number;
  sourceChunk: string;
  targetChunk: string;
  targetBuilt: boolean;
}

/** An out-of-root import stitching could not bind, with the reason. */
export interface UnresolvedImport {
  source: string;
  line?: number;
  specifier: string | null;
  reason: string;
}

/** Where a Rust path starts: a module directory, its declaring file when known, and the segments left. */
interface RustAnchor {
  dir: string;
  file?: string;
  rest: string[];
}

/** The module file a Rust path names and the item segments left after it. */
interface RustModule {
  file: string;
  rest: string[];
}

type CrateItem = (path: string) => RustModule | null;

const EXT = [
  '.ts',
  '.tsx',
  '.mts',
  '.cts',
  '.d.ts',
  '.js',
  '.jsx',
  '.mjs',
  '.cjs',
  '.json',
  '.py',
  '.rs',
  '.go',
  '.java',
  '.scala',
  '.cs',
  '.c',
  '.h',
  '.cc',
  '.cpp',
  '.hpp',
  '.cu',
  '.cuh',
];
const JS_TO_TS: Record<string, string[]> = {
  '.js': ['.ts', '.tsx'],
  '.mjs': ['.mts'],
  '.cjs': ['.cts'],
  '.jsx': ['.tsx'],
};

// The chunk that ingests `path` (root-relative): the deepest chunk containing it, unless an
// excluded directory name prunes it inside that chunk (ingest excludes names, not paths).
export function chunkOf<C extends Pick<Chunk, 'path' | 'exclude'>>(
  path: string,
  chunks: readonly C[]
): C | null {
  let best: C | null = null;
  for (const c of chunks)
    if (
      (c.path === '.' ||
        path === c.path ||
        path.startsWith(prefixOf(c.path))) &&
      (!best || c.path.length > best.path.length || best.path === '.')
    )
      best = c;
  if (!best) return null;
  const inner = path.slice(prefixOf(best.path).length).split('/').slice(0, -1);
  return best.exclude?.some(name => inner.includes(name)) ? null : best;
}

// A relative import specifier to a root-relative file that `has` knows, by the usual module suffixes;
// Rust `crate::`, `super::` and `self::` paths, and paths into a workspace crate of `rust.crates`, by the module tree.
// `rust` is `{ cratePrefix, crates }`: the stitched root's module path and the workspace crates (both optional).
export function resolveImport(
  from: string,
  specifier: string | null | undefined,
  has: (path: string) => boolean,
  rust: RustScope = {}
) {
  // A Rust path form, or a specifier whose first segment names a workspace crate: the specifier is that text.
  if (
    /^(crate|super|self)::/.test(specifier ?? '') ||
    rust.crates?.has(rustHead(specifier))
  )
    return resolveRustPath(from, specifier!, has, rust);
  if (!specifier || !/^\.\.?(\/|$)/.test(specifier)) return null;
  const target = posix.normalize(posix.join(posix.dirname(from), specifier));
  if (target.startsWith('../')) return null;
  const ext = posix.extname(target);
  const stem = target.slice(0, target.length - ext.length);
  const tries = [
    target,
    ...(JS_TO_TS[ext] ?? []).map(e => stem + e),
    ...EXT.map(e => target + e),
    ...EXT.map(e => `${target}/index${e}`),
  ];
  return tries.find(has) ?? null;
}

// A Rust module's directory: `a/b.rs` owns `a/b/`; `mod.rs`, `lib.rs` and `main.rs` own their directory.
const moduleDir = (file: string) =>
  /(^|\/)(mod|lib|main)\.rs$/.test(file)
    ? posix.dirname(file)
    : file.replace(/\.rs$/, '');
// The file that declares the module living in `dir`: `dir.rs`, `dir/mod.rs`, or the crate root in `src`.
const moduleFile = (dir: string, has: (path: string) => boolean) =>
  (dir === '.'
    ? ['mod.rs', 'lib.rs', 'main.rs']
    : [`${dir}.rs`, `${dir}/mod.rs`, `${dir}/lib.rs`, `${dir}/main.rs`]
  ).find(has) ?? null;
const childDir = (dir: string, seg: string) =>
  dir === '.' ? seg : `${dir}/${seg}`;

// A root inside a crate (`crates/x/src/tools`): the module path from the crate's `src` to the root
// (`['tools']`), so `crate::tools::a` names the root's `a`. Null when no Cargo.toml is above the root.
export function crateModulePrefix(root: string) {
  for (let dir = root; dir !== dirname(dir); dir = dirname(dir)) {
    if (existsSync(join(dir, 'Cargo.toml'))) {
      const rel = relative(join(dir, 'src'), root);
      return rel.startsWith('..') ? null : rel.split(sep).filter(Boolean);
    }
  }
  return null;
}

const rustHead = (specifier: string | null | undefined) =>
  String(specifier).split('::')[0];

// Workspace library crates by the name a Rust path spells: `[lib] name`, else the package name with `-` as `_`,
// to the crate root file (`[lib] path`, else `src/lib.rs`). Read from every Cargo.toml in `files`, since a crate can be
// split over several chunks. A name two crates share maps to null: a path cannot tell them apart.
export function workspaceCrates(root: string, files: string[]) {
  const has = new Set(files);
  const crates = new Map<string, string | null>();
  for (const manifest of files.filter(
    p => posix.basename(p) === 'Cargo.toml'
  )) {
    let toml = '';
    try {
      toml = readFileSync(join(root, manifest), 'utf8');
    } catch {
      /* unreadable: the defaults below */
    }
    const lib = tomlTable(toml, 'lib');
    const name = (lib.name ?? tomlTable(toml, 'package').name)?.replaceAll(
      '-',
      '_'
    );
    const file = posix.join(posix.dirname(manifest), lib.path ?? 'src/lib.rs');
    if (name && has.has(file)) crates.set(name, crates.has(name) ? null : file);
  }
  return crates;
}

// The string keys of one TOML table (`[lib]`), enough for Cargo's `name` and `path`.
function tomlTable(text: string, table: string) {
  const out: Record<string, string | undefined> = {};
  let current = null;
  for (const line of text.split('\n')) {
    const head = line.match(/^\s*\[([^\]]+)\]/);
    if (head) current = head[1].trim();
    else if (current === table) {
      const kv = line.match(/^\s*([\w-]+)\s*=\s*"([^"]*)"/);
      if (kv) out[kv[1]] = kv[2];
    }
  }
  return out;
}

// Where a Rust path starts: another workspace crate's root, the crate's `src` for `crate::`, else this module walked
// up by `super`. A crate root file owns its own directory whatever its name.
function rustAnchor(
  from: string | null,
  parts: string[],
  has: (path: string) => boolean,
  { cratePrefix = null, crates = null }: RustScope = {}
): RustAnchor | null {
  if (crates?.has(parts[0])) {
    const file = crates.get(parts[0]);
    return file
      ? { dir: posix.dirname(file), file, rest: parts.slice(1) }
      : null;
  }
  // Only a workspace crate path is resolved without a source file (stitch's crateItem passes none).
  if (parts[0] !== 'crate') return relativeAnchor(from!, parts);
  return crateAnchor(from!, parts, has, cratePrefix);
}

// `self::`/`super::` paths: this module's directory walked up once per `super`.
function relativeAnchor(from: string, parts: string[]) {
  let dir = moduleDir(from),
    i = 0;
  for (; parts[i] === 'self' || parts[i] === 'super'; i++)
    if (parts[i] === 'super') dir = posix.dirname(dir);
  return { dir, rest: parts.slice(i) };
}

// `crate::` paths: the nearest Cargo.toml's `src`, else the root's own module path stripped.
function crateAnchor(
  from: string,
  parts: string[],
  has: (path: string) => boolean,
  cratePrefix: string[] | null
) {
  for (let d = posix.dirname(from); ; d = posix.dirname(d)) {
    if (has(d === '.' ? 'Cargo.toml' : `${d}/Cargo.toml`))
      return { dir: d === '.' ? 'src' : `${d}/src`, rest: parts.slice(1) };
    if (d === '.' || d === '/') break;
  }
  // The crate root is above the stitched root: strip the root's own module path.
  const rest = parts.slice(1);
  return cratePrefix && cratePrefix.every((seg, i) => rest[i] === seg)
    ? { dir: '.', rest: rest.slice(cratePrefix.length) }
    : null;
}

// The deepest module file a `crate::`/`super::`/`self::`/workspace-crate path names; trailing segments are items in it.
export function resolveRustPath(
  from: string,
  specifier: string,
  has: (path: string) => boolean,
  rust: RustScope = {}
) {
  const target = rustModule(from, specifier, has, rust)?.file;
  return target && target !== from ? target : null;
}

// That module file and the item segments left after it.
function rustModule(
  from: string | null,
  specifier: string,
  has: (path: string) => boolean,
  rust: RustScope
): RustModule | null {
  const anchor = rustAnchor(from, specifier.split('::'), has, rust);
  if (!anchor) return null;
  let { dir } = anchor;
  let file = anchor.file ?? moduleFile(dir, has);
  let i = 0;
  for (; i < anchor.rest.length; i++) {
    const next = moduleFile(childDir(dir, anchor.rest[i]), has);
    if (!next || /(^|\/)(lib|main)\.rs$/.test(next)) break;
    [dir, file] = [childDir(dir, anchor.rest[i]), next];
  }
  return file ? { file, rest: anchor.rest.slice(i) } : null;
}

// A Rust path naming a module above the stitched root: a `crate::` path outside the root's module
// prefix, or a `super::` chain that climbs past it.
function leavesRoot(
  from: string,
  specifier: string | null,
  cratePrefix: string[] | null
) {
  const parts = String(specifier).split('::');
  if (parts[0] === 'crate')
    return (
      Boolean(cratePrefix) &&
      !cratePrefix?.every((seg, i) => parts[i + 1] === seg)
    );
  const climbs = parts.findIndex(p => p !== 'super');
  return (
    parts[0] === 'super' &&
    climbs >
      moduleDir(from)
        .split('/')
        .filter(p => p && p !== '.').length
  );
}

// Workspace package names as the graph prints them: `pkg:@scope/name/sub` and Cargo `a-b` vs `a_b`.
export const packageKey = (id: string | undefined) => {
  const name = String(id).replace(/^pkg:/, '');
  const parts = name.split('/');
  return (name.startsWith('@') ? parts.slice(0, 2) : parts.slice(0, 1))
    .join('/')
    .toLowerCase()
    .replaceAll('-', '_');
};

/** What every import of a chunk is stitched against. */
interface StitchContext {
  chunks: readonly StitchChunk[];
  datas: ReadonlyMap<string, SectionData>;
  has: Set<string>;
  cratePrefix: string[] | null;
  crates: Map<string, string | null>;
  byPackage: Map<string, StitchChunk>;
}

export interface StitchInput {
  root: string;
  chunks: readonly StitchChunk[];
  datas: ReadonlyMap<string, SectionData>;
  files: string[];
}

// chunks: plan rows ({id, path, package, exclude}); datas: Map chunk id -> its data.json; files: every root-relative file.
export function stitch({ root, chunks, datas, files }: StitchInput) {
  const ctx: StitchContext = {
    chunks,
    datas,
    has: new Set(files),
    cratePrefix: crateModulePrefix(root),
    crates: workspaceCrates(root, files),
    byPackage: new Map(
      chunks.filter(c => c.package).map(c => [packageKey(c.package), c])
    ),
  };
  const cross: CrossImport[] = [];
  const unresolved: UnresolvedImport[] = [];
  for (const c of chunks.filter(x => datas.has(x.id))) {
    for (const o of datas.get(c.id)?.outsideRoot ?? [])
      stitchRelative(ctx, c, o, { cross, unresolved });
    for (const x of datas.get(c.id)?.externals ?? [])
      stitchPackage(ctx, c, x, cross);
  }
  const testSources = new Set(
    chunks.flatMap(c =>
      (datas.get(c.id)?.files ?? [])
        .filter(f => f.roles?.includes('test'))
        .map(f => prefixOf(c.path) + f.id)
    )
  );
  for (const e of cross)
    if (e.source !== null && testSources.has(e.source)) e.test = true;
  const has = (p: string) => ctx.has.has(p);
  const crateItem = (path: string) =>
    ctx.crates.has(rustHead(path))
      ? rustModule(null, path, has, { crates: ctx.crates })
      : null;
  const crossCalls = bindCalls({ chunks, datas, cross, crateItem });
  for (const e of crossCalls)
    if (testSources.has(fileOf(e.source))) e.test = true;
  return {
    cross,
    unresolved,
    crossCalls,
    whole: whole({ root, chunks, datas, cross, crossCalls }),
  };
}

// One out-of-root import of chunk `c`: an edge to the chunk that holds its target, or an unresolved row with the reason.
// A Rust path into another workspace crate (`octocode_io::memo::Memo`) is a `crate-path` edge naming that crate.
function stitchRelative(
  { chunks, datas, has, cratePrefix, crates, byPackage }: StitchContext,
  c: StitchChunk,
  o: OutsideImport,
  {
    cross,
    unresolved,
  }: { cross: CrossImport[]; unresolved: UnresolvedImport[] }
) {
  const source = prefixOf(c.path) + o.file;
  const target = resolveImport(source, o.specifier, p => has.has(p), {
    cratePrefix,
    crates,
  });
  const owner = target && chunkOf(target, chunks);
  const gap = (reason: string) =>
    unresolved.push({ source, line: o.line, specifier: o.specifier, reason });
  const crate = crates.has(rustHead(o.specifier))
    ? rustHead(o.specifier)
    : null;
  const via: Pick<CrossImport, 'via' | 'crate'> = crate
    ? { via: 'crate-path', crate }
    : { via: 'relative' };
  // A bare specifier naming another chunk's workspace package (the engine saw its directory outside the chunk root,
  // so it reported a scope gap rather than a package): the same package edge stitchPackage makes.
  const bare =
    o.specifier && !/^[./]/.test(o.specifier) && !o.specifier.includes('::');
  const pkgOwner =
    !target && bare ? byPackage.get(packageKey(o.specifier!)) : undefined;
  // Inside its own chunk the engine already decided; an import it left unlinked stays unlinked.
  if (owner === c) gap('unlinked inside its own chunk');
  else if (pkgOwner && pkgOwner !== c)
    cross.push({
      source,
      target: `pkg:${pkgOwner.package}`,
      package: `pkg:${o.specifier}`,
      line: o.line,
      ...(o.test && { test: true }),
      ...(o.importKind && { kinds: [o.importKind] }),
      ...(o.importKind && o.load && { loads: { [o.importKind]: o.load } }),
      via: 'package',
      sourceChunk: c.id,
      targetChunk: pkgOwner.id,
      targetBuilt: datas.has(pkgOwner.id),
    });
  else if (owner)
    cross.push({
      source,
      target,
      line: o.line,
      ...(o.test && { test: true }),
      ...(o.importKind && { kinds: [o.importKind] }),
      ...(o.importKind && o.load && { loads: { [o.importKind]: o.load } }),
      ...via,
      sourceChunk: c.id,
      targetChunk: owner.id,
      targetBuilt: datas.has(owner.id),
    });
  else if (target) gap('target file is in no chunk');
  else if (crate && !crates.get(crate))
    gap('crate name is shared by several workspace crates');
  else
    gap(
      leavesRoot(source, o.specifier, cratePrefix)
        ? 'outside the stitched root'
        : unknownCrate(source, o.specifier)
          ? 'names no workspace crate'
          : 'no file matches the specifier'
    );
}

// A Rust `use` path from a Rust file whose first segment is a crate name the workspace does not build.
const unknownCrate = (source: string, specifier: string | null) =>
  source.endsWith('.rs') &&
  /^[A-Za-z_]\w*::/.test(specifier ?? '') &&
  !/^(crate|super|self)::/.test(specifier ?? '');

// A package import of chunk `c` that names another chunk's workspace package: one edge per importing file.
function stitchPackage(
  { datas, byPackage }: StitchContext,
  c: StitchChunk,
  x: ExternalPackage,
  cross: CrossImport[]
) {
  const owner = byPackage.get(packageKey(x.id));
  if (!owner || owner === c) return;
  const row = {
    target: `pkg:${owner.package}`,
    package: x.id,
    via: 'package' as const,
    sourceChunk: c.id,
    targetChunk: owner.id,
    targetBuilt: datas.has(owner.id),
  };
  // Data from older builds counts importers without naming them: keep the chunk-level weight.
  if (!x.files) cross.push({ ...row, source: null, count: x.count });
  const testFiles = new Set(x.testFiles);
  for (const file of x.files ?? [])
    cross.push({
      ...row,
      source: prefixOf(c.path) + file,
      ...(testFiles.has(file) && { test: true }),
      ...(x.kinds?.[file] && { kinds: x.kinds[file] }),
      ...(x.loads && { loads: x.loads }),
    });
}

export interface BindInput {
  chunks: readonly StitchChunk[];
  datas: ReadonlyMap<string, SectionData>;
  cross: CrossImport[];
  crateItem?: CrateItem;
}

// A call site a chunk left unbound binds into another chunk when exactly one declaration matches, looked up only
// where the caller's file reaches (`medium`, `stitched-import`):
// - a bare name (`read_config`): a top-level declaration in a file it imports from another chunk, then one re-export
//   hop inside the target chunk (NameIndex.lookupThrough). A name the engine saw imported (`other`: an import that
//   did not link) is also looked up through the modules of the caller's own chunk that it imports
//   (`use crate::config::read` where config re-exports another crate's `read`); a name nothing imported
//   (`not-declared-in-repo`: a std macro such as `format!`, a prelude name) is not, since those modules reach much
//   more. A macro invocation `name!` binds only a macro, a call `name(..)` only a function (NameIndex `spelling`);
// - `Type::member` (`Continuation::related_input`, a type imported from another crate): the member declared as
//   `file#Type.member` there;
// - a workspace crate path (`octocode_io::memo::fresh`, the engine spells imported names so): the item the module
//   file that path names declares (`crateItem`), or its `Type::member`.
// - a call on a typed receiver (`client.open(..)` with `client: &octocode_io::Client`, the engine's `receiverType`):
//   that type's member, by the two rules above (`typedReceiver`).
// The caller's own file wins: a name it declares itself (a test's own `cut`) binds nothing elsewhere. Calls on a
// receiver (`x.read()`) and within one chunk stay with the engine. Everything else stays unbound in the chunk's
// evidence: a repository-wide unique-name fallback measured mostly wrong on Rust.
export function bindCalls({
  chunks,
  datas,
  cross,
  crateItem = () => null,
}: BindInput) {
  const declared = Map.groupBy(chunkDeclarations(chunks, datas), d => d.file);
  const imports = chunkImports(chunks, datas);
  const reach = importTargets(cross, declared, chunks, imports);
  const index = new NameIndex(declared, imports);
  // The files of each chunk that declare anything: a Rust crate's files, for a member in an impl block elsewhere.
  const filesByChunk = Map.groupBy(declared.keys(), f => chunkOf(f, chunks));
  const crateFiles = (file: string) =>
    filesByChunk.get(chunkOf(file, chunks)) ?? [];
  const by = new Map<string, CallEdge>();
  for (const c of chunks)
    for (const u of callSites(
      (datas.get(c.id)?.unresolvedCalls ?? []).map(typedReceiver)
    )) {
      const file = prefixOf(c.path) + u.file;
      if (index.declares(file, u.targetName)) continue;
      const bound =
        crateCall(u, crateItem, index, crateFiles) ??
        candidates(u, reach.get(file), index, crateFiles);
      const exact = bound.found;
      if (exact.length !== 1 || chunkOf(exact[0].file, chunks) === c) continue;
      const source = prefixOf(c.path) + u.from,
        k = `${source}>${exact[0].id}`;
      // A test caller's file role marks the edge test code once stitched (stitch).
      const e = by.get(k) ?? {
        source,
        target: exact[0].id,
        line: u.line,
        lines: [],
        test: false,
        count: 0,
        // A crate path names its module file and item, as the compiler resolves it (93% of these confirmed by LSP on
        // the 3,516-file native workspace, the rest calls into dev-dependency test helpers it leaves unanswered).
        confidence: bound.rule === 'crate-path' ? 'high' : 'medium',
        // The rule that bound it (BoundBy), after the stitch that made it.
        resolution: ['stitched-import', bound.rule],
        via: 'graph',
        crossChunk: true,
      };
      e.count++;
      if (isInteger(u.line) && !e.lines.includes(u.line)) e.lines.push(u.line);
      by.set(k, e);
    }
  return [...by.values()];
}

// The declarations a call by a workspace crate path names: the top-level item that path's module file declares
// when one segment (the item) follows the module, or the member `Type::member` when two do. A Rust type's member may
// be declared in an impl block in any file of its crate (the engine links within a crate by the same rule): with none
// in the type's file or one import away, it is that type's member in the crate's other files. Null when the path names
// no workspace crate.
function crateCall(
  u: UnresolvedCall,
  crateItem: CrateItem,
  index: NameIndex,
  crateFiles: (file: string) => readonly string[]
): Bound | null {
  if (u.targetName.includes('.')) return null;
  const at = crateItem(u.targetName);
  if (!at) return null;
  if (at.rest.length === 1)
    return {
      found: index.lookupThrough([at.file], at.rest[0]),
      rule: 'crate-path',
    };
  if (at.rest.length !== 2 || !isTypeName(at.rest[0]))
    return { found: [], rule: 'crate-path' };
  const [type, name] = at.rest;
  const near = index.membersThrough([at.file], type, name);
  return {
    found: near.length
      ? near
      : index.crateMembers([at.file], type, name, crateFiles),
    rule: 'crate-path',
  };
}

/**
 * How a call between chunks was bound, kept in its `resolution` after `stitched-import`:
 * `crate-path` (a workspace crate path names the module file), `package` (a name imported from a workspace package),
 * `import` (a name or `Type::member` declared in a file the caller imports from another chunk, or one re-export hop
 * from it), `own-module` (an imported name reached through a module of the caller's own chunk).
 */
type BoundBy = 'crate-path' | 'package' | 'import' | 'own-module';
interface Bound {
  found: Declaration[];
  rule: BoundBy;
}

// The engine records a call whose result is used in a chain as one row naming the whole chain
// (`read_config().map_err`, `crate_x::load()?.ok_or`): that row still witnesses a call of its first callee at that
// line. Each row is reduced to the callee it starts with, once per caller, line and callee.
function callSites(rows: readonly UnresolvedCall[]) {
  const seen = new Set<string>();
  return rows.flatMap(u => {
    const paren = u.targetName.indexOf('(');
    const targetName = paren < 0 ? u.targetName : u.targetName.slice(0, paren);
    const key = `${u.from}\0${u.line}\0${targetName}`;
    if (!targetName || seen.has(key)) return [];
    seen.add(key);
    return [{ ...u, targetName }];
  });
}

// A call on a receiver whose type the source spells (`client: &base_io::client::Client`, then `client.open(..)`) is a
// call of that type's member: spelled `base_io::client::Client::open`, it binds through the crate-path and
// `Type::member` rules, and only to a member that type declares. A chain row (`client.close().await.expect`) keeps
// its head: its receiver type is its outer receiver's. Generic arguments are no part of the name (`Arc<Client>` names
// Arc, which declares nothing here, so it binds nothing).
function typedReceiver(u: UnresolvedCall): UnresolvedCall {
  if (!u.receiverType || u.targetName.includes('(')) return u;
  const member = /^[^.:]+\.([^.:]+)$/.exec(u.targetName);
  const type = u.receiverType.split('<')[0].trim();
  return member && type ? { ...u, targetName: `${type}::${member[1]}` } : u;
}

// File-level imports inside every chunk, root-relative: a module file reaches the files it imports, which is where
// a name it re-exports (`pub use edit::read`, `export { read } from './edit'`) is declared.
function chunkImports(
  chunks: readonly StitchChunk[],
  datas: ReadonlyMap<string, Pick<SectionData, 'imports'>>
) {
  const imports = new Map<string, string[]>();
  for (const c of chunks)
    for (const e of datas.get(c.id)?.imports ?? []) {
      const source = prefixOf(c.path) + e.source;
      const list = imports.get(source) ?? [];
      list.push(prefixOf(c.path) + e.target);
      imports.set(source, list);
    }
  return imports;
}

/** What one importing file reaches in other chunks (importTargets). */
interface Reach {
  /** Files it imports from other chunks. */
  files: string[];
  /** Files the modules of its own chunk that it imports reach in other chunks (re-exports). */
  through: string[];
  packages: Map<string, string>;
  chunkFiles: Map<string | undefined, string[]>;
}

// Reasons that mean the caller imported the name or its type: the engine saw the `use` and could not follow it.
const IMPORTED = new Set([
  'other',
  'type-qualified-unmatched',
  'constructor-target-unavailable',
]);

// The declarations an unbound call can name from what its file reaches:
// - `external-package`: the name came through a `use`/import of a package; it may bind into the imported workspace
//   packages' chunks, and a qualified call (`crate_x::path::f`) only into the package its first segment names.
// - any other reason (`not-declared-in-repo`, `other`: an import that did not link, `type-qualified-unmatched`,
//   `constructor-target-unavailable`): a bare name or `Type::member` in the files the caller reaches in other
//   chunks, and for an imported name (IMPORTED) also through the modules of its own chunk.
function candidates(
  u: UnresolvedCall,
  reach: Reach | undefined,
  index: NameIndex,
  crateFiles: (file: string) => readonly string[]
): Bound {
  const none: Bound = { found: [], rule: 'import' };
  if (!reach) return none;
  const parts = u.targetName.split('::');
  // split returns at least one part.
  const name = parts.at(-1)!;
  if (name.includes('.')) return none;
  if (u.reason === 'external-package') {
    const chunkIds =
      parts.length > 1
        ? [reach.packages.get(packageKey(parts[0]))].filter(Boolean)
        : [...reach.packages.values()];
    return {
      found: index.lookup(
        chunkIds.flatMap(id => reach.chunkFiles.get(id) ?? []),
        name
      ),
      rule: 'package',
    };
  }
  const member = parts.length === 2 && isTypeName(parts[0]);
  if (parts.length !== 1 && !member) return none;
  const lookup = (files: readonly string[]) => {
    if (!member) return index.lookupThrough(files, name);
    const near = index.membersThrough(files, parts[0], name);
    return near.length
      ? near
      : index.crateMembers(files, parts[0], name, crateFiles);
  };
  // Files the caller imports from other chunks first; an imported name then through its own chunk's modules.
  const direct = lookup(reach.files);
  if (direct.length || !IMPORTED.has(u.reason) || !reach.through.length)
    return { found: direct, rule: 'import' };
  return { found: lookup(reach.through), rule: 'own-module' };
}

// A Rust or TypeScript type name: `Type::member` qualifies a member, `module::f` a module path.
const isTypeName = (part: string) => /^[A-Z]/.test(part);

// Top-level callable declarations by file, looked up by the name a call spells over a set of files (one Map per
// file, built once).
class NameIndex {
  readonly byFile: Map<string, Map<string, Declaration[]>>;
  /** Callable members by file and `Type.member` (a declaration stored as `file#Type.member`). */
  readonly membersByFile: Map<string, Map<string, Declaration[]>>;
  readonly imports: Map<string, string[]>;
  /** The type names each file declares (a struct, enum, trait or type alias: neither callable nor a member). */
  readonly typesByFile: Map<string, Set<string>>;
  constructor(
    declared: Map<string, Declaration[]>,
    imports = new Map<string, string[]>()
  ) {
    this.byFile = new Map(
      [...declared].map(([file, list]) => [
        file,
        Map.groupBy(list.filter(topLevel), spelling),
      ])
    );
    this.membersByFile = new Map(
      [...declared].map(([file, list]) => [
        file,
        Map.groupBy(list.filter(member), memberKey),
      ])
    );
    this.imports = imports;
    this.typesByFile = new Map(
      [...declared].map(([file, list]) => [
        file,
        new Set(list.filter(declaresType).map(d => d.name)),
      ])
    );
  }
  /** Whether `file` itself declares the name a call spells (its last `::` segment, or `Type::member`). */
  declares(file: string, targetName: string) {
    const parts = targetName.split('::');
    const name = parts.at(-1)!;
    return (
      (this.byFile.get(file)?.has(name) ?? false) ||
      (parts.length >= 2 &&
        (this.membersByFile.get(file)?.has(`${parts.at(-2)}.${name}`) ?? false))
    );
  }
  /** The members `type.name` declared in the files, then (none there) the exported ones one import hop away. */
  membersThrough(files: readonly string[], type: string, name: string) {
    const key = `${type}.${name}`;
    const at = (list: readonly string[]) =>
      [...new Set(list)].flatMap(
        f => this.membersByFile.get(f)?.get(key) ?? []
      );
    const direct = at(files);
    if (direct.length) return direct;
    return at(files.flatMap(f => this.imports.get(f) ?? [])).filter(
      d => d.exported
    );
  }
  /**
   * A Rust type's member declared in an impl block anywhere in its crate (the engine links within a crate by the same
   * rule): for each file that declares `type` (one of `files`, or one re-export hop from them), the members `type.name`
   * in that file's crate. Anchored on the type's own declaration, so a same-named type elsewhere never answers.
   */
  crateMembers(
    files: readonly string[],
    type: string,
    name: string,
    crateFiles: (file: string) => readonly string[]
  ) {
    const key = `${type}.${name}`;
    // The type is declared in one of the files, or one re-export hop from it (`pub use client::Client`).
    const declares = (list: readonly string[]) =>
      [...new Set(list)].filter(
        f => f.endsWith('.rs') && this.typesByFile.get(f)?.has(type)
      );
    const direct = declares(files);
    const owners = direct.length
      ? direct
      : declares(files.flatMap(f => this.imports.get(f) ?? []));
    return [...new Set(owners.flatMap(crateFiles))].flatMap(
      f => this.membersByFile.get(f)?.get(key) ?? []
    );
  }
  lookup(files: readonly string[], name: string) {
    return [...new Set(files)].flatMap(
      f => this.byFile.get(f)?.get(name) ?? []
    );
  }
  // `lookup`, and when none of the files declares the name, what they re-export: an exported declaration in a
  // file they import directly (`pub use edit::read`, `export { read } from './edit'`). One hop and exported only,
  // so a same-named private helper deeper in the imports never answers; two declarations stay ambiguous.
  lookupThrough(files: readonly string[], name: string) {
    const direct = this.lookup(files, name);
    if (direct.length) return direct;
    const reexported = [
      ...new Set(files.flatMap(f => this.imports.get(f) ?? [])),
    ];
    return this.lookup(reexported, name).filter(d => d.exported);
  }
}

// An imported name can only bind a top-level declaration, never a member such as `obj.read`, and only one that can
// be called: modules, types, traits, impls and the like never are (a `json!(…)` call once bound to `mod json;`).
const NOT_CALLABLE = new Set([
  'module',
  'namespace',
  'type',
  'interface',
  'trait',
  'impl',
  'enum',
  'label',
  'property',
]);
const topLevel = (d: Declaration) =>
  !NOT_CALLABLE.has(d.kind) &&
  !d.id.split('#')[1]?.replace(/@\d+$/, '').includes('.');
// The local name of a declaration (`Type.member` for a member), without its line suffix.
const localName = (d: Declaration) =>
  d.id.split('#')[1]?.replace(/@\d+$/, '') ?? '';
const member = (d: Declaration) =>
  !NOT_CALLABLE.has(d.kind) && localName(d).split('.').length === 2;
const memberKey = (d: Declaration) => localName(d);
// A declaration of a type: a struct, enum, trait, class, interface or alias, never a function or a member.
const declaresType = (d: Declaration) =>
  ![
    'function',
    'method',
    'constructor',
    'macro',
    'module',
    'label',
    'property',
    'impl',
  ].includes(d.kind) && !localName(d).includes('.');
// The name a call spells for a declaration. Macros and functions are separate namespaces, as the engine links them
// within a chunk: a Rust macro is reached only by its invocation `name!` (the engine spells an unbound one so),
// never by a call `name(..)`, and a function never by `name!`.
const spelling = (d: Declaration) =>
  d.kind === 'macro' ? `${d.name}!` : d.name;

export function* chunkDeclarations(
  chunks: readonly StitchChunk[],
  datas: ReadonlyMap<string, Pick<SectionData, 'declarations'>>
): Generator<Declaration> {
  for (const c of chunks)
    for (const d of datas.get(c.id)?.declarations ?? []) {
      yield {
        ...d,
        id: prefixOf(c.path) + d.id,
        file: prefixOf(c.path) + d.file,
      };
    }
}

// What each importing file reaches in other chunks: files of relative imports, and the imported workspace packages
// (package key -> chunk id) with every file of each such chunk.
function importTargets(
  cross: CrossImport[],
  declared: Map<string, Declaration[]>,
  chunks: readonly StitchChunk[],
  imports: ReadonlyMap<string, readonly string[]> = new Map()
) {
  const chunkFiles = Map.groupBy(declared.keys(), f => chunkOf(f, chunks)?.id);
  const direct = new Map<string, Reach>();
  for (const e of cross.filter((x): x is CrossImport & { source: string } =>
    Boolean(x.source)
  )) {
    if (!direct.has(e.source))
      direct.set(e.source, {
        files: [],
        through: [],
        packages: new Map(),
        chunkFiles,
      });
    const r = direct.get(e.source)!;
    if (e.via !== 'package') r.files.push(e.target);
    else r.packages.set(packageKey(e.package ?? e.target), e.targetChunk);
  }
  // A file also reaches what the modules of its own chunk that it imports reach in other chunks: a name re-exported
  // by `crate::config` (`pub use other_crate::read`) is called through that module.
  const reach = new Map<string, Reach>();
  for (const file of new Set([...direct.keys(), ...imports.keys()])) {
    const own = direct.get(file);
    const through = (imports.get(file) ?? []).flatMap(
      m => direct.get(m)?.files ?? []
    );
    if (!own && !through.length) continue;
    reach.set(file, {
      files: own?.files ?? [],
      through: [...new Set(through)],
      packages: own?.packages ?? new Map(),
      chunkFiles,
    });
  }
  return reach;
}

/** Every built chunk's rows with root-relative ids, before the edges between chunks join them. */
interface Merged {
  files: FileNode[];
  imports: ImportEdge[];
  symbols: SymbolNode[];
  calls: CallEdge[];
  fnFiles: string[];
  issues: Record<string, unknown>[];
  cycles: CycleRow[];
  externals: Map<string, ExternalPackage>;
  findings: Record<string, number>;
  callsOutside: number;
  uses: UseEdge[];
}

// Every built chunk's rows with root-relative ids.
function mergeChunks(
  built: readonly StitchChunk[],
  datas: ReadonlyMap<string, SectionData>
) {
  const m: Merged = {
    files: [],
    imports: [],
    symbols: [],
    calls: [],
    fnFiles: [],
    issues: [],
    cycles: [],
    externals: new Map(),
    findings: {},
    callsOutside: 0,
    uses: [],
  };
  // `built` holds only chunks with data.
  for (const c of built) addChunk(m, c, datas.get(c.id)!);
  return m;
}

function addChunk(m: Merged, c: StitchChunk, D: SectionData) {
  const p = <T>(id: T) =>
    typeof id === 'string' && !id.startsWith('pkg:')
      ? prefixOf(c.path) + id
      : id;
  m.files.push(
    ...(D.files ?? []).map(f => ({ ...f, id: p(f.id), chunk: c.id }))
  );
  m.imports.push(
    ...(D.imports ?? []).map(e => ({
      ...e,
      source: p(e.source),
      target: p(e.target),
    }))
  );
  m.symbols.push(
    ...(D.symbols ?? []).map(x => ({ ...x, id: p(x.id), file: p(x.file) }))
  );
  m.calls.push(
    ...(D.calls ?? []).map(e => ({
      ...e,
      source: p(e.source),
      target: p(e.target),
    }))
  );
  m.fnFiles.push(...(D.fnFiles ?? []).map(p));
  m.callsOutside += D.coverage?.callsOutside ?? 0;
  for (const x of D.externals ?? []) {
    const e = m.externals.get(x.id) ?? {
      id: x.id,
      count: 0,
      files: [],
      testFiles: [],
      kinds: {},
      lines: {},
    };
    e.count += x.count;
    e.files.push(...(x.files ?? []).map(p));
    e.testFiles?.push(...(x.testFiles ?? []).map(p));
    for (const [file, kinds] of Object.entries(x.kinds ?? {}))
      if (e.kinds) e.kinds[p(file)] = kinds;
    for (const [file, lines] of Object.entries(x.lines ?? {}))
      if (e.lines) e.lines[p(file)] = lines;
    if (x.loads) e.loads = { ...e.loads, ...x.loads };
    m.externals.set(x.id, e);
  }
  m.uses.push(
    ...(D.uses ?? []).map(u => ({
      ...u,
      source: p(u.source),
      target: p(u.target),
    }))
  );
  const insights: Partial<Insights> = D.insights ?? {};
  for (const [k, v] of Object.entries(insights.findings ?? {}))
    m.findings[k] = (m.findings[k] ?? 0) + v;
  m.issues.push(
    ...(insights.issues ?? []).map(x => ({
      ...x,
      subject: p(x.subject),
      chunk: c.id,
    }))
  );
  m.cycles.push(
    ...(insights.cycles?.all ?? []).map(x => ({
      ...x,
      nodes: (x.nodes ?? []).map(p),
      chunk: c.id,
    }))
  );
}

// Imports between chunks join the merged rows; an imported workspace package becomes a package node.
function addCrossImports(
  m: Merged,
  cross: CrossImport[],
  chunks: readonly StitchChunk[]
) {
  const byPair = new Map(
    m.imports.map(e => [JSON.stringify([e.source, e.target]), e])
  );
  const fileIds = new Set(m.files.map(f => f.id));
  for (const e of cross.filter(
    (x): x is CrossImport & { source: string } =>
      x.source !== null && fileIds.has(x.source)
  )) {
    if (!fileIds.has(e.target)) {
      if (e.via !== 'package') continue;
      // A package edge targets its owner chunk, whose package named the target.
      const owner = chunks.find(c => c.id === e.targetChunk)!;
      m.files.push({
        id: e.target,
        label: owner.package!,
        group: owner.path.split('/')[0],
        kind: 'package',
        chunk: owner.id,
        fanIn: 0,
        fanOut: 0,
      });
      fileIds.add(e.target);
    }
    const key = JSON.stringify([e.source, e.target]);
    const edge: ImportEdge = byPair.get(key) ?? {
      source: e.source,
      target: e.target,
      line: null,
      lines: [],
      count: 0,
      confidence: 'high',
      resolution: [],
      crossChunk: true,
      test: true,
    };
    edge.test = Boolean(edge.test && e.test);
    edge.count += 1;
    if (e.loads) edge.loads = { ...edge.loads, ...e.loads };
    for (const kind of [e.via, ...(e.kinds ?? [])]) {
      if (!edge.resolution.includes(kind)) edge.resolution.push(kind);
      if (kind !== e.via && isInteger(e.line)) {
        edge.kindLines ??= {};
        const at = (edge.kindLines[kind] ??= []);
        if (!at.includes(e.line)) at.push(e.line);
      }
    }
    if (isInteger(e.line) && !edge.lines.includes(e.line)) {
      edge.lines.push(e.line);
      edge.line = Math.min(edge.line ?? e.line, e.line);
    }
    byPair.set(key, edge);
  }
  m.imports = [...byPair.values()];
  return fileIds;
}

// Calls bound across chunks join the graph with their declarations as function nodes.
function addCrossCalls(
  m: Merged,
  {
    crossCalls,
    fileIds,
    built,
    datas,
  }: {
    crossCalls: CallEdge[];
    fileIds: Set<string>;
    built: readonly StitchChunk[];
    datas: ReadonlyMap<string, SectionData>;
  }
) {
  const symbols = new Map(m.symbols.map(x => [x.id, x]));
  const declared = new Map(
    Array.from(chunkDeclarations(built, datas), d => [d.id, d])
  );
  for (const e of crossCalls.filter(
    x => fileIds.has(fileOf(x.source)) && fileIds.has(fileOf(x.target))
  )) {
    m.calls.push(e);
    for (const id of [e.source, e.target].filter(
      x => x.includes('#') && !symbols.has(x)
    )) {
      const row = {
        ...(declared.get(id) ?? {
          id,
          file: fileOf(id),
          name: id.split('#')[1],
          kind: 'symbol',
          exported: false,
        }),
        calls: 0,
      };
      symbols.set(id, row);
      m.symbols.push(row);
    }
    const caller = symbols.get(e.source);
    if (caller) caller.calls++;
    m.fnFiles.push(fileOf(e.source), fileOf(e.target));
  }
}

// Import degrees per node, and import counts between top-level groups.
function shape(m: Merged) {
  const { fanIn, fanOut } = degrees(m.imports);
  for (const f of m.files)
    Object.assign(f, {
      fanIn: fanIn.get(f.id) ?? 0,
      fanOut: fanOut.get(f.id) ?? 0,
      group: f.kind === 'package' ? f.group : topGroup(f.id),
    });
  const groupOf = new Map(m.files.map(f => [f.id, f.group]));
  const crossings = new Map<string, number>();
  for (const e of m.imports) {
    const k = `${groupOf.get(e.source) ?? topGroup(e.source)}>${groupOf.get(e.target) ?? topGroup(e.target)}`;
    if (k.split('>')[0] !== k.split('>')[1])
      crossings.set(k, (crossings.get(k) ?? 0) + 1);
  }
  return [...crossings].map(([k, count]) => ({
    from: k.split('>')[0],
    to: k.split('>')[1],
    count,
  }));
}

// Verify production hubs and files heavily used across chunk boundaries, including by tests.
// Keeping boundary witnesses in probe selection makes confirmation independent of production reporting.
function probeHubs(files: FileNode[], imports: ImportEdge[]) {
  const budget = 8;
  const incoming = new Map<string, number>();
  for (const e of imports)
    if (!e.test || e.crossChunk)
      incoming.set(e.target, (incoming.get(e.target) ?? 0) + 1);
  // Only files with incoming imports are ranked.
  const boundaries = files
    .filter(f => incoming.has(f.id))
    .toSorted((a, b) => incoming.get(b.id)! - incoming.get(a.id)!);
  return [
    ...new Set(
      [...files.slice(0, budget), ...boundaries.slice(0, budget)].map(f => f.id)
    ),
  ];
}

// One section-shaped data.json over every built chunk, ids relative to the root.
function whole({
  root,
  chunks,
  datas,
  cross,
  crossCalls,
}: {
  root: string;
  chunks: readonly StitchChunk[];
  datas: ReadonlyMap<string, SectionData>;
  cross: CrossImport[];
  crossCalls: CallEdge[];
}): GraphData {
  const built = chunks.filter(c => datas.has(c.id));
  const m = mergeChunks(built, datas);
  addCrossCalls(m, {
    crossCalls,
    fileIds: addCrossImports(m, cross, chunks),
    built,
    datas,
  });
  const groups = shape(m);
  const sorted = m.cycles.toSorted((a, b) => b.size - a.size);
  const runtime = sorted.filter(x => x.runtime);
  const fnSet = [...new Set(m.fnFiles)];
  const sources = m.files
    .filter(f => f.kind !== 'package')
    .toSorted((a, b) => b.fanIn - a.fanIn);
  const crossCount = m.imports.filter(e => e.crossChunk).length;
  const { partial, partialChunks, notBuilt } = chunkCoverage(chunks, datas);
  return {
    section: root,
    mode: 'whole',
    frozen: false,
    evidence: 'chunks/<id>/evidence',
    coverage: {
      ...partial,
      partialChunks,
      chunks: chunks.length,
      built: built.length,
      notBuilt,
      files: sources.length,
      imports: m.imports.length,
      crossChunkImports: crossCount,
      calls: m.calls.length,
      crossChunkCalls: m.calls.filter(c => c.crossChunk).length,
      callsOutside: m.callsOutside,
      note: 'Stitched from built chunk snapshots. Imports between chunks come from out-of-root relative imports, Rust paths into other workspace crates (crate-path) and workspace package imports. Calls between chunks bind when exactly one declaration matches where the caller reaches (resolution stitched-import plus its rule): crate-path (a workspace crate path names the module file and item; high), import (a name or Type::member declared in a file it imports from another chunk, or one re-export hop away; medium), own-module (an imported name reached through a module of its own chunk; medium), package (a name from a workspace package import; medium). Calls on a receiver, names the caller declares itself and ambiguous names stay unbound in the evidence of each chunk. Cycles and findings are per chunk.',
    },
    snapshot: {
      ...partial,
      id: 'whole',
      root,
      counts: {
        nodes: m.files.length + m.symbols.length,
        edges: m.imports.length + m.calls.length,
        nodesByKind: { file: sources.length },
      },
    },
    files: m.files,
    imports: m.imports,
    symbols: m.symbols,
    calls: m.calls,
    uses: m.uses,
    fnFiles: fnSet,
    legends: {
      files: [
        `Files: ${m.files.length}`,
        `Imports: ${m.imports.length} (${crossCount} between chunks)`,
      ],
      functions: [
        `Files: ${fnSet.length}`,
        `Functions: ${m.symbols.length}`,
        `Calls: ${m.calls.length}`,
      ],
    },
    insights: {
      hubs: sources
        .slice(0, 6)
        .map(({ id, fanIn, fanOut }) => ({ id, fanIn, fanOut })),
      health: {},
      findings: m.findings,
      issues: m.issues,
      cycles: {
        all: sorted,
        total: sorted.length,
        runtime: runtime.length,
        largestRuntime: Math.max(0, ...runtime.map(x => x.size)),
        top: sorted
          .slice(0, 5)
          .map(x => ({ size: x.size, runtime: x.runtime, nodes: x.nodes })),
      },
      groups,
      mutualGroups: groups
        .filter(
          g =>
            g.from < g.to &&
            groups.some(d => d.from === g.to && d.to === g.from)
        )
        .map(g => [g.from, g.to]),
    },
    hubFiles: probeHubs(sources, m.imports),
    externals: [...m.externals.values()]
      .filter(
        x =>
          !chunks.some(
            c => c.package && packageKey(c.package) === packageKey(x.id)
          )
      )
      .toSorted((a, b) => b.count - a.count),
    callStats: { mode: 'graph', lsp: 'off' },
    tokens: { sourceBytes: 0, sourceFiles: m.files.length },
  };
}
