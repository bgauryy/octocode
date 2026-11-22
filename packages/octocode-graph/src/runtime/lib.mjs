// Shared helpers: flag parsing, async octocode CLI calls with a tool-call hook, a small worker pool.
import { AsyncLocalStorage } from 'node:async_hooks';
import { execFile, execFileSync } from 'node:child_process';
import { existsSync, mkdirSync, realpathSync, writeFileSync } from 'node:fs';
import { randomUUID } from 'node:crypto';
import { freezeExecutable } from './lib/pin-cli.mjs';
import { isAbsolute, join, resolve } from 'node:path';

// Directories a graph build never reads; the frozen copy skips the same ones.
export const SKIPPED_DIRS = ['target', 'node_modules', '.git', 'dist', 'build', '.octocode'];

// The top-level folder a root-relative id belongs to; a file at the root belongs to the root, `.`, not to a folder
// named after itself.
export const topGroup = (id) => (id.includes('/') ? id.split('/')[0] : '.');

// Import degrees from production imports: test code depending on a file does not make it a hub.
export function degrees(imports) {
  const fanIn = new Map(), fanOut = new Map();
  for (const e of imports.filter((x) => !x.test)) {
    fanIn.set(e.target, (fanIn.get(e.target) ?? 0) + 1);
    fanOut.set(e.source, (fanOut.get(e.source) ?? 0) + 1);
  }
  return { fanIn, fanOut };
}

export function flags(argv, spec) {
  const out = {};
  for (const [name, dflt] of Object.entries(spec)) {
    const i = argv.indexOf(`--${name}`);
    if (i >= 0 && (argv[i + 1] === undefined || argv[i + 1].startsWith('--'))) {
      console.error(`--${name} needs a value`);
      process.exit(2);
    }
    out[name] = i < 0 ? dflt : argv.splice(i, 2)[1];
  }
  return out;
}

// Resolve lazily so replay and offline queries need no binary.
let cli;
let resolving;
async function selectedCli() {
  if (cli) return cli;
  resolving ??= resolveCli().catch(error => { resolving = undefined; throw error; });
  return cli ??= await resolving;
}

async function resolveCli() {
  const override = process.env.OCTOCODE_CLI;
  if (override) return existsSync(override) ? [override] : override.split(' ');
  const { resolveNativeBin } = await import('octocode/native');
  const result = await resolveNativeBin();
  if (result.error) throw new Error(result.error);
  return [realpathSync(result.bin)];
}

// Ask the selected launcher which executable served the catalog. Read the contract again from
// its private copy: a package install or rebuild can replace the source between discovery and copy.
let contract;
export function prepareGraphRun() {
  contract ??= loadGraphContract().catch((error) => { contract = undefined; throw error; });
  return contract;
}

async function loadGraphContract() {
  const schema = (command, args) => {
    let raw;
    try {
      raw = execFileSync(command[0], [...command.slice(1), ...args], { encoding: 'utf8', maxBuffer: Infinity });
    } catch (error) {
      throw new Error(parseOutput(error.stdout)?.error ?? error.message, { cause: error });
    }
    const value = JSON.parse(raw);
    if (value.error) throw new Error(value.error);
    return value;
  };
  const full = ['--view', 'full'];
  const source = schema(await selectedCli(), ['schema', ...full]).executable;
  if (typeof source !== 'string' || !isAbsolute(source)) throw new Error('The CLI catalog must identify its native executable. Update Octocode and retry.');
  const pinned = [await freezeExecutable(source)];
  const catalog = schema(pinned, ['schema', ...full]);
  const graph = schema(pinned, ['__graph', 'schema', ...full]);
  const limit = (name) => {
    const owner = name === 'graphQuery' ? graph : catalog;
    const max = owner.tools?.find((t) => t.name === name)?.maxQueries;
    if (!Number.isSafeInteger(max) || max < 1) throw new Error(`${name} schema declares no positive queries maximum`);
    return max;
  };
  const targetLimit = (operation) => {
    const query = schema(pinned, ['__graph', 'schema', 'graphQuery', '--select', `operation=${operation}`]).querySchema;
    const dereference = (node) => {
      const seen = new Set();
      while (node?.$ref?.startsWith('#/$defs/') && !seen.has(node)) {
        seen.add(node);
        node = query.$defs?.[node.$ref.slice('#/$defs/'.length)];
      }
      return node;
    };
    const target = dereference(query?.properties?.target);
    const maxTargets = (target?.anyOf ?? [target]).map(dereference).find((s) => s?.type === 'array')?.maxItems;
    if (!Number.isSafeInteger(maxTargets) || maxTargets < 1) throw new Error(`graphQuery ${operation} schema declares no positive target list maximum`);
    return maxTargets;
  };
  if (!Array.isArray(catalog.grammarCapabilities)) throw new Error('The CLI catalog is missing grammarCapabilities');
  const limits = {
    graphBatch: limit('graphQuery'), lspBatch: catalog.tools.some((t) => t.name === 'lspSearch') ? limit('lspSearch') : null,
    maxTargets: targetLimit('unresolved'), fileGroup: Math.min(targetLimit('witnesses'), targetLimit('symbols')),
    extensions: new Set(catalog.grammarCapabilities.filter((g) => g.graphFacts).flatMap((g) => g.extensions)),
  };
  cli = pinned;
  return limits;
}

// The engine can root its answers in a configured ancestor of the CLI caller's cwd.
// Read the effective setting from Octocode so .env, .octocoderc and defaults agree.
let workspace;
export function nativeWorkspaceRoot() {
  workspace ??= octocode(['config', 'get', 'WORKSPACE_ROOT', '--json']).then(setting => {
    if (setting.key !== 'WORKSPACE_ROOT') throw new Error('Octocode did not report its effective workspace root');
    return resolve(setting.value ?? process.cwd());
  }).catch(error => { workspace = undefined; throw error; });
  return workspace;
}

// Ordinary local tools use the research interface of the same pinned engine.
function nativeArgs(args) {
  const name = args[0] === 'schema' ? args[1] : args[0];
  return name === 'graphBuild' || name === 'graphQuery' ? ['__graph', ...args] : args;
}

let evidenceRoot;
export function captureEvidence(directory) {
  mkdirSync(directory, { recursive: true });
  evidenceRoot = directory;
}

// Every CLI call is reported twice (start, done) so a viewer can show each tool, page and cursor live.
const toolHooks = new AsyncLocalStorage();
let callSeq = 0;
export function withToolHook(hook, task) {
  return toolHooks.run(hook, task);
}

// A clasify-style `next.read` is a source read, not missing evidence (TOOL_DATA_CONTRACT "source-read continuations").
const incomplete = (v) => v.kind === 'continue' || v.isPartial === true;
export function requireCompleteToolResponse(value) {
  if (!value || typeof value !== 'object') return;
  if (incomplete(value)) throw new Error('Incomplete tool evidence: follow the saved continuation or rerun a narrower query before judging this layer');
  for (const [key, child] of Object.entries(value)) if (key !== 'related' && key !== 'hints') requireCompleteToolResponse(child);
}

function parseOutput(stdout) {
  try { return JSON.parse(stdout); } catch { return undefined; }
}

// Whether stdout is an answer: exit 0, exit 1 (empty), or any exit status from a batch envelope (`results`).
const answered = (err, stdout) => !err || err.code === 1 || (Number.isInteger(err.code) && Array.isArray(parseOutput(stdout)?.results));

// `octocode` exits 1 on an empty result but still prints one JSON object, and a batch exits with a rejected or
// failed row's code (1 only when every row is empty) while stdout still carries every row: both are answers. `partial: true` callers (graph pages among them) follow
// their own continuations; every other caller gets only complete evidence. No wall-clock or output cap: the runtime owns its
// deadlines. `timeout` (ms) is only for a caller that explicitly asks for one.
// The parsed answer, or the error to reject with (and to report to the tool hook).
function settle(args, partial, { err, stdout, stderr, saved }) {
  const value = answered(err, stdout) ? parseOutput(stdout) : undefined;
  const where = saved ? `; evidence: ${saved}` : '';
  if (!value) return { error: `octocode ${args.slice(0, 2).join(' ')}: ${String(stderr || stdout || err?.message || 'no output')}${where}` };
  try {
    if (!partial) requireCompleteToolResponse(value);
  } catch (error) {
    return { error: `${error.message}${where}` };
  }
  return { value };
}

// `onStderr` receives each stderr line as the call prints it (graphBuild progress), before the call ends.
export async function octocode(args, { timeout = 0, partial = false, onStderr } = {}) {
  const command = await selectedCli();
  const id = ++callSeq;
  const started = Date.now();
  const toolHook = toolHooks.getStore();
  toolHook?.({ id, state: 'start', args });
  return new Promise((fulfill, reject) => {
    const child = execFile(command[0], [...command.slice(1), ...nativeArgs(args)], { maxBuffer: Infinity, timeout }, (err, stdout, stderr) => {
      const saved = evidenceRoot && join(evidenceRoot, `${randomUUID()}.json`);
      if (saved) writeFileSync(saved, JSON.stringify({ args, stdout, stderr, exitCode: err?.code ?? 0 }));
      const r = settle(args, partial, { err, stdout, stderr, saved });
      toolHook?.({ id, state: 'done', args, ms: Date.now() - started, bytes: stdout.length, exitCode: err?.code ?? 0, ...r });
      if (r.error) reject(new Error(r.error));
      else fulfill(r.value);
    });
    if (onStderr) {
      let rest = '';
      child.stderr.on('data', (chunk) => {
        const lines = (rest + chunk).split('\n');
        rest = lines.pop();
        for (const line of lines) if (line) onStderr(line);
      });
    }
  });
}

export async function pool(items, size, fn) {
  const out = Array.from({ length: items.length });
  let next = 0;
  await Promise.all(
    Array.from({ length: Math.min(size, items.length) }, async () => {
      while (next < items.length) {
        const i = next++;
        out[i] = await fn(items[i], i);
      }
    }),
  );
  return out;
}

// True when `octocode <args>` exits 0 (help text is not JSON, so octocode() cannot be used for it).
export async function cliOk(args) {
  try {
    const command = await selectedCli();
    execFileSync(command[0], [...command.slice(1), ...nativeArgs(args)], { stdio: 'ignore' });
    return true;
  } catch {
    return false;
  }
}
