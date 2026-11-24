// The shared graph cache: content-addressed results that every build, refresh and result directory reuses, kept in
// the Octocode home (OCTOCODE_HOME, else ~/.octocode) under graph-cache/. Keys hold everything that changes an
// answer, never a modification time, so a hit is the answer a new run would compute:
//   chunks/<k>/   a chunk's section result and evidence. Key: the engine executable's digest, this package's version
//                 and code digest, the options that change output, the section path and every file it owns (content
//                 digests of graph sources and configs; the other listed paths by name). The entry also records the
//                 configs the engine reads outside the chunk (its snapshot manifest's `inputs`, plus the conventional
//                 config names of every ancestor up to the workspace): a lookup checks each one's digest again.
//   lsp/<k>.json  one declaration's callees answer. Key: the engine digest, the language server route that answers
//                 its file (command, arguments, the executables' stamps), the configs of its directories, its file
//                 digest, name, line and kind. The entry records the digest of every file the answer points into; a
//                 lookup checks them again. The LSP mode is not part of the key: it chooses which declarations are
//                 probed, never what one probe answers, so `--lsp all` reuses the answers `--lsp smart` paid for.
//   derived/<namespace>/<k>.json.gz   data derived the same way (package closures, search indexes): getDerived and
//                 putDerived take the namespace and the parts that decide it.
// A version bump changes every key, so nothing is migrated. A corrupt entry is a miss: logged, removed, rewritten.
// Entries are written whole under tmp/ and renamed into place; readers touch what they use, and `cache prune`
// evicts the least recently used.
import { createHash, randomUUID } from 'node:crypto';
import {
  constants,
  copyFileSync,
  existsSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  realpathSync,
  renameSync,
  rmSync,
  statSync,
  utimesSync,
  writeFileSync,
} from 'node:fs';
import { readFile, stat } from 'node:fs/promises';
import {
  basename,
  delimiter,
  dirname,
  extname,
  isAbsolute,
  join,
  relative,
  resolve,
  sep,
} from 'node:path';
import { fileURLToPath } from 'node:url';
import { gunzipSync, gzipSync } from 'node:zlib';
import { registryHome } from './registry.ts';
import {
  engineDigest,
  errorMessage,
  isRecord,
  nativeWorkspaceRoot,
  octocode,
  pool,
} from './lib.ts';
import type { BuiltGraph } from './graph-pages.ts';
import type { Emit, SectionData } from './types.ts';

/** Bumped when an entry's layout or meaning changes: every key changes with it. */
export const CACHE_FORMAT = 1;

/** The cache directory: graph-cache in the Octocode home (OCTOCODE_HOME when set, else ~/.octocode). */
export function cacheHome(env: NodeJS.ProcessEnv = process.env) {
  return join(dirname(registryHome(env)), 'graph-cache');
}

/** Whether a build or refresh reads and writes the cache (`--no-cache`: off). Every lookup is a `cache` event. */
export interface CacheSettings {
  enabled: boolean;
}

/** Why a lookup missed: the first key part that differs from the nearest previous entry. */
export interface MissReason {
  part: string;
  detail: string;
}

/** A `cache` event: one chunk, or one probe group (the declarations of one file), hit or missed. */
export interface CacheEvent {
  type: 'cache';
  scope: 'chunk' | 'lsp';
  id: string;
  state: 'hit' | 'miss' | 'off';
  key?: string;
  hits?: number;
  misses?: number;
  reason?: MissReason;
}

function canonical(value: unknown): string {
  if (Array.isArray(value)) return `[${value.map(canonical).join(',')}]`;
  if (isRecord(value))
    return `{${Object.keys(value)
      .filter(k => value[k] !== undefined)
      .sort()
      .map(k => `${JSON.stringify(k)}:${canonical(value[k])}`)
      .join(',')}}`;
  return JSON.stringify(value) ?? 'null';
}

/** The key of a value: sha256 of its canonical JSON (object keys sorted). */
export const cacheKey = (value: unknown) =>
  createHash('sha256').update(canonical(value)).digest('hex');

const short = (digest: string | null | undefined) =>
  digest ? digest.slice(0, 12) : 'absent';

// File digests, remembered per process by the file's identity (device, inode, size, both times): an edit changes the
// change time, so a remembered digest is never served for other content.
const digests = new Map<string, { stamp: string; digest: string }>();

/** The sha256 of a file's content; null when it is missing or not a file. */
export async function fileDigest(path: string): Promise<string | null> {
  let s;
  try {
    s = await stat(path, { bigint: true });
  } catch {
    return null;
  }
  if (!s.isFile()) return null;
  const stamp = `${s.dev}-${s.ino}-${s.size}-${s.mtimeNs}-${s.ctimeNs}`;
  const known = digests.get(path);
  if (known?.stamp === stamp) return known.digest;
  let bytes;
  try {
    bytes = await readFile(path);
  } catch {
    return null;
  }
  const digest = createHash('sha256').update(bytes).digest('hex');
  digests.set(path, { stamp, digest });
  return digest;
}

/** Digests of many absolute paths, by path. */
export async function fileDigests(paths: readonly string[]) {
  const unique = [...new Set(paths)];
  const values = await pool(unique, 16, fileDigest);
  return Object.fromEntries(unique.map((p, i) => [p, values[i]])) as Record<
    string,
    string | null
  >;
}

// The package that runs this module: its version and the digest of the code that shapes a chunk's result. From
// sources (tests, a checkout) that is the import closure of section.ts and this module; from the published build,
// every bundled Node module.
let packageId: { version: string; code: string } | undefined;
export function packageIdentity() {
  if (packageId) return packageId;
  const here = fileURLToPath(import.meta.url);
  let dir = dirname(here);
  while (!existsSync(join(dir, 'package.json')) && dirname(dir) !== dir)
    dir = dirname(dir);
  let version = 'unknown';
  try {
    const pj: unknown = JSON.parse(
      readFileSync(join(dir, 'package.json'), 'utf8')
    );
    if (isRecord(pj) && typeof pj.version === 'string') version = pj.version;
  } catch {
    /* a package without a readable manifest keeps the code digest only */
  }
  const files = here.endsWith('.ts')
    ? importClosure([join(dirname(here), 'section.ts'), here])
    : bundledModules(dir);
  const hash = createHash('sha256');
  for (const f of files.toSorted()) {
    hash.update(relative(dir, f));
    hash.update('\0');
    hash.update(readFileSync(f));
    hash.update('\0');
  }
  packageId = { version, code: hash.digest('hex') };
  return packageId;
}

function importClosure(roots: string[]) {
  const seen = new Set<string>();
  const queue = [...roots];
  for (const file of queue) {
    if (seen.has(file) || !existsSync(file)) continue;
    seen.add(file);
    const text = readFileSync(file, 'utf8');
    for (const m of text.matchAll(
      /\b(?:from|import)\s*\(?\s*['"](\.{1,2}\/[^'"]+)['"]/g
    ))
      queue.push(resolve(dirname(file), m[1]));
  }
  return [...seen];
}

function bundledModules(pkg: string) {
  const dist = join(pkg, 'dist');
  if (!existsSync(dist)) return [];
  return readdirSync(dist, { recursive: true, withFileTypes: true })
    .filter(
      e =>
        e.isFile() &&
        /\.m?js$/.test(e.name) &&
        !/^(browser|assets)(\/|$)/.test(
          relative(dist, e.parentPath).split(sep).join('/')
        )
    )
    .map(e => join(e.parentPath, e.name));
}

// Config files the engine and the language servers read beside the sources (package manifests, TypeScript and
// JavaScript projects, Cargo, Go and Python projects). A superset is safe: a change only costs a miss.
const CONFIG =
  /^(package\.json|[tj]sconfig(\..*)?\.json|deno\.jsonc?|Cargo\.(toml|lock)|go\.(mod|work|sum)|pyproject\.toml|setup\.cfg|pnpm-workspace\.yaml)$/;
const CONFIG_NAMES = [
  'package.json',
  'tsconfig.json',
  'tsconfig.base.json',
  'jsconfig.json',
  'deno.json',
  'Cargo.toml',
  'Cargo.lock',
  'go.mod',
  'go.work',
  'pyproject.toml',
  'setup.cfg',
  'pnpm-workspace.yaml',
];

// Directories from `dir` up to `top` (inclusive) when `dir` lies inside it; else none.
function ancestors(dir: string, top: string) {
  const out: string[] = [];
  const rel = relative(top, dir);
  if (rel.startsWith('..') || isAbsolute(rel)) return out;
  for (let d = dir; ; d = dirname(d)) {
    out.push(d);
    if (d === top || dirname(d) === d) break;
  }
  return out;
}

const readJson = (file: string): unknown =>
  JSON.parse(readFileSync(file, 'utf8'));

// Copy-on-write where the filesystem supports it: entries and result directories never share an inode, so writing
// one never changes the other.
function copyTree(from: string, to: string) {
  mkdirSync(to, { recursive: true });
  for (const e of readdirSync(from, { withFileTypes: true })) {
    const [src, dst] = [join(from, e.name), join(to, e.name)];
    if (e.isDirectory()) copyTree(src, dst);
    else if (e.isFile()) copyFileSync(src, dst, constants.COPYFILE_FICLONE);
  }
}

function touch(path: string) {
  const now = new Date();
  try {
    utimesSync(path, now, now);
  } catch {
    /* an entry removed meanwhile is a miss next time */
  }
}

function writeAtomic(file: string, data: string | Buffer) {
  mkdirSync(dirname(file), { recursive: true });
  const tmp = `${file}.${process.pid}.${randomUUID()}.tmp`;
  writeFileSync(tmp, data);
  renameSync(tmp, file);
}

const shard = (key: string) => key.slice(0, 2);

/** The parts a chunk's result depends on: its key is their digest. */
export interface ChunkParts {
  format: number;
  engine: string;
  package: { version: string; code: string };
  /** The Octocode settings the engine reads (config files and OCTOCODE_* variables), as one digest. */
  settings: string;
  options: { exclude: string[]; file: string | null; cross: boolean };
  section: string;
  /** Section-relative path → content digest for graph sources and configs, `listed` for any other listed file. */
  files: Record<string, string | null>;
}

interface ChunkMeta {
  format: number;
  key: string;
  parts: ChunkParts;
  /** Absolute config paths outside the chunk's files → digest (null: absent), checked again on every lookup. */
  inputs: Record<string, string | null>;
  snapshot: { id: string; counts: unknown };
  /** The engine's own record of its versions, from the snapshot manifest when it could be read. */
  reported?: Record<string, unknown>;
  at: string;
}

/** A chunk's lookup: its key, the entry that answers it (null on a miss) and why it missed. */
export interface ChunkProbe {
  key: string;
  hit: boolean;
  reason?: MissReason;
  /** On a hit: the cached result restored into `dir` for the fresh snapshot, or null when the engine disagrees. */
  restore(dir: string, snap: BuiltGraph): Promise<SectionData | null>;
  /** After a build: the result in `dir` stored under this key. */
  store(dir: string, data: SectionData): Promise<void>;
}

/** What a chunk build asks the cache with. */
export interface ChunkRequest {
  id: string;
  section: string;
  /** The section's own directory (a file section: its directory). */
  sectionDir: string;
  /** Section-relative paths of every listed file the chunk owns. */
  files: string[];
  exclude: string[];
  file: string | null;
  cross: boolean;
  /** Extensions with a graph grammar: their content is part of the key. */
  extensions: ReadonlySet<string>;
}

/** The lspSearch route that serves an extension, as `octocode lsp-server status --json` lists it. */
interface Route {
  extension?: string;
  available?: boolean;
  server?: {
    command?: string;
    args?: string[];
    source?: string;
    languageId?: string;
    initializationOptions?: unknown;
  };
}

// A server executable's identity without running it: its real path, size and modification time, and the
// `name@version` of the package that holds it, when one does.
function executableStamp(path: string) {
  let file = path;
  if (!isAbsolute(file)) {
    const found = (process.env.PATH ?? '')
      .split(delimiter)
      .filter(Boolean)
      .map(d => join(d, file))
      .find(p => statSync(p, { throwIfNoEntry: false })?.isFile());
    if (!found) return { path, missing: true };
    file = found;
  }
  try {
    const real = realpathSync(file);
    const s = statSync(real);
    if (!s.isFile()) return null;
    let pkg: string | undefined;
    for (let d = dirname(real); dirname(d) !== d; d = dirname(d)) {
      const pj = join(d, 'package.json');
      if (!existsSync(pj)) continue;
      const m = readJson(pj);
      if (isRecord(m)) pkg = `${String(m.name)}@${String(m.version)}`;
      break;
    }
    return { path: real, size: s.size, mtime: s.mtimeMs, pkg };
  } catch {
    return { path, missing: true };
  }
}

/** One declaration's callees answer as lspSearch gave it: every definition with its absolute path and call sites. */
export interface RawCallee {
  path: string;
  symbolName: string;
  line: number;
  sites: { line?: number }[];
}

/** A declaration probed for callees. */
export interface ProbedDeclaration {
  id: string;
  file: string;
  name: string;
  line: number;
  kind: string;
}

/** The LSP answers cache as lspPhase sees it (lsp.ts): look up a batch of declarations, store complete answers. */
export interface ProbeCache {
  lookup(
    probes: readonly ProbedDeclaration[]
  ): Promise<Map<string, RawCallee[]>>;
  store(probe: ProbedDeclaration, callees: RawCallee[]): void;
  /** Writes the stored answers and reports each probe group (the declarations of one file). */
  flush(): Promise<void>;
}

interface LspEntry {
  format: number;
  key: string;
  file: string;
  name: string;
  line: number;
  deps: Record<string, string | null>;
  callees: RawCallee[];
  at: string;
}

/** The declarations of one file in one LSP pass: the key parts they share, and how many hit and missed. */
interface LspGroup {
  parts: LspGroupParts;
  hits: number;
  misses: number;
  reason?: MissReason;
}

interface LspGroupParts {
  engine: string;
  server: string;
  configs: string;
  digest: string | null;
}

/** The cache of one build or refresh: settings, the identities every key shares, and the record of lookups. */
export class RunCache {
  readonly settings: CacheSettings;
  readonly home: string;
  readonly events: CacheEvent[] = [];
  private readonly emit: Emit;
  private shared?: Promise<{
    engine: string;
    package: { version: string; code: string };
    settings: string;
    workspace: string;
  }>;
  private routes?: Promise<Map<string, string> | null>;
  private readonly configMemo = new Map<string, Promise<string>>();

  constructor(settings: CacheSettings, emit: Emit, home = cacheHome()) {
    this.settings = settings;
    this.emit = emit;
    this.home = home;
  }

  private note(text: string) {
    this.emit({ type: 'note', level: 'warn', text: `graph cache: ${text}` });
  }

  private record(event: Omit<CacheEvent, 'type'>) {
    const e: CacheEvent = { type: 'cache', ...event };
    this.events.push(e);
    this.emit({ ...e });
  }

  // The parts every key shares, read once per run.
  private identities() {
    this.shared ??= (async () => {
      const [engine, workspace, settings] = await Promise.all([
        engineDigest(),
        nativeWorkspaceRoot(),
        settingsDigest(),
      ]);
      return { engine, package: packageIdentity(), settings, workspace };
    })();
    return this.shared;
  }

  // The conventional configs of `dir` and its ancestors up to the workspace root, as one digest.
  private configsOf(dir: string, workspace: string) {
    let found = this.configMemo.get(dir);
    if (!found) {
      found = (async () =>
        cacheKey(
          await fileDigests(
            ancestors(dir, workspace).flatMap(d =>
              CONFIG_NAMES.map(n => join(d, n))
            )
          )
        ))();
      this.configMemo.set(dir, found);
    }
    return found;
  }

  private corrupt(path: string, error: unknown) {
    this.note(
      `corrupt entry ${path} (${errorMessage(error).split('\n')[0]}): removed, rebuilt`
    );
    rmSync(path, { recursive: true, force: true });
  }

  // ---- chunks ----

  private chunkDir(key: string) {
    return join(this.home, 'chunks', shard(key), key);
  }

  private sectionIndex(section: string) {
    return join(this.home, 'chunks', 'sections', `${cacheKey(section)}.json`);
  }

  private readMeta(dir: string): ChunkMeta | null {
    const file = join(dir, 'meta.json');
    if (!existsSync(dir)) return null;
    try {
      const meta = readJson(file);
      if (
        !isRecord(meta) ||
        meta.format !== CACHE_FORMAT ||
        !isRecord(meta.parts) ||
        !isRecord(meta.inputs) ||
        !isRecord(meta.snapshot) ||
        !existsSync(join(dir, 'data.json.gz'))
      )
        throw new Error('missing fields or result');
      return meta as unknown as ChunkMeta;
    } catch (error) {
      this.corrupt(dir, error);
      return null;
    }
  }

  // The nearest previous entry of a section: the most recently used one with another key.
  private nearestChunk(section: string, key: string) {
    let keys: string[] = [];
    try {
      const index = readJson(this.sectionIndex(section));
      if (isRecord(index) && Array.isArray(index.keys))
        keys = index.keys.filter((k): k is string => typeof k === 'string');
    } catch {
      return null;
    }
    const metas = keys
      .filter(k => k !== key)
      .flatMap(k => {
        const dir = this.chunkDir(k);
        const meta = existsSync(dir) ? this.readMeta(dir) : null;
        return meta
          ? [{ meta, used: statSync(join(dir, 'meta.json')).mtimeMs }]
          : [];
      });
    return metas.toSorted((a, b) => b.used - a.used)[0]?.meta ?? null;
  }

  /** Looks a chunk up; a disabled cache answers a miss that stores nothing. */
  async chunk(request: ChunkRequest): Promise<ChunkProbe> {
    if (!this.settings.enabled) {
      this.record({ scope: 'chunk', id: request.id, state: 'off' });
      return {
        key: '',
        hit: false,
        restore: async () => null,
        store: async () => {},
      };
    }
    const ids = await this.identities();
    const contentOf = (p: string) =>
      request.extensions.has(extname(p).slice(1)) || CONFIG.test(basename(p));
    const content = await fileDigests(
      request.files.filter(contentOf).map(p => join(request.sectionDir, p))
    );
    const files = Object.fromEntries(
      request.files
        .toSorted()
        .map(p => [
          p,
          contentOf(p) ? content[join(request.sectionDir, p)] : 'listed',
        ])
    );
    const parts: ChunkParts = {
      format: CACHE_FORMAT,
      engine: ids.engine,
      package: ids.package,
      settings: ids.settings,
      options: {
        exclude: request.exclude.toSorted(),
        file: request.file,
        cross: request.cross,
      },
      section: request.section,
      files,
    };
    const key = cacheKey(parts);
    const dir = this.chunkDir(key);
    const meta = this.readMeta(dir);
    let reason: MissReason | undefined;
    if (meta) {
      const now = await fileDigests(Object.keys(meta.inputs));
      const changed = Object.keys(meta.inputs).find(
        p => now[p] !== meta.inputs[p]
      );
      if (changed)
        reason = {
          part: 'input',
          detail: `${changed} ${short(meta.inputs[changed])} → ${short(now[changed])}`,
        };
    } else reason = diffChunk(parts, this.nearestChunk(request.section, key));
    const hit = !reason;
    const record = (r?: MissReason) =>
      this.record({
        scope: 'chunk',
        id: request.id,
        state: r ? 'miss' : 'hit',
        key,
        ...(r && { reason: r }),
      });
    if (!hit) record(reason);
    return {
      key,
      hit,
      ...(reason && { reason }),
      restore: async (out, snap) => {
        // A hit's lookup found the entry.
        const m = meta!;
        if (canonical(snap.counts) !== canonical(m.snapshot.counts)) {
          const r = {
            part: 'engine',
            detail: `the engine's snapshot ${snap.graph} counts ${canonical(snap.counts)}, the cached one ${canonical(m.snapshot.counts)}`,
          };
          record(r);
          return null;
        }
        const restored = restoreChunk(dir, out, m, snap);
        touch(join(dir, 'meta.json'));
        record();
        return restored;
      },
      store: async (out, data) => {
        // A frozen copy's snapshot lives in that result directory only; a failed chunk has no result.
        if (data.frozen) return;
        await this.storeChunk(key, parts, out, data, ids.workspace);
      },
    };
  }

  private async storeChunk(
    key: string,
    parts: ChunkParts,
    out: string,
    data: SectionData,
    workspace: string
  ) {
    try {
      const manifest = await snapshotManifest(data.snapshot.id);
      const sectionDir = statSync(parts.section).isDirectory()
        ? parts.section
        : dirname(parts.section);
      const inside = (p: string) =>
        !relative(sectionDir, p).startsWith('..') &&
        parts.files[relative(sectionDir, p).split(sep).join('/')] !== undefined;
      const conventional = ancestors(
        parts.options.file ? sectionDir : dirname(sectionDir),
        workspace
      ).flatMap(d => CONFIG_NAMES.map(n => join(d, n)));
      const paths = [...conventional, ...(manifest?.inputs ?? [])].filter(
        p => !inside(p)
      );
      const meta: ChunkMeta = {
        format: CACHE_FORMAT,
        key,
        parts,
        inputs: await fileDigests(paths),
        snapshot: { id: data.snapshot.id, counts: data.snapshot.counts },
        ...(manifest?.reported && { reported: manifest.reported }),
        at: new Date().toISOString(),
      };
      const final = this.chunkDir(key);
      const tmp = join(this.home, 'tmp', randomUUID());
      mkdirSync(tmp, { recursive: true });
      try {
        copyFileSync(
          join(out, 'data.json.gz'),
          join(tmp, 'data.json.gz'),
          constants.COPYFILE_FICLONE
        );
        if (existsSync(join(out, 'evidence')))
          copyTree(join(out, 'evidence'), join(tmp, 'evidence'));
        writeFileSync(join(tmp, 'meta.json'), JSON.stringify(meta));
        rmSync(final, { recursive: true, force: true });
        mkdirSync(dirname(final), { recursive: true });
        renameSync(tmp, final);
      } finally {
        rmSync(tmp, { recursive: true, force: true });
      }
      const index = this.sectionIndex(parts.section);
      let keys: string[] = [];
      try {
        const known = readJson(index);
        if (isRecord(known) && Array.isArray(known.keys))
          keys = known.keys.filter((k): k is string => typeof k === 'string');
      } catch {
        /* a new or unreadable index starts again */
      }
      writeAtomic(
        index,
        JSON.stringify({
          section: parts.section,
          keys: [key, ...keys.filter(k => k !== key)].slice(0, 16),
        })
      );
    } catch (error) {
      this.note(`chunk entry not written: ${errorMessage(error)}`);
    }
  }

  // ---- LSP answers ----

  // Each extension's route identity, from the engine's own route listing; null when it cannot be read (the LSP
  // answers are then neither read nor written).
  private serverRoutes() {
    this.routes ??= (async () => {
      try {
        const answer = await octocode(['lsp-server', 'status', '--json']);
        const routes =
          isRecord(answer) && Array.isArray(answer.routes)
            ? (answer.routes as Route[])
            : null;
        if (!routes) throw new Error('no routes in `lsp-server status`');
        return new Map(
          routes.flatMap(r => {
            if (typeof r.extension !== 'string' || !r.server) return [];
            const { command, args = [], source, languageId } = r.server;
            const executables = [command ?? '', ...args].filter(
              p => p && (isAbsolute(p) || p === command)
            );
            return [
              [
                r.extension.replace(/^\./, ''),
                cacheKey({
                  command,
                  args,
                  source,
                  languageId,
                  initializationOptions: r.server.initializationOptions,
                  available: r.available,
                  stamps: executables.map(executableStamp),
                }),
              ] as const,
            ];
          })
        );
      } catch (error) {
        this.note(
          `LSP answers not cached: the language server routes are unknown (${errorMessage(error).split('\n')[0]})`
        );
        return null;
      }
    })();
    return this.routes;
  }

  /** The LSP answers cache for probes under `root` (lsp.ts ProbeCache); null when it is off or cannot key. */
  async probes(root: string): Promise<ProbeCache | null> {
    if (!this.settings.enabled) return null;
    const [ids, routes] = await Promise.all([
      this.identities(),
      this.serverRoutes(),
    ]);
    if (!routes) return null;
    const groups = new Map<string, LspGroup>();
    const pending: { key: string; entry: LspEntry }[] = [];
    const entryFile = (key: string) =>
      join(this.home, 'lsp', shard(key), `${key}.json`);
    const groupIndex = (file: string) =>
      join(this.home, 'lsp', 'files', `${cacheKey(file)}.json`);
    // One group per file, created once: concurrent lookups of its declarations share it.
    const pendingGroups = new Map<string, Promise<LspGroup>>();
    const groupOf = (file: string) => {
      const abs = join(root, file);
      let g = pendingGroups.get(abs);
      if (!g) {
        const ext = extname(file).slice(1);
        g = (async () => {
          const group: LspGroup = {
            parts: {
              engine: ids.engine,
              server: routes.get(ext) ?? `none:${ext}`,
              configs: await this.configsOf(dirname(abs), ids.workspace),
              digest: await fileDigest(abs),
            },
            hits: 0,
            misses: 0,
          };
          groups.set(abs, group);
          return group;
        })();
        pendingGroups.set(abs, g);
      }
      return g;
    };
    const keyOf = (p: ProbedDeclaration, parts: LspGroupParts) =>
      cacheKey({
        format: CACHE_FORMAT,
        scope: 'lsp-callees',
        ...parts,
        file: join(root, p.file),
        name: p.name,
        line: p.line,
        kind: p.kind,
      });
    // Why a declaration of this group has no entry: what changed since the group's last recorded parts.
    const groupReason = (abs: string, parts: LspGroupParts): MissReason => {
      try {
        const index = readJson(groupIndex(abs));
        const last =
          isRecord(index) && Array.isArray(index.records) && index.records[0];
        if (isRecord(last)) {
          for (const part of ['engine', 'server', 'configs'] as const)
            if (last[part] !== parts[part])
              return {
                part,
                detail: `${short(String(last[part]))} → ${short(parts[part])}`,
              };
          if (last.digest !== parts.digest)
            return {
              part: 'file',
              detail: `${abs} ${short(String(last.digest))} → ${short(parts.digest)}`,
            };
          return {
            part: 'declaration',
            detail: 'not answered in full before (new, moved or unanswered)',
          };
        }
      } catch {
        /* no index: nothing was cached for this file */
      }
      return { part: 'none', detail: 'no previous entry for this file' };
    };
    return {
      lookup: async probes => {
        const found = new Map<string, RawCallee[]>();
        await pool(probes, 16, async p => {
          const g = await groupOf(p.file);
          const abs = join(root, p.file);
          const file = entryFile(keyOf(p, g.parts));
          let entry: LspEntry | null = null;
          if (existsSync(file))
            try {
              const e = readJson(file);
              if (
                !isRecord(e) ||
                e.format !== CACHE_FORMAT ||
                !Array.isArray(e.callees) ||
                !isRecord(e.deps)
              )
                throw new Error('missing fields');
              entry = e as unknown as LspEntry;
            } catch (error) {
              this.corrupt(file, error);
            }
          let reason: MissReason | undefined;
          if (entry) {
            const deps = entry.deps;
            const now = await fileDigests(Object.keys(deps));
            const changed = Object.keys(deps).find(d => now[d] !== deps[d]);
            if (changed)
              reason = {
                part: 'callee',
                detail: `${changed} ${short(deps[changed])} → ${short(now[changed])}`,
              };
          } else reason = groupReason(abs, g.parts);
          if (reason) {
            g.misses++;
            g.reason ??= reason;
          } else {
            g.hits++;
            found.set(p.id, entry!.callees);
            touch(file);
          }
        });
        return found;
      },
      store: (p, callees) => {
        const g = groups.get(join(root, p.file));
        if (!g) return;
        pending.push({
          key: keyOf(p, g.parts),
          entry: {
            format: CACHE_FORMAT,
            key: keyOf(p, g.parts),
            file: join(root, p.file),
            name: p.name,
            line: p.line,
            deps: {},
            callees,
            at: new Date().toISOString(),
          },
        });
      },
      flush: async () => {
        for (const { key, entry } of pending)
          try {
            // An edit while the server answered: the answer may be of the new text, the key is of the old.
            if (
              (await fileDigest(entry.file)) !==
              groups.get(entry.file)?.parts.digest
            )
              continue;
            entry.deps = await fileDigests(entry.callees.map(c => c.path));
            writeAtomic(entryFile(key), JSON.stringify(entry));
          } catch (error) {
            this.note(`LSP entry not written: ${errorMessage(error)}`);
          }
        pending.length = 0;
        for (const [abs, g] of groups) {
          this.record({
            scope: 'lsp',
            id: relative(root, abs),
            state: g.misses ? 'miss' : 'hit',
            hits: g.hits,
            misses: g.misses,
            ...(g.reason && { reason: g.reason }),
          });
          try {
            const file = groupIndex(abs);
            let records: unknown[] = [];
            try {
              const known = readJson(file);
              if (isRecord(known) && Array.isArray(known.records))
                records = known.records;
            } catch {
              /* a new index */
            }
            writeAtomic(
              file,
              JSON.stringify({
                file: abs,
                records: [
                  { ...g.parts, at: new Date().toISOString() },
                  ...records,
                ].slice(0, 8),
              })
            );
          } catch (error) {
            this.note(`LSP index not written: ${errorMessage(error)}`);
          }
        }
        groups.clear();
        pendingGroups.clear();
      },
    };
  }

  /** Hits and misses so far, by scope (an LSP count is per declaration). */
  summary() {
    const of = (scope: CacheEvent['scope']) => {
      const events = this.events.filter(e => e.scope === scope);
      const final = new Map(events.map(e => [e.id, e]));
      const list = [...final.values()];
      return scope === 'chunk'
        ? {
            hit: list.filter(e => e.state === 'hit').length,
            miss: list.filter(e => e.state === 'miss').length,
          }
        : {
            hit: list.reduce((a, e) => a + (e.hits ?? 0), 0),
            miss: list.reduce((a, e) => a + (e.misses ?? 0), 0),
          };
    };
    return {
      enabled: this.settings.enabled,
      chunks: of('chunk'),
      lsp: of('lsp'),
    };
  }
}

// The first part of `parts` that differs from the nearest previous entry's: the engine, this package, the settings,
// an option, then a file (changed, added or removed).
function diffChunk(parts: ChunkParts, previous: ChunkMeta | null): MissReason {
  if (!previous)
    return { part: 'none', detail: 'no previous entry for this section' };
  const p = previous.parts;
  if (p.engine !== parts.engine) {
    const v = previous.reported;
    return {
      part: 'engine',
      detail: `executable ${short(p.engine)} → ${short(parts.engine)}${v ? ` (cached entry built by analysis ${String(v.analysisVersion)}, format ${String(v.formatVersion)})` : ''}`,
    };
  }
  if (p.package?.version !== parts.package.version)
    return {
      part: 'package',
      detail: `version ${p.package?.version} → ${parts.package.version}`,
    };
  if (p.package?.code !== parts.package.code)
    return {
      part: 'package',
      detail: `code ${short(p.package?.code)} → ${short(parts.package.code)}`,
    };
  if (p.settings !== parts.settings)
    return {
      part: 'settings',
      detail: `${short(p.settings)} → ${short(parts.settings)}`,
    };
  for (const option of ['exclude', 'file', 'cross'] as const)
    if (canonical(p.options?.[option]) !== canonical(parts.options[option]))
      return {
        part: `option ${option}`,
        detail: `${canonical(p.options?.[option])} → ${canonical(parts.options[option])}`,
      };
  const before = p.files ?? {};
  const names = [
    ...new Set([...Object.keys(before), ...Object.keys(parts.files)]),
  ].toSorted();
  const changed = names.filter(n => before[n] !== parts.files[n]);
  if (changed.length) {
    const n = changed[0];
    const what =
      before[n] === undefined
        ? 'added'
        : parts.files[n] === undefined
          ? 'removed'
          : `${short(before[n])} → ${short(parts.files[n])}`;
    return {
      part: 'file',
      detail: `${n} ${what}${changed.length > 1 ? ` (+${changed.length - 1} more)` : ''}`,
    };
  }
  return { part: 'section', detail: `${p.section} → ${parts.section}` };
}

// A cached result in `out`, for the snapshot graphBuild just answered: the same rows; the snapshot id the engine
// serves now, and the graphBuild answer as this build's ingest evidence.
function restoreChunk(
  dir: string,
  out: string,
  meta: ChunkMeta,
  snap: BuiltGraph
): SectionData {
  mkdirSync(out, { recursive: true });
  const evidence = join(dir, 'evidence');
  if (existsSync(evidence)) copyTree(evidence, join(out, 'evidence'));
  const data = JSON.parse(
    gunzipSync(readFileSync(join(dir, 'data.json.gz'))).toString('utf8')
  ) as SectionData;
  if (data.snapshot.id === snap.graph) {
    copyFileSync(
      join(dir, 'data.json.gz'),
      join(out, 'data.json.gz'),
      constants.COPYFILE_FICLONE
    );
    return data;
  }
  const { createdAt: _createdAt, ...rest } =
    data.snapshot as typeof data.snapshot & {
      createdAt?: string;
    };
  data.snapshot = { ...rest, id: snap.graph };
  Object.assign(data, {
    cache: { key: meta.key, builtFrom: meta.snapshot.id, at: meta.at },
  });
  mkdirSync(join(out, 'evidence'), { recursive: true });
  writeFileSync(
    join(out, 'evidence', 'ingest.json.gz'),
    gzipSync(JSON.stringify(snap))
  );
  writeAtomic(join(out, 'data.json.gz'), gzipSync(JSON.stringify(data)));
  return data;
}

// ---- what the engine reads besides the tree ----

let settingsMemo: Promise<{ files: string[] }> | undefined;
// The Octocode config files the engine reads (global and project .octocoderc and .env), from `octocode config`.
function settingsFilesOnce() {
  settingsMemo ??= (async () => {
    try {
      const answer = await octocode(['config', '--json']);
      const files: string[] = [];
      const add = (v: unknown) => {
        if (isRecord(v) && typeof v.path === 'string') files.push(v.path);
      };
      if (isRecord(answer)) {
        add(answer.configFile);
        add(answer.projectConfigFile);
        if (isRecord(answer.envFiles)) {
          add(answer.envFiles.global);
          add(answer.envFiles.project);
        }
      }
      return { files };
    } catch {
      return { files: [] };
    }
  })();
  return settingsMemo;
}

async function settingsFiles() {
  return (await settingsFilesOnce()).files;
}

// Variables that name this process's tools, not an engine setting: the engine digest already keys the executable.
const NOT_SETTINGS = new Set(['OCTOCODE_CLI', 'OCTOCODE_LANE']);

// The Octocode environment variables and config files, as one digest: the engine's settings come from them.
async function settingsDigest() {
  const env = Object.fromEntries(
    Object.entries(process.env)
      .filter(
        ([k]) =>
          (k.startsWith('OCTOCODE_') && !NOT_SETTINGS.has(k)) ||
          k === 'WORKSPACE_ROOT' ||
          k === 'ALLOWED_PATHS'
      )
      .toSorted(([a], [b]) => (a < b ? -1 : 1))
  );
  return cacheKey({ env, files: await fileDigests(await settingsFiles()) });
}

let graphHomeMemo: Promise<string | null> | undefined;
// Where the engine keeps snapshots: its configured graph home, else <workspace>/.octocode/graph.
function graphHome() {
  graphHomeMemo ??= (async () => {
    try {
      const answer = await octocode([
        'config',
        'get',
        'OCTOCODE_GRAPH_HOME',
        '--json',
      ]);
      if (
        isRecord(answer) &&
        answer.key === 'OCTOCODE_GRAPH_HOME' &&
        typeof answer.value === 'string'
      )
        return answer.value;
    } catch {
      /* the default below */
    }
    try {
      return join(await nativeWorkspaceRoot(), '.octocode', 'graph');
    } catch {
      return null;
    }
  })();
  return graphHomeMemo;
}

// The engine's manifest of a snapshot: the configs it read (`inputs`, absolute) and the versions it was built with.
async function snapshotManifest(id: string) {
  const home = await graphHome();
  if (!home) return null;
  try {
    const m = readJson(join(home, id, 'manifest.json'));
    if (!isRecord(m) || typeof m.root !== 'string') return null;
    const root = m.root;
    return {
      inputs: isRecord(m.inputs)
        ? Object.keys(m.inputs).map(p => resolve(root, p))
        : [],
      reported: Object.fromEntries(
        [
          'formatVersion',
          'analysisVersion',
          'factsSchemaVersion',
          'octocodeVersion',
          'buildIdentity',
        ].map(k => [k, m[k]])
      ),
    };
  } catch {
    return null;
  }
}

// ---- derived data ----

/** A derived value (a package closure, a search index) by its namespace and the parts that decide it. */
export function getDerived(
  namespace: string,
  parts: unknown,
  { home = cacheHome(), log = defaultLog }: DerivedOptions = {}
): unknown {
  const file = derivedFile(home, namespace, parts);
  if (!existsSync(file)) return null;
  try {
    const entry: unknown = JSON.parse(
      gunzipSync(readFileSync(file)).toString('utf8')
    );
    if (
      !isRecord(entry) ||
      entry.format !== CACHE_FORMAT ||
      entry.namespace !== namespace ||
      !('value' in entry)
    )
      throw new Error('missing fields');
    touch(file);
    return entry.value;
  } catch (error) {
    log(
      `graph cache: corrupt entry ${file} (${errorMessage(error).split('\n')[0]}): removed, rebuilt`
    );
    rmSync(file, { force: true });
    return null;
  }
}

/** Stores a derived value under its namespace and parts (getDerived reads it back). */
export function putDerived(
  namespace: string,
  parts: unknown,
  value: unknown,
  { home = cacheHome(), log = defaultLog }: DerivedOptions = {}
) {
  try {
    writeAtomic(
      derivedFile(home, namespace, parts),
      gzipSync(
        JSON.stringify({
          format: CACHE_FORMAT,
          namespace,
          parts,
          value,
          at: new Date().toISOString(),
        })
      )
    );
  } catch (error) {
    log(`graph cache: derived entry not written: ${errorMessage(error)}`);
  }
}

/** Where derived entries live and where a corrupt one is reported. */
export interface DerivedOptions {
  home?: string;
  log?: (text: string) => void;
}

const defaultLog = (text: string) => process.stderr.write(`${text}\n`);

function derivedFile(home: string, namespace: string, parts: unknown) {
  if (!/^[a-z0-9][a-z0-9-]*$/.test(namespace))
    throw new Error(`Invalid derived namespace ${namespace}`);
  const key = cacheKey({ format: CACHE_FORMAT, namespace, parts });
  return join(home, 'derived', namespace, shard(key), `${key}.json.gz`);
}

// ---- upkeep ----

/** One evictable entry: its path, kind, bytes and last use. */
interface Unit {
  path: string;
  kind: string;
  bytes: number;
  used: number;
}

function sizeOf(path: string): number {
  const s = statSync(path, { throwIfNoEntry: false });
  if (!s) return 0;
  if (!s.isDirectory()) return s.size;
  return readdirSync(path).reduce((a, n) => a + sizeOf(join(path, n)), 0);
}

const dirs = (path: string) =>
  existsSync(path)
    ? readdirSync(path, { withFileTypes: true })
        .filter(e => e.isDirectory())
        .map(e => e.name)
    : [];
const filesIn = (path: string) =>
  existsSync(path)
    ? readdirSync(path, { withFileTypes: true })
        .filter(e => e.isFile())
        .map(e => e.name)
    : [];

// Every entry of the cache. Indexes (chunks/sections, lsp/files) are bookkeeping, never evicted.
function units(home: string): Unit[] {
  const out: Unit[] = [];
  const chunks = join(home, 'chunks');
  for (const s of dirs(chunks).filter(d => d !== 'sections'))
    for (const k of dirs(join(chunks, s))) {
      const path = join(chunks, s, k);
      const meta = statSync(join(path, 'meta.json'), { throwIfNoEntry: false });
      out.push({
        path,
        kind: 'chunk',
        bytes: sizeOf(path),
        used: meta?.mtimeMs ?? 0,
      });
    }
  const lsp = join(home, 'lsp');
  for (const s of dirs(lsp).filter(d => d !== 'files'))
    for (const f of filesIn(join(lsp, s))) {
      const path = join(lsp, s, f);
      const st = statSync(path);
      out.push({ path, kind: 'lsp', bytes: st.size, used: st.mtimeMs });
    }
  const derived = join(home, 'derived');
  for (const ns of dirs(derived))
    for (const s of dirs(join(derived, ns)))
      for (const f of filesIn(join(derived, ns, s))) {
        const path = join(derived, ns, s, f);
        const st = statSync(path);
        out.push({
          path,
          kind: `derived:${ns}`,
          bytes: st.size,
          used: st.mtimeMs,
        });
      }
  return out;
}

/** Entries and bytes by kind, with the oldest and newest last use. */
export function cacheStats(home = cacheHome()) {
  const all = units(home);
  const byKind: Record<string, { entries: number; bytes: number }> = {};
  for (const u of all) {
    const k = (byKind[u.kind] ??= { entries: 0, bytes: 0 });
    k.entries++;
    k.bytes += u.bytes;
  }
  const used = all.map(u => u.used).filter(Boolean);
  return {
    home,
    entries: all.length,
    bytes: all.reduce((a, u) => a + u.bytes, 0),
    indexBytes:
      sizeOf(join(home, 'chunks', 'sections')) +
      sizeOf(join(home, 'lsp', 'files')),
    byKind,
    oldestUse: used.length ? new Date(Math.min(...used)).toISOString() : null,
    newestUse: used.length ? new Date(Math.max(...used)).toISOString() : null,
  };
}

/** Evicts the least recently used entries until the entries hold at most `maxBytes`. */
export function pruneCache(maxBytes: number, home = cacheHome()) {
  if (!Number.isSafeInteger(maxBytes) || maxBytes < 0)
    throw new Error('--max-bytes requires a nonnegative integer');
  const all = units(home).toSorted((a, b) => a.used - b.used);
  const before = all.reduce((a, u) => a + u.bytes, 0);
  let total = before;
  const removed: Unit[] = [];
  for (const u of all) {
    if (total <= maxBytes) break;
    rmSync(u.path, { recursive: true, force: true });
    total -= u.bytes;
    removed.push(u);
  }
  return {
    home,
    maxBytes,
    before: { entries: all.length, bytes: before },
    after: { entries: all.length - removed.length, bytes: total },
    removed: { entries: removed.length, bytes: before - total },
  };
}
