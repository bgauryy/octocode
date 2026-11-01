// Shared helpers: flag parsing, async octocode CLI calls with a tool-call hook, a small worker pool.
import { execFile, execFileSync } from 'node:child_process';
import { existsSync, mkdirSync, realpathSync, writeFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { randomUUID } from 'node:crypto';
import { join, resolve } from 'node:path';

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

// The local launcher only forwards arguments to its platform's native binary, and its Node startup costs about
// 40 ms per call. Inside the checkout the scripts ask the launcher's own resolver
// (`packages/octocode-native/bin/resolve-binary.cjs`) for that binary and run it directly. `OCTOCODE_NATIVE_BIN`,
// or a binary the resolver cannot find, keeps the launcher, so its override policy and download fallback decide.
export function localCli(root, env = process.env) {
  const launcher = join(root, 'packages', 'octocode', 'out', 'octocode.js');
  const via = [process.execPath, launcher];
  if (env.OCTOCODE_NATIVE_BIN?.trim()) return via;
  try {
    const resolver = createRequire(import.meta.url)(join(root, 'packages', 'octocode-native', 'bin', 'resolve-binary.cjs'));
    // The real path: the warm `octocode serve` daemon is keyed by binary path, so a symlinked path would start a
    // second daemon with cold language servers (measured: LSP median 1.8 s -> 6.2 s).
    return [realpathSync(resolver.resolveNativeBinaryPath())];
  } catch {
    return via;
  }
}
// Repo rule: the local build inside the checkout (its launcher, or that launcher's verified binary); OCTOCODE_CLI
// overrides; otherwise `octocode` on PATH.
const cli = process.env.OCTOCODE_CLI
  ? process.env.OCTOCODE_CLI.split(' ')
  : existsSync(resolve('packages/octocode/out/octocode.js')) ? localCli(resolve('.')) : ['octocode'];
export const cliCommand = () => [...cli];

let evidenceRoot;
export function captureEvidence(directory) {
  mkdirSync(directory, { recursive: true });
  evidenceRoot = directory;
}

// Every CLI call is reported twice (start, done) so a viewer can show each tool, page and cursor live.
let toolHook = () => {};
let callSeq = 0;
export function onToolCall(fn) {
  toolHook = fn;
}

// A clasify-style `next.read` is a source read, not missing evidence (TOOL_DATA_CONTRACT "source-read continuations").
const incomplete = (v) => v.kind === 'continue' || v.isPartial === true || v.hasMore === true;
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

// `octocode` exits 1 on an empty result but still prints one JSON object, and a batch exits with its worst row
// while stdout still carries every row: both are answers. `partial: true` callers (graph pages among them) follow
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
export function octocode(args, { timeout = 0, partial = false, onStderr } = {}) {
  const id = ++callSeq;
  const started = Date.now();
  toolHook({ id, state: 'start', args });
  return new Promise((fulfill, reject) => {
    const child = execFile(cli[0], [...cli.slice(1), ...args], { maxBuffer: Infinity, timeout }, (err, stdout, stderr) => {
      const saved = evidenceRoot && join(evidenceRoot, `${randomUUID()}.json`);
      if (saved) writeFileSync(saved, JSON.stringify({ args, stdout, stderr, exitCode: err?.code ?? 0 }));
      const r = settle(args, partial, { err, stdout, stderr, saved });
      toolHook({ id, state: 'done', args, ms: Date.now() - started, bytes: stdout.length, exitCode: err?.code ?? 0, ...r });
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
export function cliOk(args) {
  try {
    execFileSync(cli[0], [...cli.slice(1), ...args], { stdio: 'ignore' });
    return true;
  } catch {
    return false;
  }
}
