// Chunk plan over root-relative file paths. Boundaries come from the repository itself: a directory with
// a package.json or Cargo.toml is a package whose imports and calls stitch.mjs can rejoin by package name
// and module path. A chunk the graph refuses as over its snapshot budget (scopeTooBroad) is refined again.
import { basename, posix } from 'node:path';

const MANIFESTS = new Set(['package.json', 'Cargo.toml']);
// Refinement on scopeTooBroad keeps directories under this many files with their parent's loose files.
const MIN_CHUNK = 10;

export function tree(files) {
  const size = new Map();
  const kids = new Map();
  for (const p of files) {
    const parts = p.split('/');
    for (let i = 1; i < parts.length; i++) {
      const dir = parts.slice(0, i).join('/');
      size.set(dir, (size.get(dir) ?? 0) + 1);
      const parent = i === 1 ? '' : parts.slice(0, i - 1).join('/');
      if (!kids.has(parent)) kids.set(parent, new Set());
      kids.get(parent).add(dir);
    }
  }
  const packages = new Set(files.filter((p) => MANIFESTS.has(basename(p)) && p.includes('/')).map((p) => posix.dirname(p)));
  return { files, size, kids, packages, count: (dir) => (dir ? size.get(dir) ?? 0 : files.length), children: (dir) => [...(kids.get(dir) ?? [])] };
}

const containsPackage = (t, dir) => t.packages.has(dir) || [...t.packages].some((p) => p.startsWith(`${dir}/`));

// Chunks under `dir`: children chosen by `split` get their own plan; the rest stay with the files directly
// in `dir` as its loose chunk, which excludes the split children by name (ingest prunes names, not paths).
function planDir(t, dir, split) {
  const own = t.children(dir).filter((k) => split(k));
  if (!own.length) return t.count(dir) ? [{ path: dir || '.', files: t.count(dir) }] : [];
  const loose = t.count(dir) - own.reduce((a, k) => a + t.count(k), 0);
  const head = loose > 0 ? [{ path: dir || '.', files: loose, loose: true, exclude: own.map((k) => basename(k)) }] : [];
  return [...head, ...own.flatMap((k) => planDir(t, k, split))];
}

// The whole root as one chunk (`whole`), or one chunk per package and per loose remainder.
export function planChunks(files, { whole = false } = {}) {
  const t = tree(files);
  const chunks = whole ? [{ path: '.', files: files.length }] : planDir(t, '', (k) => containsPackage(t, k));
  return name(chunks.toSorted((a, b) => b.files - a.files));
}

// A chunk the graph refused, split one level: a directory into its children of a useful size and its own
// files; a loose chunk into its remaining children and the files directly in it. null: nothing left to split.
export function refine(c, files) {
  const t = tree(files);
  const dir = c.path === '.' ? '' : c.path;
  if (!c.loose) {
    const parts = planDir(t, dir, (k) => t.count(k) >= MIN_CHUNK);
    return parts.length > 1 ? parts : null;
  }
  const all = t.children(dir);
  const kids = all.filter((k) => !c.exclude.includes(basename(k)));
  if (!kids.length) return null;
  const direct = t.count(dir) - all.reduce((a, k) => a + t.count(k), 0);
  return [...(direct > 0 ? [{ path: c.path, files: direct, loose: true, exclude: all.map((k) => basename(k)) }] : []), ...kids.map((k) => ({ path: k, files: t.count(k) }))];
}

const slug = (p) => p.replace(/[^A-Za-z0-9]+/g, '-').replace(/^-|-$/g, '') || 'root';
// Stable unique ids: the path slug, `-loose` for a remainder chunk, a counter on collision.
export function name(chunks, used = new Set()) {
  for (const c of chunks) {
    const base = slug(c.path) + (c.loose ? '-loose' : '');
    let id = base;
    for (let n = 2; used.has(id); n++) id = `${base}-${n}`;
    used.add(id);
    c.id = id;
  }
  return chunks;
}
