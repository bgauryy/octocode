// Shared helpers: flag parsing, async octocode CLI calls with a tool-call hook, a small worker pool.
import { AsyncLocalStorage } from 'node:async_hooks';
import { execFile, execFileSync } from 'node:child_process';
import type { ExecFileException } from 'node:child_process';
import { existsSync, mkdirSync, realpathSync, writeFileSync } from 'node:fs';
import { randomUUID } from 'node:crypto';
import { freezeExecutable } from './lib/pin-cli.ts';
import { basename, isAbsolute, join, resolve, sep } from 'node:path';

// Directories a graph build never reads; the frozen copy skips the same ones.
export const SKIPPED_DIRS = [
  'target',
  'node_modules',
  '.git',
  'dist',
  'build',
  '.octocode',
];

// The top-level folder a root-relative id belongs to; a file at the root belongs to the root, `.`, not to a folder
// named after itself.
export const topGroup = (id: string) =>
  id.includes('/') ? id.split('/')[0] : '.';

/** A JSON object (not null, not an array): its fields can be read and written by name. */
export const isRecord = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);

/** Number.isInteger as a type guard: an integer count or line. */
export const isInteger = (value: unknown): value is number =>
  Number.isInteger(value);

/** A thrown value's message: an Error's own message, else the value as text. */
export const errorMessage = (error: unknown) =>
  error instanceof Error ? error.message : String(error);

// Import degrees from production imports: test code depending on a file does not make it a hub.
export function degrees(
  imports: readonly { source: string; target: string; test?: boolean }[]
) {
  const fanIn = new Map<string, number>(),
    fanOut = new Map<string, number>();
  for (const e of imports.filter(x => !x.test)) {
    fanIn.set(e.target, (fanIn.get(e.target) ?? 0) + 1);
    fanOut.set(e.source, (fanOut.get(e.source) ?? 0) + 1);
  }
  return { fanIn, fanOut };
}

export function flags(argv: string[], spec: Record<string, string>) {
  const out: Record<string, string> = {};
  for (const [name, dflt] of Object.entries(spec)) {
    const i = argv.indexOf(`--${name}`);
    if (i >= 0 && (argv[i + 1] === undefined || argv[i + 1].startsWith('--'))) {
      process.stderr.write(`--${name} needs a value\n`);
      process.exit(2);
    }
    out[name] = i < 0 ? dflt : argv.splice(i, 2)[1];
  }
  return out;
}

// Resolve lazily so replay and offline queries need no binary.
let cli: string[] | undefined;
let resolving: Promise<string[]> | undefined;
async function selectedCli() {
  if (cli) return cli;
  resolving ??= resolveCli().catch((error: unknown) => {
    resolving = undefined;
    throw error;
  });
  return (cli ??= await resolving);
}

async function resolveCli() {
  const override = process.env.OCTOCODE_CLI;
  if (override) return existsSync(override) ? [override] : override.split(' ');
  const { resolveNativeBin } = await import('octocode/native');
  const result = await resolveNativeBin();
  if (result.error) throw new Error(result.error);
  return [realpathSync(result.bin)];
}

/** What the pinned engine's schema allows a graph run: batch sizes, target list maximums and graph grammars. */
export interface GraphLimits {
  graphBatch: number;
  lspBatch: number | null;
  maxTargets: number;
  fileGroup: number;
  extensions: Set<string>;
}

/** A JSON Schema node as the schema views print it: only what the target list maximum is read from. */
interface SchemaNode {
  $ref?: string;
  $defs?: Record<string, SchemaNode>;
  properties?: Record<string, SchemaNode>;
  anyOf?: SchemaNode[];
  type?: string;
  maxItems?: unknown;
}

/** The parts of an `octocode schema` answer a graph run reads; each one is checked where it is read. */
interface SchemaAnswer {
  error?: string;
  executable?: unknown;
  tools: { name: string; maxQueries?: unknown }[];
  grammarCapabilities?: { graphFacts?: boolean; extensions: string[] }[];
  querySchema?: SchemaNode;
}

const isPositiveInteger = (value: unknown): value is number =>
  typeof value === 'number' && Number.isSafeInteger(value) && value >= 1;

// Ask the selected launcher which executable served the catalog. Read the contract again from
// its private copy: a package install or rebuild can replace the source between discovery and copy.
let contract: Promise<GraphLimits> | undefined;
export function prepareGraphRun() {
  contract ??= loadGraphContract().catch((error: unknown) => {
    contract = undefined;
    throw error;
  });
  return contract;
}

/**
 * The sha256 of the pinned engine executable (pin-cli.ts names its frozen image by it). The engine reports its
 * analysis and format versions only inside each snapshot's manifest, never through a tool or CLI answer, so the graph
 * cache keys engine answers by this digest: every engine change, a version bump included, changes it.
 */
export async function engineDigest() {
  await prepareGraphRun();
  // prepareGraphRun pinned the executable: cli is its frozen image.
  return basename(cli![0]).replace(/\.[^.]*$/, '');
}

async function loadGraphContract(): Promise<GraphLimits> {
  const schema = (command: string[], args: string[]): SchemaAnswer => {
    let raw;
    try {
      raw = execFileSync(command[0], [...command.slice(1), ...args], {
        encoding: 'utf8',
        maxBuffer: Infinity,
      });
    } catch (error) {
      const stdout =
        error instanceof Error &&
        'stdout' in error &&
        typeof error.stdout === 'string'
          ? error.stdout
          : undefined;
      throw new Error(parseOutput(stdout)?.error ?? errorMessage(error), {
        cause: error,
      });
    }
    const value: SchemaAnswer = JSON.parse(raw);
    if (value.error) throw new Error(value.error);
    return value;
  };
  const full = ['--view', 'full'];
  const source = schema(await selectedCli(), ['schema', ...full]).executable;
  if (typeof source !== 'string' || !isAbsolute(source))
    throw new Error(
      'The CLI catalog must identify its native executable. Update Octocode and retry.'
    );
  const pinned = [await freezeExecutable(source)];
  const catalog = schema(pinned, ['schema', ...full]);
  const graph = schema(pinned, ['__graph', 'schema', ...full]);
  const limit = (name: string) => {
    const owner = name === 'graphQuery' ? graph : catalog;
    const max = owner.tools?.find(t => t.name === name)?.maxQueries;
    if (!isPositiveInteger(max))
      throw new Error(`${name} schema declares no positive queries maximum`);
    return max;
  };
  const targetLimit = (operation: string) => {
    const query = schema(pinned, [
      '__graph',
      'schema',
      'graphQuery',
      '--select',
      `operation=${operation}`,
    ]).querySchema;
    const dereference = (node: SchemaNode | undefined) => {
      const seen = new Set<SchemaNode>();
      while (node?.$ref?.startsWith('#/$defs/') && !seen.has(node)) {
        seen.add(node);
        node = query?.$defs?.[node.$ref.slice('#/$defs/'.length)];
      }
      return node;
    };
    const target = dereference(query?.properties?.target);
    const maxTargets = (target?.anyOf ?? [target])
      .map(dereference)
      .find(s => s?.type === 'array')?.maxItems;
    if (!isPositiveInteger(maxTargets))
      throw new Error(
        `graphQuery ${operation} schema declares no positive target list maximum`
      );
    return maxTargets;
  };
  if (!Array.isArray(catalog.grammarCapabilities))
    throw new Error('The CLI catalog is missing grammarCapabilities');
  const limits = {
    graphBatch: limit('graphQuery'),
    lspBatch: catalog.tools.some(t => t.name === 'lspSearch')
      ? limit('lspSearch')
      : null,
    maxTargets: targetLimit('unresolved'),
    fileGroup: Math.min(targetLimit('witnesses'), targetLimit('symbols')),
    extensions: new Set(
      catalog.grammarCapabilities
        .filter(g => g.graphFacts)
        .flatMap(g => g.extensions)
    ),
  };
  cli = pinned;
  return limits;
}

// The engine reads only inside its workspace root (WORKSPACE_ROOT, else its cwd). A build or refresh of a root outside
// this process's workspace runs the engine rooted elsewhere for that run's calls only: where its build ran, else at the
// root itself. The MCP server and the viewer serve several runs at once, so the root is per call chain, never global.
const engineRoots = new AsyncLocalStorage<string>();

/** Runs `task` with every engine call it makes rooted at `root` (cwd and WORKSPACE_ROOT); null leaves them as they are. */
export function withEngineRoot<T>(root: string | null, task: () => T): T {
  return root ? engineRoots.run(root, task) : task();
}

// Containment of real paths: a symlinked spelling (/var and /private/var on macOS) is the same directory.
const real = (path: string) => {
  try {
    return realpathSync(path);
  } catch {
    return resolve(path);
  }
};
const within = (dir: string, base: string) => {
  const [d, b] = [real(dir), real(base)];
  return d === b || d.startsWith(b.endsWith(sep) ? b : b + sep);
};

/**
 * Where the engine must be rooted to read `root`: null when this process's workspace holds it; else the workspace its
 * build recorded, when that still exists and holds the root (parent tsconfigs and manifests stay readable); else the
 * root itself.
 */
export async function engineRootFor(root: string, recorded?: string | null) {
  const current = await nativeWorkspaceRoot();
  if (recorded && within(root, recorded) && existsSync(recorded))
    return real(recorded) === real(current) ? null : recorded;
  return within(root, current) ? null : root;
}

// The engine can root its answers in a configured ancestor of the CLI caller's cwd.
// Read the effective setting from Octocode so .env, .octocoderc and defaults agree.
let workspace: Promise<string> | undefined;
export function nativeWorkspaceRoot() {
  const rooted = engineRoots.getStore();
  if (rooted) return Promise.resolve(rooted);
  workspace ??= octocode(['config', 'get', 'WORKSPACE_ROOT', '--json'])
    .then(answer => {
      // `config get --json` prints one setting object; its key is checked here.
      const setting = answer as { key?: unknown; value?: string | null };
      if (setting.key !== 'WORKSPACE_ROOT')
        throw new Error('Octocode did not report its effective workspace root');
      return resolve(setting.value ?? process.cwd());
    })
    .catch((error: unknown) => {
      workspace = undefined;
      throw error;
    });
  return workspace;
}

/**
 * The directory a local tool answer's paths are relative to: the `root` an answer names (a path in an allowed root
 * outside the workspace, such as a clone), else the workspace root.
 */
export function answerBase(data: unknown, workspaceRoot: string) {
  const root = (data as { root?: unknown } | undefined)?.root;
  return typeof root === 'string' && root ? root : workspaceRoot;
}

// Ordinary local tools use the research interface of the same pinned engine.
function nativeArgs(args: string[]) {
  const name = args[0] === 'schema' ? args[1] : args[0];
  return name === 'graphBuild' || name === 'graphQuery'
    ? ['__graph', ...args]
    : args;
}

let evidenceRoot: string | undefined;
export function captureEvidence(directory: string) {
  mkdirSync(directory, { recursive: true });
  evidenceRoot = directory;
}

/** One CLI call as the tool hook sees it: at its start, then when it is done with its answer or error. */
export interface ToolCallEvent {
  id: number;
  state: 'start' | 'done';
  args: string[];
  ms?: number;
  bytes?: number;
  exitCode?: string | number;
  value?: unknown;
  error?: string;
}
export type ToolHook = (event: ToolCallEvent) => void;

// Every CLI call is reported twice (start, done) so a viewer can show each tool, page and cursor live.
const toolHooks = new AsyncLocalStorage<ToolHook>();
let callSeq = 0;
export function withToolHook<T>(hook: ToolHook, task: () => T) {
  return toolHooks.run(hook, task);
}

// A clasify-style `next.read` is a source read, not missing evidence (TOOL_DATA_CONTRACT "source-read continuations").
const incomplete = (v: object) =>
  ('kind' in v && v.kind === 'continue') ||
  ('isPartial' in v && v.isPartial === true);
export function requireCompleteToolResponse(value: unknown): void {
  if (!value || typeof value !== 'object') return;
  if (incomplete(value))
    throw new Error(
      'Incomplete tool evidence: follow the saved continuation or rerun a narrower query before judging this layer'
    );
  for (const [key, child] of Object.entries(value))
    if (key !== 'related' && key !== 'hints')
      requireCompleteToolResponse(child);
}

/** What this module reads of a printed answer before handing it on: a batch's rows and a refusal's message. */
interface PrintedAnswer {
  results?: unknown;
  error?: string;
}

function parseOutput(stdout: string | undefined): PrintedAnswer | undefined {
  try {
    return JSON.parse(stdout ?? '');
  } catch {
    return undefined;
  }
}

// Whether stdout is an answer: exit 0, exit 1 (empty), or any exit status from a batch envelope (`results`).
const answered = (err: ExecFileException | null, stdout: string) =>
  !err ||
  err.code === 1 ||
  (Number.isInteger(err.code) && Array.isArray(parseOutput(stdout)?.results));

interface CallOutput {
  err: ExecFileException | null;
  stdout: string;
  stderr: string;
  saved: string | undefined;
}

// `octocode` exits 1 on an empty result but still prints one JSON object, and a batch exits with a rejected or
// failed row's code (1 only when every row is empty) while stdout still carries every row: both are answers. `partial: true` callers (graph pages among them) follow
// their own continuations; every other caller gets only complete evidence. No wall-clock or output cap: the runtime owns its
// deadlines. `timeout` (ms) is only for a caller that explicitly asks for one.
// The parsed answer, or the error to reject with (and to report to the tool hook).
function settle(
  args: string[],
  partial: boolean,
  { err, stdout, stderr, saved }: CallOutput
): { value?: unknown; error?: string } {
  const value = answered(err, stdout) ? parseOutput(stdout) : undefined;
  const where = saved ? `; evidence: ${saved}` : '';
  if (!value)
    return {
      error: `octocode ${args.slice(0, 2).join(' ')}: ${stderr || stdout || err?.message || 'no output'}${where}`,
    };
  try {
    if (!partial) requireCompleteToolResponse(value);
  } catch (error) {
    return { error: `${errorMessage(error)}${where}` };
  }
  return { value };
}

export interface CallOptions {
  timeout?: number;
  partial?: boolean;
  onStderr?: (line: string) => void;
}

// `onStderr` receives each stderr line as the call prints it (graphBuild progress), before the call ends.
// The answer is the printed JSON, unchecked beyond the completeness rule above: each caller reads its own fields.
export async function octocode(
  args: string[],
  { timeout = 0, partial = false, onStderr }: CallOptions = {}
): Promise<unknown> {
  const command = await selectedCli();
  const id = ++callSeq;
  const started = Date.now();
  const toolHook = toolHooks.getStore();
  toolHook?.({ id, state: 'start', args });
  return new Promise((fulfill, reject) => {
    const rooted = engineRoots.getStore();
    const child = execFile(
      command[0],
      [...command.slice(1), ...nativeArgs(args)],
      {
        maxBuffer: Infinity,
        timeout,
        ...(rooted && {
          cwd: rooted,
          env: { ...process.env, WORKSPACE_ROOT: rooted },
        }),
      },
      (err, stdout, stderr) => {
        const saved =
          evidenceRoot && join(evidenceRoot, `${randomUUID()}.json`);
        if (saved)
          writeFileSync(
            saved,
            JSON.stringify({ args, stdout, stderr, exitCode: err?.code ?? 0 })
          );
        const r = settle(args, partial, { err, stdout, stderr, saved });
        toolHook?.({
          id,
          state: 'done',
          args,
          ms: Date.now() - started,
          bytes: stdout.length,
          exitCode: err?.code ?? 0,
          ...r,
        });
        if (r.error) reject(new Error(r.error));
        else fulfill(r.value);
      }
    );
    if (onStderr) {
      let rest = '';
      child.stderr?.on('data', chunk => {
        const lines = (rest + chunk).split('\n');
        rest = lines.pop() ?? '';
        for (const line of lines) if (line) onStderr(line);
      });
    }
  });
}

export async function pool<T, R>(
  items: readonly T[],
  size: number,
  fn: (item: T, index: number) => R | Promise<R>
) {
  const out = Array.from<R>({ length: items.length });
  let next = 0;
  await Promise.all(
    Array.from({ length: Math.min(size, items.length) }, async () => {
      while (next < items.length) {
        const i = next++;
        out[i] = await fn(items[i], i);
      }
    })
  );
  return out;
}

// True when `octocode <args>` exits 0 (help text is not JSON, so octocode() cannot be used for it).
export async function cliOk(args: string[]) {
  try {
    const command = await selectedCli();
    execFileSync(command[0], [...command.slice(1), ...nativeArgs(args)], {
      stdio: 'ignore',
    });
    return true;
  } catch {
    return false;
  }
}
