// Chunk membership and stitching: one root-relative graph from separately built chunk snapshots.
import { existsSync } from 'node:fs';
import { dirname, join, posix, relative, sep } from 'node:path';
import { degrees, topGroup } from './lib.mjs';

const EXT = ['.ts', '.tsx', '.mts', '.cts', '.d.ts', '.js', '.jsx', '.mjs', '.cjs', '.json', '.py', '.rs', '.go', '.java', '.scala', '.cs', '.c', '.h', '.cc', '.cpp', '.hpp', '.cu', '.cuh'];
const JS_TO_TS = { '.js': ['.ts', '.tsx'], '.mjs': ['.mts'], '.cjs': ['.cts'], '.jsx': ['.tsx'] };

const prefixOf = (c) => (c.path === '.' ? '' : `${c.path}/`);

// The chunk that ingests `path` (root-relative): the deepest chunk containing it, unless an
// excluded directory name prunes it inside that chunk (ingest excludes names, not paths).
export function chunkOf(path, chunks) {
  let best = null;
  for (const c of chunks) if ((c.path === '.' || path === c.path || path.startsWith(prefixOf(c))) && (!best || c.path.length > best.path.length || best.path === '.')) best = c;
  if (!best) return null;
  const inner = path.slice(prefixOf(best).length).split('/').slice(0, -1);
  return best.exclude?.some((name) => inner.includes(name)) ? null : best;
}

// A relative import specifier to a root-relative file that `has` knows, by the usual module suffixes;
// Rust `crate::`, `super::` and `self::` paths by the module tree.
export function resolveImport(from, specifier, has, cratePrefix = null) {
  if (/^(crate|super|self)::/.test(specifier ?? '')) return resolveRustPath(from, specifier, has, cratePrefix);
  if (!specifier || !/^\.\.?(\/|$)/.test(specifier)) return null;
  const target = posix.normalize(posix.join(posix.dirname(from), specifier));
  if (target.startsWith('../')) return null;
  const ext = posix.extname(target);
  const stem = target.slice(0, target.length - ext.length);
  const tries = [target, ...(JS_TO_TS[ext] ?? []).map((e) => stem + e), ...EXT.map((e) => target + e), ...EXT.map((e) => `${target}/index${e}`)];
  return tries.find(has) ?? null;
}

// A Rust module's directory: `a/b.rs` owns `a/b/`; `mod.rs`, `lib.rs` and `main.rs` own their directory.
const moduleDir = (file) => (/(^|\/)(mod|lib|main)\.rs$/.test(file) ? posix.dirname(file) : file.replace(/\.rs$/, ''));
// The file that declares the module living in `dir`: `dir.rs`, `dir/mod.rs`, or the crate root in `src`.
const moduleFile = (dir, has) => (dir === '.' ? ['mod.rs', 'lib.rs', 'main.rs'] : [`${dir}.rs`, `${dir}/mod.rs`, `${dir}/lib.rs`, `${dir}/main.rs`]).find(has) ?? null;
const childDir = (dir, seg) => (dir === '.' ? seg : `${dir}/${seg}`);

// A root inside a crate (`crates/x/src/tools`): the module path from the crate's `src` to the root
// (`['tools']`), so `crate::tools::a` names the root's `a`. Null when no Cargo.toml is above the root.
export function crateModulePrefix(root) {
  for (let dir = root; dir !== dirname(dir); dir = dirname(dir)) {
    if (existsSync(join(dir, 'Cargo.toml'))) {
      const rel = relative(join(dir, 'src'), root);
      return rel.startsWith('..') ? null : rel.split(sep).filter(Boolean);
    }
  }
  return null;
}

// Where a Rust path starts: the crate's `src` for `crate::`, else this module walked up by `super`.
function rustAnchor(from, parts, has, cratePrefix) {
  if (parts[0] !== 'crate') {
    let dir = moduleDir(from), i = 0;
    for (; parts[i] === 'self' || parts[i] === 'super'; i++) if (parts[i] === 'super') dir = posix.dirname(dir);
    return { dir, rest: parts.slice(i) };
  }
  for (let d = posix.dirname(from); ; d = posix.dirname(d)) {
    if (has(d === '.' ? 'Cargo.toml' : `${d}/Cargo.toml`)) return { dir: d === '.' ? 'src' : `${d}/src`, rest: parts.slice(1) };
    if (d === '.' || d === '/') break;
  }
  // The crate root is above the stitched root: strip the root's own module path.
  const rest = parts.slice(1);
  return cratePrefix && cratePrefix.every((seg, i) => rest[i] === seg) ? { dir: '.', rest: rest.slice(cratePrefix.length) } : null;
}

// The deepest module file a `crate::`/`super::`/`self::` path names; trailing segments are items in it.
export function resolveRustPath(from, specifier, has, cratePrefix = null) {
  const anchor = rustAnchor(from, specifier.split('::'), has, cratePrefix);
  if (!anchor) return null;
  let { dir } = anchor;
  let target = moduleFile(dir, has);
  for (const seg of anchor.rest) {
    const file = moduleFile(childDir(dir, seg), has);
    if (!file || /(^|\/)(lib|main)\.rs$/.test(file)) break;
    [dir, target] = [childDir(dir, seg), file];
  }
  return target && target !== from ? target : null;
}

// A Rust path naming a module above the stitched root: a `crate::` path outside the root's module
// prefix, or a `super::` chain that climbs past it.
function leavesRoot(from, specifier, cratePrefix) {
  const parts = String(specifier).split('::');
  if (parts[0] === 'crate') return Boolean(cratePrefix) && !cratePrefix.every((seg, i) => parts[i + 1] === seg);
  const climbs = parts.findIndex((p) => p !== 'super');
  return parts[0] === 'super' && climbs > moduleDir(from).split('/').filter((p) => p && p !== '.').length;
}

// Workspace package names as the graph prints them: `pkg:@scope/name/sub` and Cargo `a-b` vs `a_b`.
export const packageKey = (id) => {
  const name = String(id).replace(/^pkg:/, '');
  const parts = name.split('/');
  return (name.startsWith('@') ? parts.slice(0, 2) : parts.slice(0, 1)).join('/').toLowerCase().replaceAll('-', '_');
};

// chunks: plan rows ({id, path, package, exclude}); datas: Map chunk id -> its data.json; files: every root-relative file.
export function stitch({ root, chunks, datas, files }) {
  const ctx = { chunks, datas, has: new Set(files), cratePrefix: crateModulePrefix(root), byPackage: new Map(chunks.filter((c) => c.package).map((c) => [packageKey(c.package), c])) };
  const cross = [];
  const unresolved = [];
  for (const c of chunks.filter((x) => datas.has(x.id))) {
    for (const o of datas.get(c.id).outsideRoot ?? []) stitchRelative(ctx, c, o, { cross, unresolved });
    for (const x of datas.get(c.id).externals ?? []) stitchPackage(ctx, c, x, cross);
  }
  const crossCalls = bindCalls({ chunks, datas, cross });
  return { cross, unresolved, crossCalls, whole: whole({ root, chunks, datas, cross, crossCalls }) };
}

// One out-of-root import of chunk `c`: an edge to the chunk that holds its target, or an unresolved row with the reason.
function stitchRelative({ chunks, datas, has, cratePrefix }, c, o, { cross, unresolved }) {
  const source = prefixOf(c) + o.file;
  const target = resolveImport(source, o.specifier, (p) => has.has(p), cratePrefix);
  const owner = target && chunkOf(target, chunks);
  const gap = (reason) => unresolved.push({ source, line: o.line, specifier: o.specifier, reason });
  // Inside its own chunk the engine already decided; an import it left unlinked stays unlinked.
  if (owner === c) gap('unlinked inside its own chunk');
  else if (owner) cross.push({ source, target, line: o.line, via: 'relative', sourceChunk: c.id, targetChunk: owner.id, targetBuilt: datas.has(owner.id) });
  else if (target) gap('target file is in no chunk');
  else gap(leavesRoot(source, o.specifier, cratePrefix) ? 'outside the stitched root' : 'no file matches the specifier');
}

// A package import of chunk `c` that names another chunk's workspace package: one edge per importing file.
function stitchPackage({ datas, byPackage }, c, x, cross) {
  const owner = byPackage.get(packageKey(x.id));
  if (!owner || owner === c) return;
  const row = { target: `pkg:${owner.package}`, package: x.id, via: 'package', sourceChunk: c.id, targetChunk: owner.id, targetBuilt: datas.has(owner.id) };
  // Data from older builds counts importers without naming them: keep the chunk-level weight.
  if (!x.files) cross.push({ ...row, source: null, count: x.count });
  for (const file of x.files ?? []) cross.push({ ...row, source: prefixOf(c) + file });
}

// A call site a chunk left unbound (`not-declared-in-repo`) binds through its file's imports of other
// chunks when exactly one imported file declares that exact top-level name (`medium`, `stitched-import`).
// Everything else stays unbound and remains in the chunk's evidence: a repository-wide unique-name
// fallback measured mostly wrong on Rust, and a last-segment match bound unrelated members.
export function bindCalls({ chunks, datas, cross }) {
  const declared = declarationsByFile(chunks, datas);
  const reach = importTargets(cross, declared, chunks);
  const index = new NameIndex(declared);
  const by = new Map();
  for (const c of chunks) for (const u of datas.get(c.id)?.unresolvedCalls ?? []) {
    const exact = candidates(u, reach.get(prefixOf(c) + u.file), index);
    if (exact.length !== 1) continue;
    const source = prefixOf(c) + u.from, k = `${source}>${exact[0].id}`;
    const e = by.get(k) ?? { source, target: exact[0].id, line: u.line, count: 0, confidence: 'medium', resolution: ['stitched-import'], via: 'graph', crossChunk: true };
    e.count++;
    by.set(k, e);
  }
  return [...by.values()];
}

// The declarations an unbound call can name, by the engine's own reason for leaving it unbound:
// - `external-package`: the name came through a `use`/import of a package; it may bind into the imported workspace
//   packages' chunks, and a qualified call (`crate_x::path::f`) only into the package its first segment names.
// - `not-declared-in-repo`: nothing imported it (macros such as `format!`, locals, prelude names); it may bind only
//   through a relative import that resolved to a file in another chunk.
function candidates(u, reach, index) {
  if (!reach) return [];
  const parts = String(u.targetName).split('::');
  const name = parts.at(-1);
  if (name.includes('.')) return [];
  if (u.reason === 'not-declared-in-repo') return parts.length === 1 ? index.lookup(reach.files, name) : [];
  if (u.reason !== 'external-package') return [];
  const chunkIds = parts.length > 1 ? [reach.packages.get(packageKey(parts[0]))].filter(Boolean) : [...reach.packages.values()];
  return index.lookup(chunkIds.flatMap((id) => reach.chunkFiles.get(id) ?? []), name);
}

// Top-level callable declarations by file, looked up by name over a set of files (one Map per file, built once).
class NameIndex {
  constructor(declared) {
    this.byFile = new Map([...declared].map(([file, list]) => [file, Map.groupBy(list.filter(topLevel), (d) => d.name)]));
  }
  lookup(files, name) {
    return [...new Set(files)].flatMap((f) => this.byFile.get(f)?.get(name) ?? []);
  }
}

// An imported name can only bind a top-level declaration, never a member such as `obj.read`, and only one that can
// be called: modules, types, traits, impls and the like never are (a `json!(…)` call once bound to `mod json;`).
const NOT_CALLABLE = new Set(['module', 'namespace', 'type', 'interface', 'trait', 'impl', 'enum', 'label', 'property']);
const topLevel = (d) => !NOT_CALLABLE.has(d.kind) && !d.id.split('#')[1]?.replace(/@\d+$/, '').includes('.');

function declarationsByFile(chunks, datas) {
  const declared = new Map();
  for (const c of chunks) for (const d of datas.get(c.id)?.declarations ?? []) {
    const file = prefixOf(c) + d.file;
    if (!declared.has(file)) declared.set(file, []);
    declared.get(file).push({ ...d, id: prefixOf(c) + d.id, file });
  }
  return declared;
}

// What each importing file reaches in other chunks: files of relative imports, and the imported workspace packages
// (package key -> chunk id) with every file of each such chunk.
function importTargets(cross, declared, chunks) {
  const chunkFiles = Map.groupBy(declared.keys(), (f) => chunkOf(f, chunks)?.id);
  const reach = new Map();
  for (const e of cross.filter((x) => x.source)) {
    if (!reach.has(e.source)) reach.set(e.source, { files: [], packages: new Map(), chunkFiles });
    const r = reach.get(e.source);
    if (e.via === 'relative') r.files.push(e.target);
    else r.packages.set(packageKey(e.package ?? e.target), e.targetChunk);
  }
  return reach;
}

// Every built chunk's rows with root-relative ids.
function mergeChunks(built, datas) {
  const m = { files: [], imports: [], symbols: [], calls: [], fnFiles: [], issues: [], cycles: [], externals: new Map(), findings: {}, callsOutside: 0 };
  for (const c of built) addChunk(m, c, datas.get(c.id));
  return m;
}

function addChunk(m, c, D) {
  const p = (id) => (typeof id === 'string' && !id.startsWith('pkg:') ? prefixOf(c) + id : id);
  const rows = (k) => D[k] ?? [];
  m.files.push(...rows('files').map((f) => ({ ...f, id: p(f.id), chunk: c.id })));
  m.imports.push(...rows('imports').map((e) => ({ ...e, source: p(e.source), target: p(e.target) })));
  m.symbols.push(...rows('symbols').map((x) => ({ ...x, id: p(x.id), file: p(x.file) })));
  m.calls.push(...rows('calls').map((e) => ({ ...e, source: p(e.source), target: p(e.target) })));
  m.fnFiles.push(...rows('fnFiles').map(p));
  m.callsOutside += D.coverage?.callsOutside ?? 0;
  for (const x of rows('externals')) {
    const e = m.externals.get(x.id) ?? { id: x.id, count: 0, files: [] };
    e.count += x.count;
    e.files.push(...(x.files ?? []).map(p));
    m.externals.set(x.id, e);
  }
  const insights = D.insights ?? {};
  for (const [k, v] of Object.entries(insights.findings ?? {})) m.findings[k] = (m.findings[k] ?? 0) + v;
  m.issues.push(...(insights.issues ?? []).map((x) => ({ ...x, subject: p(x.subject), chunk: c.id })));
  m.cycles.push(...(insights.cycles?.all ?? []).map((x) => ({ ...x, nodes: (x.nodes ?? []).map(p), chunk: c.id })));
}

// Imports between chunks join the merged rows; an imported workspace package becomes a package node.
function addCrossImports(m, cross, chunks) {
  const fileIds = new Set(m.files.map((f) => f.id));
  for (const e of cross.filter((x) => fileIds.has(x.source))) {
    if (!fileIds.has(e.target)) {
      if (e.via !== 'package') continue;
      const owner = chunks.find((c) => c.id === e.targetChunk);
      m.files.push({ id: e.target, label: owner.package, group: owner.path.split('/')[0], kind: 'package', chunk: owner.id, fanIn: 0, fanOut: 0 });
      fileIds.add(e.target);
    }
    m.imports.push({ source: e.source, target: e.target, line: e.line ?? null, count: 1, confidence: 'high', resolution: [e.via], crossChunk: true });
  }
  m.imports = [...new Map(m.imports.map((e) => [`${e.source}>${e.target}`, e])).values()];
  return fileIds;
}

const fileOf = (id) => id.split('#')[0];

// Calls bound across chunks join the graph with their declarations as function nodes.
function addCrossCalls(m, { crossCalls, fileIds, built, datas }) {
  const symbols = new Map(m.symbols.map((x) => [x.id, x]));
  const declared = new Map(built.flatMap((c) => (datas.get(c.id).declarations ?? []).map((d) => [prefixOf(c) + d.id, { ...d, id: prefixOf(c) + d.id, file: prefixOf(c) + d.file }])));
  for (const e of crossCalls.filter((x) => fileIds.has(fileOf(x.source)) && fileIds.has(fileOf(x.target)))) {
    m.calls.push(e);
    for (const id of [e.source, e.target].filter((x) => x.includes('#') && !symbols.has(x))) {
      const row = { ...(declared.get(id) ?? { id, file: fileOf(id), name: id.split('#')[1], kind: 'symbol', exported: false }), calls: 0 };
      symbols.set(id, row);
      m.symbols.push(row);
    }
    if (symbols.has(e.source)) symbols.get(e.source).calls++;
    m.fnFiles.push(fileOf(e.source), fileOf(e.target));
  }
}

// Import degrees per node, and import counts between top-level groups.
function shape(m) {
  const { fanIn, fanOut } = degrees(m.imports);
  for (const f of m.files) Object.assign(f, { fanIn: fanIn.get(f.id) ?? 0, fanOut: fanOut.get(f.id) ?? 0, group: f.kind === 'package' ? f.group : topGroup(f.id) });
  const groupOf = new Map(m.files.map((f) => [f.id, f.group]));
  const crossings = new Map();
  for (const e of m.imports) {
    const k = `${groupOf.get(e.source) ?? topGroup(e.source)}>${groupOf.get(e.target) ?? topGroup(e.target)}`;
    if (k.split('>')[0] !== k.split('>')[1]) crossings.set(k, (crossings.get(k) ?? 0) + 1);
  }
  return [...crossings].map(([k, count]) => ({ from: k.split('>')[0], to: k.split('>')[1], count }));
}

// One section-shaped data.json over every built chunk, ids relative to the root.
function whole({ root, chunks, datas, cross, crossCalls }) {
  const built = chunks.filter((c) => datas.has(c.id));
  const m = mergeChunks(built, datas);
  addCrossCalls(m, { crossCalls, fileIds: addCrossImports(m, cross, chunks), built, datas });
  const groups = shape(m);
  const sorted = m.cycles.toSorted((a, b) => b.size - a.size);
  const runtime = sorted.filter((x) => x.runtime);
  const fnSet = [...new Set(m.fnFiles)];
  const sources = m.files.filter((f) => f.kind !== 'package').toSorted((a, b) => b.fanIn - a.fanIn);
  const crossCount = m.imports.filter((e) => e.crossChunk).length;
  return {
    section: root,
    mode: 'whole',
    frozen: false,
    evidence: 'chunks/<id>/tmp/evidence',
    coverage: { chunks: chunks.length, built: built.length, notBuilt: chunks.filter((c) => !datas.has(c.id)).map((c) => c.id), files: sources.length, imports: m.imports.length, crossChunkImports: crossCount, calls: m.calls.length, crossChunkCalls: m.calls.filter((c) => c.crossChunk).length, callsOutside: m.callsOutside, note: 'Stitched from built chunk snapshots. Imports between chunks come from out-of-root relative imports and workspace package imports. Calls between chunks are bound by name through those imports (stitched-import, medium: the exact top-level name declared in one imported file); member, qualified, dynamic-import and ambiguous calls stay unbound in the evidence of each chunk. Cycles and findings are per chunk.' },
    snapshot: { id: 'whole', root, counts: { nodes: m.files.length + m.symbols.length, edges: m.imports.length + m.calls.length, nodesByKind: { file: sources.length } } },
    files: m.files,
    imports: m.imports,
    symbols: m.symbols,
    calls: m.calls,
    fnFiles: fnSet,
    legends: { files: [`Files: ${m.files.length}`, `Imports: ${m.imports.length} (${crossCount} between chunks)`], functions: [`Files: ${fnSet.length}`, `Functions: ${m.symbols.length}`, `Calls: ${m.calls.length}`] },
    insights: {
      hubs: sources.slice(0, 6).map(({ id, fanIn, fanOut }) => ({ id, fanIn, fanOut })),
      health: {},
      findings: m.findings,
      issues: m.issues,
      cycles: { all: sorted, total: sorted.length, runtime: runtime.length, largestRuntime: Math.max(0, ...runtime.map((x) => x.size)), top: sorted.slice(0, 5).map((x) => ({ size: x.size, runtime: x.runtime, nodes: x.nodes })) },
      groups,
      mutualGroups: groups.filter((g) => g.from < g.to && groups.some((d) => d.from === g.to && d.to === g.from)).map((g) => [g.from, g.to]),
    },
    hubFiles: sources.slice(0, 8).map((f) => f.id),
    externals: [...m.externals.values()].filter((x) => !chunks.some((c) => c.package && packageKey(c.package) === packageKey(x.id))).toSorted((a, b) => b.count - a.count),
    callStats: { mode: 'graph', lsp: 'off' },
    tokens: { sourceBytes: 0, sourceFiles: m.files.length },
  };
}
