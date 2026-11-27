// Chunk plan over root-relative file paths. Boundaries come from the repository itself: a directory with
// a package.json or Cargo.toml is a package whose imports and calls stitch.ts can rejoin by package name
// and module path. A chunk the graph refuses as over its snapshot budget (scopeTooBroad) is refined again.
import { basename, posix } from 'node:path';
import type { Chunk } from './types.ts';

/** A planned chunk before `name` gives it its id. */
export type ChunkPlan = Omit<Chunk, 'id'> & { id?: string };

const MANIFESTS = new Set(['package.json', 'Cargo.toml']);
export function tree(files: string[], extensions?: ReadonlySet<string>) {
  const manifests = files.filter(
    p => MANIFESTS.has(basename(p)) && p.includes('/')
  );
  if (extensions)
    files = files.filter(p =>
      extensions.has(posix.extname(p).slice(1).toLowerCase())
    );
  const size = new Map<string, number>();
  const kids = new Map<string, Set<string>>();
  for (const p of files) {
    const parts = p.split('/');
    for (let i = 1; i < parts.length; i++) {
      const dir = parts.slice(0, i).join('/');
      size.set(dir, (size.get(dir) ?? 0) + 1);
      const parent = i === 1 ? '' : parts.slice(0, i - 1).join('/');
      if (!kids.has(parent)) kids.set(parent, new Set());
      kids.get(parent)!.add(dir);
    }
  }
  const packages = new Set(
    manifests.map(p => posix.dirname(p)).filter(dir => size.has(dir))
  );
  return {
    files,
    size,
    kids,
    packages,
    count: (dir: string) => (dir ? (size.get(dir) ?? 0) : files.length),
    children: (dir: string) => [...(kids.get(dir) ?? [])],
  };
}

type FileTree = ReturnType<typeof tree>;

const containsPackage = (t: FileTree, dir: string) =>
  t.packages.has(dir) || [...t.packages].some(p => p.startsWith(`${dir}/`));

// Chunks under `dir`: children chosen by `split` get their own plan; the rest stay with the files directly
// in `dir` as its loose chunk, which excludes the split children by name (ingest prunes names, not paths).
function planDir(
  t: FileTree,
  dir: string,
  split: (dir: string) => boolean
): ChunkPlan[] {
  const own = t.children(dir).filter(k => split(k));
  if (!own.length)
    return t.count(dir) ? [{ path: dir || '.', files: t.count(dir) }] : [];
  const loose = t.count(dir) - own.reduce((a, k) => a + t.count(k), 0);
  const head =
    loose > 0
      ? [
          {
            path: dir || '.',
            files: loose,
            loose: true,
            exclude: own.map(k => basename(k)),
          },
        ]
      : [];
  return [...head, ...own.flatMap(k => planDir(t, k, split))];
}

// The whole root as one chunk (`whole`), or one chunk per package and per loose remainder.
export function planChunks(
  files: string[],
  {
    whole = false,
    extensions,
  }: { whole?: boolean; extensions?: ReadonlySet<string> } = {}
) {
  const t = tree(files, extensions);
  if (!t.files.length) return [];
  const chunks = whole
    ? [{ path: '.', files: t.files.length }]
    : planDir(t, '', k => containsPackage(t, k));
  return name(chunks.toSorted((a, b) => b.files - a.files));
}

// Split a refused scope at its first real branch. The largest children build separately;
// smaller siblings stay together until the engine itself refuses that remainder.
export function refine(
  c: Pick<Chunk, 'path' | 'exclude'>,
  files: string[],
  extensions?: ReadonlySet<string>
): ChunkPlan[] | null {
  let dir = c.path === '.' ? '' : c.path;
  const prefix = dir ? `${dir}/` : '';
  const exclude = c.exclude ?? [];
  const inside = files.filter(
    p =>
      p.startsWith(prefix) &&
      !p
        .slice(prefix.length)
        .split('/')
        .slice(0, -1)
        .some(part => exclude.includes(part))
  );
  const t = tree(inside, extensions);
  for (;;) {
    const kids = t.children(dir);
    if (!kids.length) return null;
    const largest = Math.max(...kids.map(t.count));
    if (kids.length === 1 && largest === t.count(dir)) {
      [dir] = kids;
      continue;
    }
    const split = kids.filter(k => t.count(k) === largest);
    const remaining = t.count(dir) - split.length * largest;
    const chunks: ChunkPlan[] = split.map(path => ({
      path,
      files: largest,
      ...(exclude.length && { exclude }),
    }));
    if (remaining)
      chunks.unshift({
        path: dir || '.',
        files: remaining,
        loose: true,
        exclude: [...exclude, ...split.map(k => basename(k))],
      });
    return chunks;
  }
}

const slug = (p: string) =>
  p.replace(/[^A-Za-z0-9]+/g, '-').replace(/^-|-$/g, '') || 'root';
// Stable unique ids: the path slug, `-loose` for a remainder chunk, a counter on collision.
export function name<C extends ChunkPlan>(
  chunks: C[],
  used = new Set<string>()
) {
  for (const c of chunks) {
    const base = slug(c.path) + (c.loose ? '-loose' : '');
    let id = base;
    for (let n = 2; used.has(id); n++) id = `${base}-${n}`;
    used.add(id);
    c.id = id;
  }
  // Every chunk now carries its id.
  return chunks as (C & Pick<Chunk, 'id'>)[];
}
