// Shared parts of the benchmark harness: the round manifest, paths, seeded randomness, a job pool,
// transcript reading and scratch cleanup. See README.md in this folder.
import { spawn } from 'node:child_process';
import { createHash } from 'node:crypto';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { CONFIG_FIELDS } from '@octocodeai/config';

export const HARNESS = path.dirname(fileURLToPath(import.meta.url));
export const BENCH = path.resolve(HARNESS, '..');
export const REPO = path.resolve(BENCH, '..', '..');
export const CLI = path.join(REPO, 'packages', 'octocode', 'out', 'octocode.js');

export class HarnessError extends Error {}

export const isRecord = value => Boolean(value) && typeof value === 'object' && !Array.isArray(value);

/** Length in Unicode code points (METHOD.md counts characters, not UTF-16 units or bytes). */
export function codePoints(text) {
  let count = text.length;
  for (let i = 0; i < text.length; i += 1) {
    const unit = text.charCodeAt(i);
    if (unit >= 0xd800 && unit <= 0xdbff && i + 1 < text.length) {
      const next = text.charCodeAt(i + 1);
      if (next >= 0xdc00 && next <= 0xdfff) {
        count -= 1;
        i += 1;
      }
    }
  }
  return count;
}

/** Repo-relative, `~`-relative or absolute → absolute. */
export function resolvePath(value, base = REPO) {
  if (value === '~' || value.startsWith('~/')) return path.join(os.homedir(), value.slice(1));
  return path.resolve(base, value);
}

/** "Q1-Q30", "Q1,Q3", "all" or a list → question IDs (`all` needs the set's own IDs). */
export function expandIds(spec, all) {
  if (Array.isArray(spec)) return spec.flatMap(item => expandIds(item, all));
  if (spec === 'all') {
    if (!all) throw new HarnessError('"all" is only valid for an arm of a set');
    return [...all];
  }
  return String(spec).split(',').flatMap(part => {
    const range = /^([A-Za-z]+)(\d+)-\1?(\d+)$/.exec(part.trim());
    if (!range) return [part.trim()];
    const [, prefix, from, to] = range;
    if (Number(to) < Number(from)) throw new HarnessError(`empty ID range ${part}`);
    return Array.from({ length: Number(to) - Number(from) + 1 }, (_, i) => `${prefix}${Number(from) + i}`);
  });
}

const need = (condition, message) => {
  if (!condition) throw new HarnessError(`round manifest: ${message}`);
};

/**
 * Reads and checks a round manifest (README.md#round-manifest). Returns it with paths resolved and
 * sets expanded: sets[name] = {dir, ids[], arms: {arm: Set(ids)}}.
 */
export function loadManifest(file) {
  const source = resolvePath(file, process.cwd());
  let raw;
  try {
    raw = JSON.parse(fs.readFileSync(source, 'utf8'));
  } catch (error) {
    throw new HarnessError(`cannot read round manifest ${source}: ${error.message}`);
  }
  return checkManifest(raw, source);
}

export function checkManifest(raw, source = '<manifest>') {
  need(isRecord(raw), 'must be a JSON object');
  need(typeof raw.round === 'string' && /^[\w.+-]+$/.test(raw.round), '"round" must be a name of letters, digits, . + - _');
  const passes = raw.passes ?? 1;
  need(Number.isSafeInteger(passes) && passes >= 1, '"passes" must be a positive integer');
  const jobs = raw.jobs ?? 1;
  need(Number.isSafeInteger(jobs) && jobs >= 1, '"jobs" must be a positive integer');
  need(isRecord(raw.arms) && Object.keys(raw.arms).length, '"arms" must name at least one arm');
  need(isRecord(raw.sets) && Object.keys(raw.sets).length, '"sets" must name at least one question set');
  const passDir = raw.passDir ?? 'pass-{n}';
  need(passes === 1 || passDir.includes('{n}'), '"passDir" needs {n} when there are several passes');

  const pins = {};
  for (const [name, pin] of Object.entries(raw.pins ?? {})) {
    need(isRecord(pin) && typeof pin.path === 'string' && /^[0-9a-f]{64}$/.test(pin.sha256 ?? ''), `pin "${name}" needs "path" and a 64-hex "sha256"`);
    pins[name] = { path: resolvePath(pin.path), sha256: pin.sha256 };
  }
  const arms = {};
  for (const [name, arm] of Object.entries(raw.arms)) {
    need(/^[\w.+-]+$/.test(name), `arm name "${name}" may hold letters, digits, . + - _`);
    need(isRecord(arm) && ['mcp', 'shell'].includes(arm.kind), `arm "${name}" needs "kind": "mcp" or "shell"`);
    need(typeof arm.instruction === 'string' && arm.instruction, `arm "${name}" needs the "instruction" line appended to each question`);
    if (arm.kind === 'mcp') need(isRecord(arm.server) && typeof arm.server.command === 'string', `arm "${name}" needs "server": {command, args, env}`);
    if (arm.pin !== undefined) need(pins[arm.pin], `arm "${name}" names pin "${arm.pin}", which "pins" does not define`);
    arms[name] = { filters: [], ...arm, name };
  }
  const sets = {};
  for (const [name, set] of Object.entries(raw.sets)) {
    need(/^[\w.+-]+$/.test(name), `set name "${name}" may hold letters, digits, . + - _`);
    need(isRecord(set) && typeof set.questions === 'string' && set.ids !== undefined && isRecord(set.arms), `set "${name}" needs "questions", "ids" and "arms"`);
    const ids = expandIds(set.ids);
    const setArms = {};
    for (const [arm, spec] of Object.entries(set.arms)) {
      need(arms[arm], `set "${name}" uses arm "${arm}", which "arms" does not define`);
      const armIds = expandIds(spec, ids);
      need(armIds.every(id => ids.includes(id)), `set "${name}" arm "${arm}" lists an ID outside the set`);
      setArms[arm] = new Set(armIds);
    }
    sets[name] = { name, dir: resolvePath(set.questions), ids, arms: setArms };
  }
  const groups = {};
  for (const [name, group] of Object.entries(raw.groups ?? Object.fromEntries(Object.keys(sets).map(set => [set, { set }])))) {
    need(isRecord(group) && sets[group.set], `group "${name}" needs "set" naming a question set`);
    groups[name] = { set: group.set, ids: group.ids === undefined ? sets[group.set].ids : expandIds(group.ids, sets[group.set].ids) };
  }
  const compare = raw.compare ?? [];
  need(Array.isArray(compare) && compare.every(pair => Array.isArray(pair) && pair.length === 2 && pair.every(arm => arms[arm])), '"compare" must list [arm, otherArm] pairs of defined arms');
  need(typeof raw.neutralCwd === 'string', '"neutralCwd" must name the empty directory sessions start in');
  const campaign = resolvePath(raw.campaign ?? path.join('packages', 'octocode-benchmark', 'campaigns', raw.round));
  return {
    source,
    round: raw.round,
    host: raw.host ?? 'claude',
    passes,
    jobs,
    seed: raw.seed ?? raw.round,
    campaign,
    passDir,
    scratch: path.join(REPO, '.octocode', 'tmp', raw.round),
    neutralCwd: resolvePath(raw.neutralCwd),
    clones: resolvePath(raw.clones ?? path.join('packages', 'octocode-benchmark', 'local', 'repos')),
    models: { runner: raw.models?.runner, judge: raw.models?.judge },
    timeouts: { runSeconds: raw.timeouts?.runSeconds ?? 900, judgeSeconds: raw.timeouts?.judgeSeconds ?? 1500 },
    judge: { arms: raw.judge?.arms, brief: resolvePath(raw.judge?.brief ?? path.join('packages', 'octocode-benchmark', 'briefs', 'judge.md')) },
    pins,
    arms,
    sets,
    groups,
    compare,
  };
}

export const passPath = (manifest, pass) => path.join(manifest.campaign, manifest.passDir.replaceAll('{n}', String(pass)));
export const runName = (set, id, arm) => `${set}-${id}-${arm}`;
export const runPath = (manifest, pass, set, id, arm) => path.join(passPath(manifest, pass), 'runs', runName(set, id, arm));

/** Every (set, id) of the manifest in set order, with the arms that answer it. */
export function questions(manifest) {
  const out = [];
  for (const set of Object.values(manifest.sets)) {
    for (const id of set.ids) {
      const arms = Object.keys(set.arms).filter(arm => set.arms[arm].has(id));
      out.push({ set: set.name, id, name: `${set.name}-${id}`, file: path.join(set.dir, `${id}.md`), key: path.join(set.dir, 'answers', `${id}.md`), arms });
    }
  }
  return out;
}

/** The question as a runner or judge sees it: everything up to the end of the `## Question` section. */
export function questionText(file, { clones, withHead = true }) {
  const text = fs.readFileSync(file, 'utf8').replaceAll('packages/octocode-benchmark/local/repos', clones);
  const at = text.indexOf('## Question');
  const head = at === -1 ? text : text.slice(0, at);
  const rest = at === -1 ? '' : text.slice(at + '## Question'.length);
  const body = rest.split(/\n## /)[0];
  return `${`${withHead ? head : ''}## Question${body}`.trimEnd()}\n`;
}

/** Fills {repo}, {bench}, {clones}, {scratch}, {home}, {pin} and {pin:<name>} in a manifest string. */
export function fill(value, manifest, { home, pin } = {}) {
  if (Array.isArray(value)) return value.map(item => fill(item, manifest, { home, pin }));
  if (isRecord(value)) return Object.fromEntries(Object.entries(value).map(([key, item]) => [key, fill(item, manifest, { home, pin })]));
  if (typeof value !== 'string') return value;
  return value.replace(/\{(repo|bench|clones|scratch|home|pin)(?::([\w.+-]+))?\}/g, (whole, key, name) => {
    if (key === 'repo') return REPO;
    if (key === 'bench') return BENCH;
    if (key === 'clones') return manifest.clones;
    if (key === 'scratch') return manifest.scratch;
    if (key === 'home') {
      if (!home) throw new HarnessError('{home} is used where no session home exists');
      return home;
    }
    const chosen = name ? manifest.pins[name]?.path : pin;
    if (!chosen) throw new HarnessError(`${whole} names no pinned binary`);
    return chosen;
  });
}

export function sha256File(file) {
  return createHash('sha256').update(fs.readFileSync(file)).digest('hex');
}

/** Fails when a pinned binary is missing or is not the file the manifest recorded. */
export function checkPins(manifest, names = Object.keys(manifest.pins)) {
  for (const name of names) {
    const pin = manifest.pins[name];
    if (!fs.existsSync(pin.path)) throw new HarnessError(`pin "${name}": ${pin.path} does not exist`);
    const actual = sha256File(pin.path);
    if (actual !== pin.sha256) throw new HarnessError(`pin "${name}": ${pin.path} has SHA-256 ${actual}, the manifest records ${pin.sha256}`);
  }
}

/** A deterministic generator: the same seed gives the same shuffle and the same bootstrap interval. */
export function seeded(seed) {
  let state = createHash('sha256').update(String(seed)).digest().readUInt32LE(0);
  return () => {
    state = (state + 0x6d2b79f5) | 0;
    let t = Math.imul(state ^ (state >>> 15), 1 | state);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

export function shuffled(items, random) {
  const out = [...items];
  for (let i = out.length - 1; i > 0; i -= 1) {
    const j = Math.floor(random() * (i + 1));
    [out[i], out[j]] = [out[j], out[i]];
  }
  return out;
}

/** Runs `work(item)` for every item, `jobs` at a time, starting them in order; calls `done` as each ends. */
export async function pool(items, jobs, work, done = () => {}) {
  let next = 0;
  const results = Array.from({ length: items.length });
  const worker = async () => {
    while (next < items.length) {
      const index = next;
      next += 1;
      results[index] = await work(items[index], index);
      done(results[index], index);
    }
  };
  await Promise.all(Array.from({ length: Math.min(jobs, items.length) }, worker));
  return results;
}

/** Runs a child to its end with a deadline; resolves {code, stdout, stderr, timedOut}. Never rejects. */
export function runChild(command, args, { cwd, env, timeoutMs, input } = {}) {
  return new Promise(resolve => {
    const child = spawn(command, args, { cwd, env, stdio: [input === undefined ? 'ignore' : 'pipe', 'pipe', 'pipe'] });
    const out = [];
    const err = [];
    let timedOut = false;
    const timer = timeoutMs
      ? setTimeout(() => {
          timedOut = true;
          child.kill('SIGKILL');
        }, timeoutMs)
      : undefined;
    child.stdout.on('data', chunk => out.push(chunk));
    child.stderr.on('data', chunk => err.push(chunk));
    if (input !== undefined) child.stdin.end(input);
    const finish = (code, failure) => {
      clearTimeout(timer);
      resolve({ code: timedOut ? 124 : (code ?? 1), stdout: Buffer.concat(out).toString('utf8'), stderr: failure ?? Buffer.concat(err).toString('utf8'), timedOut });
    };
    child.on('error', error => finish(127, `failed to start ${command}: ${error.message}`));
    child.on('close', code => finish(code));
  });
}

/** Removes a directory tree, including read-only ones (a Go module cache is written without write bits). */
export function removeTree(dir) {
  if (!fs.existsSync(dir)) return;
  try {
    fs.rmSync(dir, { recursive: true, force: true });
    return;
  } catch (error) {
    if (!['EACCES', 'EPERM', 'ENOTEMPTY'].includes(error.code)) throw error;
  }
  const unlock = at => {
    const stat = fs.lstatSync(at);
    if (!stat.isDirectory()) return;
    fs.chmodSync(at, 0o700);
    for (const entry of fs.readdirSync(at)) unlock(path.join(at, entry));
  };
  unlock(dir);
  fs.rmSync(dir, { recursive: true, force: true });
}

/** Removes `dir` and then each parent up to (not including) `stop` while it is empty. */
export function removeTreeAndEmptyParents(dir, stop) {
  removeTree(dir);
  let at = path.dirname(dir);
  while (at.startsWith(`${stop}${path.sep}`) || at === stop) {
    try {
      fs.rmdirSync(at);
    } catch {
      return; // not empty (another run's scratch), or already gone
    }
    if (at === stop) return;
    at = path.dirname(at);
  }
}

/** The environment a session starts with: the caller's, minus anything that configures Octocode, gh or the host. */
export function sessionEnv(base = process.env) {
  // Every setting's variable (their earlier `OCTOCODE_*` names fall under the prefixes below).
  const settings = CONFIG_FIELDS.flatMap(field => (field.env ? [field.env.name] : []));
  const dropped = new Set([...settings, 'NODE_ENV', 'RUN_ID']);
  const prefixes = ['OCTOCODE', 'GH_', 'GITHUB_', 'CLAUDECODE', 'CLAUDE_CODE_', 'BENCH_GH_'];
  return Object.fromEntries(Object.entries(base).filter(([key]) => !dropped.has(key) && !prefixes.some(prefix => key.startsWith(prefix))));
}

/** The first `gh` on PATH that is not this harness's wrapper. */
export function realGh(env = process.env) {
  for (const dir of (env.PATH ?? '').split(path.delimiter)) {
    const candidate = path.join(dir, 'gh');
    try {
      fs.accessSync(candidate, fs.constants.X_OK);
      if (fs.realpathSync(candidate) !== path.join(HARNESS, 'gh-readonly')) return candidate;
    } catch {
      // not here
    }
  }
  throw new HarnessError('gh is not on PATH');
}

/** Puts the read-only wrapper on a PATH directory of its own as `gh`; returns that directory. */
export function wrapperBin(manifest) {
  const dir = path.join(manifest.scratch, 'bin');
  fs.mkdirSync(dir, { recursive: true });
  const link = path.join(dir, 'gh');
  fs.rmSync(link, { force: true });
  fs.symlinkSync(path.join(HARNESS, 'gh-readonly'), link);
  return dir;
}

export function textOf(content) {
  if (typeof content === 'string') return content;
  return (content ?? []).map(part => (isRecord(part) && typeof part.text === 'string' ? part.text : '')).join('');
}

/**
 * Reads a stream-json transcript into ordered model steps. A step is one model request: its usage, the blocks
 * it wrote, and the tool results that came back before the next request.
 * Returns {tools, steps: [{id, usage, blocks: [{type, name, input, id, chars}], results: [{id, text, isError}]}]}.
 */
export function readTranscript(file) {
  const steps = [];
  const byId = new Map();
  let tools;
  for (const line of fs.readFileSync(file, 'utf8').split('\n')) {
    let event;
    try {
      event = JSON.parse(line);
    } catch {
      continue;
    }
    if (event?.type === 'system' && event.subtype === 'init') tools = event.tools;
    if (event?.type === 'assistant' && isRecord(event.message)) {
      const { id, usage = {}, content = [] } = event.message;
      let step = byId.get(id);
      if (!step) {
        step = { id, usage, blocks: [], results: [] };
        byId.set(id, step);
        steps.push(step);
      }
      for (const block of content) {
        if (block.type === 'tool_use') step.blocks.push({ type: 'tool_use', id: block.id, name: block.name, input: block.input, chars: codePoints(JSON.stringify(block.input ?? {})) });
        else step.blocks.push({ type: block.type, chars: codePoints(block.text ?? block.thinking ?? '') });
      }
    }
    if (event?.type === 'user' && Array.isArray(event.message?.content)) {
      for (const block of event.message.content) {
        if (isRecord(block) && block.type === 'tool_result' && steps.length) {
          steps.at(-1).results.push({ id: block.tool_use_id, text: textOf(block.content), isError: Boolean(block.is_error) });
        }
      }
    }
  }
  return { tools, steps };
}

export function parseArgs(argv, { flags = [], values = [] }) {
  const options = {};
  const positional = [];
  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (flags.includes(arg)) options[arg.slice(2)] = true;
    else if (values.includes(arg)) {
      if (argv[i + 1] === undefined) throw new HarnessError(`${arg} needs a value`);
      options[arg.slice(2)] = argv[++i];
    } else if (arg.startsWith('--')) throw new HarnessError(`unknown option ${arg} (see --help)`);
    else positional.push(arg);
  }
  return { options, positional };
}

/** Runs a command's main; a HarnessError prints its message and exits 2. */
export async function cli(main, help) {
  const argv = process.argv.slice(2);
  if (argv.includes('--help') || argv.includes('-h')) {
    console.log(help);
    return;
  }
  try {
    process.exitCode = (await main(argv)) ?? 0;
  } catch (error) {
    if (!(error instanceof HarnessError)) throw error;
    console.error(error.message);
    process.exitCode = 2;
  }
}

export const isMain = url => Boolean(process.argv[1]) && fs.realpathSync(process.argv[1]) === fileURLToPath(url);
