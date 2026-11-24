// Package bytes at build and refresh time, written into data.json's externals (package-bytes.ts reads them):
//   bundle             a bundler file found under the root's or a chunk's output directories (an esbuild metafile,
//                      webpack stats, a rollup or rolldown output dump or analyze-data.json), read by the bundle
//                      ingest (bundle-ingest.ts): the bytes of the package's inputs in the bundle.
//   installed-closure  otherwise, for a JavaScript or TypeScript importer: the package's entry as Node resolves it from
//                      the importing file (node_modules lookup, `exports` with the node, import or require and default
//                      conditions, else `main`, else index.js), then every file its runtime imports load (static
//                      import and export-from, import() and require() with a literal specifier; `import type` loads
//                      nothing), across installed packages, summed. This is a minimal resolver, not the engine: the
//                      engine builds snapshots of workspace sources and links bare specifiers to `pkg:` nodes, so it
//                      cannot walk into node_modules. A specifier it cannot resolve is listed, never guessed.
// A package nothing measures keeps `bytes: null` with the reason. Closures are cached by name@version in the graph
// cache (graph-cache.ts `package-closure`), checked against the installed versions they walked.
import { builtinModules } from 'node:module';
import {
  existsSync,
  readFileSync,
  readdirSync,
  realpathSync,
  statSync,
} from 'node:fs';
import {
  basename,
  dirname,
  extname,
  join,
  relative,
  resolve,
  sep,
} from 'node:path';
import {
  bundleFormat,
  readEsbuildMetafile,
  readRolldownAnalyze,
  readRollupBundle,
  readWebpackStats,
} from './bundle-ingest.ts';
import { getDerived, putDerived } from './graph-cache.ts';
import { errorMessage, isRecord } from './lib.ts';
import type { BytesEvidence, PackageSize } from './package-bytes.ts';
import type { Observation, ProviderOptions } from '../providers.ts';
import type { GraphData } from './types.ts';

// Where bundlers write by convention, under the root and under each chunk; files named like a metafile or stats dump.
const OUTPUT_DIRS = [
  '.',
  'dist',
  'build',
  'out',
  'output',
  '.output',
  'stats',
  'reports',
];
const BUNDLE_FILE =
  /(^|[.\-_])(meta|metafile|stats|analyze-data|bundle-stats|webpack-stats|rollup-bundle|bundle)([.\-_][^/]*)?\.json$/i;

const READERS: Record<
  string,
  (data: never, opts: ProviderOptions) => { relations: Observation[] }
> = {
  'esbuild-metafile': readEsbuildMetafile,
  'rollup-bundle': readRollupBundle,
  'rolldown-analyze': readRolldownAnalyze,
  'webpack-stats': readWebpackStats,
};

/** A bundler file found under an output directory: its path, format and the directory the bundler ran in. */
export interface FoundBundle {
  file: string;
  format: string;
  base: string;
}

const isDir = (p: string) =>
  statSync(p, { throwIfNoEntry: false })?.isDirectory() ?? false;

/** Bundler files under the output directories of `bases` (and one level below them), by their content's shape. */
export function discoverBundles(bases: readonly string[]): FoundBundle[] {
  const found: FoundBundle[] = [];
  const seen = new Set<string>();
  for (const base of new Set(bases))
    for (const out of OUTPUT_DIRS) {
      const dir = join(base, out);
      if (!isDir(dir)) continue;
      const dirs = [
        dir,
        ...(out === '.'
          ? []
          : readdirSync(dir, { withFileTypes: true })
              .filter(e => e.isDirectory())
              .map(e => join(dir, e.name))),
      ];
      for (const d of dirs)
        for (const e of readdirSync(d, { withFileTypes: true })) {
          const file = join(d, e.name);
          if (!e.isFile() || !BUNDLE_FILE.test(e.name) || seen.has(file))
            continue;
          seen.add(file);
          let data: unknown;
          try {
            data = JSON.parse(readFileSync(file, 'utf8'));
          } catch {
            continue;
          }
          const format = bundleFormat(data);
          if (format) found.push({ file, format, base });
        }
    }
  return found;
}

/** The installed package an input path belongs to: the name after its last node_modules/, else null. */
export function nodeModulesPackage(id: string) {
  const at = id.lastIndexOf('node_modules/');
  if (at < 0) return null;
  const parts = id.slice(at + 'node_modules/'.length).split('/');
  return parts[0].startsWith('@') ? parts.slice(0, 2).join('/') : parts[0];
}

/** Per package: the bytes of its inputs in a bundle (each input once, at its largest in any output). */
export function bundlePackageBytes(
  found: readonly FoundBundle[],
  root: string
) {
  const best = new Map<
    string,
    { bytes: number; files: number; evidence: BytesEvidence }
  >();
  for (const f of found) {
    const artifact = relative(root, f.file).split(sep).join('/');
    const data = JSON.parse(readFileSync(f.file, 'utf8')) as never;
    const { relations } = READERS[f.format](data, {
      artifact,
      sourceRoot: root,
      workingDirectory: f.base,
    });
    const measure = relations.some(
      r =>
        r.relation === 'OUTPUTS' &&
        isRecord(r.details) &&
        r.details.measure === 'module'
    )
      ? 'module'
      : 'output';
    const inputs = new Map<string, number>();
    for (const r of relations)
      if (r.relation === 'BUNDLES' && isRecord(r.details)) {
        const bytes = Number(r.details.bytes ?? 0);
        inputs.set(r.target, Math.max(inputs.get(r.target) ?? 0, bytes));
      }
    const byPackage = new Map<string, { bytes: number; files: number }>();
    for (const [input, bytes] of inputs) {
      const name = nodeModulesPackage(input);
      if (!name) continue;
      const p = byPackage.get(name) ?? { bytes: 0, files: 0 };
      p.bytes += bytes;
      p.files++;
      byPackage.set(name, p);
    }
    for (const [name, p] of byPackage) {
      const known = best.get(name);
      if (!known || p.bytes > known.bytes)
        best.set(name, {
          ...p,
          evidence: { source: artifact, files: p.files, measure },
        });
    }
  }
  return best;
}

// ---- the installed closure ----

const isObj = (v: unknown): v is Record<string, unknown> =>
  v !== null && typeof v === 'object' && !Array.isArray(v);

const BUILTINS = new Set(builtinModules);
const isBuiltin = (spec: string) =>
  spec.startsWith('node:') ||
  BUILTINS.has(spec.split('/')[0]) ||
  BUILTINS.has(spec);

/** How a specifier loads: an ES import (static or import()) or a CommonJS require. */
type LoadKind = 'import' | 'require';

const CONDITIONS: Record<LoadKind, Set<string>> = {
  import: new Set(['node', 'import', 'module-sync', 'default']),
  require: new Set(['node', 'require', 'module-sync', 'default']),
};

const stringOr = (value: unknown, otherwise = '') =>
  typeof value === 'string' ? value : otherwise;

function readPackage(dir: string): Record<string, unknown> | null {
  try {
    const pj: unknown = JSON.parse(
      readFileSync(join(dir, 'package.json'), 'utf8')
    );
    return isObj(pj) ? pj : null;
  } catch {
    return null;
  }
}

// A conditional export target: the first condition this load matches, in the order the package lists them.
function conditional(target: unknown, conditions: Set<string>): string | null {
  if (typeof target === 'string') return target;
  if (Array.isArray(target)) {
    for (const t of target) {
      const r = conditional(t, conditions);
      if (r) return r;
    }
    return null;
  }
  if (isObj(target))
    for (const [k, v] of Object.entries(target))
      if (conditions.has(k)) {
        const r = conditional(v, conditions);
        if (r) return r;
      }
  return null;
}

// `exports` (or `imports`) for a subpath (`.`, `./x`, `#x`): exact keys, then the longest `*` pattern.
function mapped(map: unknown, subpath: string, conditions: Set<string>) {
  if (
    typeof map === 'string' ||
    Array.isArray(map) ||
    (isObj(map) &&
      !Object.keys(map).some(k => k.startsWith('.') || k.startsWith('#')))
  )
    return subpath === '.' ? conditional(map, conditions) : undefined;
  if (!isObj(map)) return undefined;
  if (Object.hasOwn(map, subpath)) return conditional(map[subpath], conditions);
  let best: { key: string; match: string } | undefined;
  for (const key of Object.keys(map)) {
    const star = key.indexOf('*');
    if (star < 0) continue;
    const [prefix, suffix] = [key.slice(0, star), key.slice(star + 1)];
    if (
      subpath.startsWith(prefix) &&
      subpath.endsWith(suffix) &&
      subpath.length >= key.length - 1 &&
      (!best || prefix.length > best.key.indexOf('*'))
    )
      best = {
        key,
        match: subpath.slice(prefix.length, subpath.length - suffix.length),
      };
  }
  if (!best) return undefined;
  return (
    conditional(map[best.key], conditions)?.replaceAll('*', best.match) ?? null
  );
}

const EXTENSIONS = ['.js', '.json', '.node', '.mjs', '.cjs'];

// A path as Node and bundlers find it: the file itself, then with an extension, then the directory's index.
function fileAt(path: string): string | null {
  const s = statSync(path, { throwIfNoEntry: false });
  if (s?.isFile()) return path;
  for (const ext of EXTENSIONS)
    if (statSync(path + ext, { throwIfNoEntry: false })?.isFile())
      return path + ext;
  if (s?.isDirectory()) {
    const main = readPackage(path)?.main;
    if (typeof main === 'string') {
      const m = fileAt(resolve(path, main));
      if (m) return m;
    }
    for (const ext of EXTENSIONS) {
      const index = join(path, `index${ext}`);
      if (statSync(index, { throwIfNoEntry: false })?.isFile()) return index;
    }
  }
  return null;
}

function packageDir(name: string, from: string) {
  for (let d = from; ; d = dirname(d)) {
    const p = join(d, 'node_modules', name);
    if (existsSync(join(p, 'package.json'))) return p;
    if (dirname(d) === d) return null;
  }
}

// A package's file for a subpath (`.` is its entry), as Node resolves it for this kind of load.
function packageFile(dir: string, subpath: string, kind: LoadKind) {
  const pj = readPackage(dir) ?? {};
  if (pj.exports !== undefined) {
    const target = mapped(pj.exports, subpath, CONDITIONS[kind]);
    return target ? fileAt(resolve(dir, target)) : null;
  }
  if (subpath !== '.') return fileAt(resolve(dir, subpath));
  return fileAt(
    resolve(dir, typeof pj.main === 'string' ? pj.main : 'index.js')
  );
}

// The package.json that owns a file: the nearest one with a name.
function ownerOf(file: string) {
  for (let d = dirname(file); dirname(d) !== d; d = dirname(d)) {
    const pj = readPackage(d);
    if (pj && typeof pj.name === 'string') return { dir: d, pj };
  }
  return null;
}

// Source text without comments, so a commented-out require loads nothing (string and template contents stay).
function withoutComments(text: string) {
  let out = '';
  let quote: string | null = null;
  for (let i = 0; i < text.length; i++) {
    const c = text[i];
    if (quote) {
      out += c;
      if (c === '\\') out += text[++i] ?? '';
      else if (c === quote || (c === '\n' && quote !== '`')) quote = null;
    } else if (c === '/' && text[i + 1] === '/') {
      while (i < text.length && text[i] !== '\n') i++;
      out += '\n';
    } else if (c === '/' && text[i + 1] === '*') {
      const end = text.indexOf('*/', i + 2);
      i = end < 0 ? text.length : end + 1;
      out += ' ';
    } else {
      if (c === '"' || c === "'" || c === '`') quote = c;
      out += c;
    }
  }
  return out;
}

const SPECIFIERS: [RegExp, LoadKind][] = [
  [/\brequire\s*\(\s*(['"`])([^'"`\n]+)\1\s*\)/g, 'require'],
  [/\bimport\s*\(\s*(['"`])([^'"`\n]+)\1\s*[,)]/g, 'import'],
  [
    /(?:^|[^\w$.])(?:import|export)(?!\s+type\b)\s*(?:[\w$*{}\s,]+?\s*from\s*)?(['"])([^'"\n]+)\1/g,
    'import',
  ],
];

/** The runtime specifiers a JavaScript file names, with how each loads. */
export function runtimeSpecifiers(text: string) {
  const code = withoutComments(text);
  const out: { specifier: string; kind: LoadKind }[] = [];
  for (const [pattern, kind] of SPECIFIERS)
    for (const m of code.matchAll(pattern)) out.push({ specifier: m[2], kind });
  return out;
}

const SCRIPT = new Set(['.js', '.mjs', '.cjs', '.jsx']);

/** An installed package's closure: the files its entry loads, their bytes, and what it could not resolve. */
export interface Closure {
  entry: string;
  version: string;
  bytes: number;
  files: number;
  packages: string[];
  unresolved: { specifier: string; from: string }[];
  /** Package directory → the version walked: a cached closure holds while each is still installed. */
  walked: Record<string, string>;
}

/** The closure of `name` as `kind` loads it from `fromDir`; a string is why there is none. */
export function installedClosure(
  name: string,
  fromDir: string,
  kind: LoadKind,
  { cache = true, home }: { cache?: boolean; home?: string } = {}
): Closure | string {
  const [pkg, ...sub] = name.startsWith('@')
    ? [name.split('/').slice(0, 2).join('/'), ...name.split('/').slice(2)]
    : name.split('/');
  const dir = packageDir(pkg, fromDir);
  if (!dir)
    return `not installed: no node_modules/${pkg} in the importer's directory or above it`;
  const real = realpathSync(dir);
  const version = stringOr(readPackage(real)?.version, 'unknown');
  const parts = { closure: 1, name, version, dir: real, kind };
  const opts = home ? { home } : {};
  if (cache) {
    const known = getDerived('package-closure', parts, opts) as Closure | null;
    if (
      known &&
      Object.entries(known.walked).every(
        ([d, v]) => stringOr(readPackage(d)?.version) === v
      )
    )
      return known;
  }
  const entry = packageFile(
    real,
    sub.length ? `./${sub.join('/')}` : '.',
    kind
  );
  if (!entry)
    return `${pkg}@${version} has no entry for ${kind} (exports, main and index.js resolve to no file)`;
  const files = new Set<string>([entry]);
  const unresolved: Closure['unresolved'] = [];
  const walked: Record<string, string> = {};
  for (const file of files) {
    const owner = ownerOf(file);
    if (owner) walked[owner.dir] = stringOr(owner.pj.version);
    if (!SCRIPT.has(extname(file))) continue;
    let text: string;
    try {
      text = readFileSync(file, 'utf8');
    } catch {
      continue;
    }
    for (const { specifier, kind: how } of runtimeSpecifiers(text)) {
      if (isBuiltin(specifier)) continue;
      let target: string | null = null;
      if (specifier.startsWith('.') || specifier.startsWith('/'))
        target = fileAt(resolve(dirname(file), specifier));
      else if (specifier.startsWith('#')) {
        const t = owner
          ? mapped(owner.pj.imports, specifier, CONDITIONS[how])
          : undefined;
        target = t
          ? t.startsWith('.')
            ? fileAt(resolve(owner!.dir, t))
            : null
          : null;
      } else {
        const segs = specifier.split('/');
        const dep = specifier.startsWith('@')
          ? segs.slice(0, 2).join('/')
          : segs[0];
        const rest = segs.slice(specifier.startsWith('@') ? 2 : 1);
        const d = packageDir(dep, dirname(file));
        target = d
          ? packageFile(
              realpathSync(d),
              rest.length ? `./${rest.join('/')}` : '.',
              how
            )
          : null;
      }
      if (target) files.add(realpathSync(target));
      else unresolved.push({ specifier, from: file });
    }
  }
  let bytes = 0;
  for (const f of files) bytes += statSync(f).size;
  const closure: Closure = {
    entry,
    version,
    bytes,
    files: files.size,
    packages: [
      ...new Set(
        Object.entries(walked).map(
          ([d, v]) => `${stringOr(readPackage(d)?.name, basename(d))}@${v}`
        )
      ),
    ],
    unresolved,
    walked,
  };
  if (cache) putDerived('package-closure', parts, closure, opts);
  return closure;
}

const JS = new Set(['javascript', 'typescript']);

/**
 * Every external package of `D` with its bytes: a found bundle's when it holds the package, else the installed
 * closure from its first JavaScript or TypeScript importer, else null with the reason. Never throws: a failure is the
 * package's reason.
 */
export function measurePackages(
  D: GraphData,
  {
    root,
    bases,
    cache,
    home,
  }: { root: string; bases: string[]; cache: boolean; home?: string }
) {
  let bundles = new Map<
    string,
    { bytes: number; files: number; evidence: BytesEvidence }
  >();
  let bundleError: string | undefined;
  try {
    bundles = bundlePackageBytes(discoverBundles([root, ...bases]), root);
  } catch (error) {
    bundleError = errorMessage(error).split('\n')[0];
  }
  const language = new Map(D.files.map(f => [f.id, f.language]));
  // Closure paths are real paths (a symlinked root, /var and /private/var on macOS, is the same directory).
  const realRoot = realpathSync(root);
  return (D.externals ?? []).map(x => {
    const size = ((): PackageSize => {
      const name = x.id.startsWith('pkg:') ? x.id.slice(4) : x.id;
      const importers = x.files ?? [];
      const kinds = Object.values(x.kinds ?? {}).flat();
      if (kinds.includes('external-builtin') || isBuiltin(name))
        return {
          bytes: null,
          bytesMethod: null,
          bytesEvidence: {
            reason: 'a runtime builtin: it ships with the runtime',
          },
        };
      const bundled = bundles.get(
        name.startsWith('@')
          ? name.split('/').slice(0, 2).join('/')
          : name.split('/')[0]
      );
      if (bundled)
        return {
          bytes: bundled.bytes,
          bytesMethod: 'bundle',
          bytesEvidence: bundled.evidence,
        };
      const js = importers.find(f => JS.has(language.get(f) ?? ''));
      if (!js) {
        const langs = [
          ...new Set(importers.map(f => language.get(f) ?? 'unknown')),
        ];
        return {
          bytes: null,
          bytesMethod: null,
          bytesEvidence: {
            reason: `no bundle holds it, and installed closures measure npm packages (importers: ${langs.join(', ')})${bundleError ? `; bundle files unread: ${bundleError}` : ''}`,
          },
        };
      }
      const kind: LoadKind =
        (x.kinds?.[js] ?? []).some(k => /require/.test(k)) &&
        !(x.kinds?.[js] ?? []).some(k => /import/.test(k))
          ? 'require'
          : 'import';
      let c: Closure | string;
      try {
        c = installedClosure(name, dirname(join(root, js)), kind, {
          cache,
          ...(home && { home }),
        });
      } catch (error) {
        c = `closure failed: ${errorMessage(error).split('\n')[0]}`;
      }
      // The importer the package was resolved from.
      if (typeof c === 'string')
        return {
          bytes: null,
          bytesMethod: null,
          bytesEvidence: { reason: c, source: js },
        };
      const rel = (p: string) => relative(realRoot, p).split(sep).join('/');
      return {
        bytes: c.bytes,
        bytesMethod: 'installed-closure',
        bytesEvidence: {
          source: rel(c.entry),
          files: c.files,
          version: c.version,
          packages: c.packages,
          ...(c.unresolved.length && {
            unresolved: c.unresolved.map(u => ({
              specifier: u.specifier,
              from: rel(u.from),
            })),
          }),
        },
      };
    })();
    return { ...x, ...size };
  });
}
