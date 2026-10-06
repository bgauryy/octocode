import fs from 'node:fs';
import path from 'node:path';
import { getAgentDir, hasTrustRequiringProjectResources, ProjectTrustStore, type ExtensionCommandContext, type ExtensionContext } from '@earendil-works/pi-coding-agent';
import { atomicWriteFileSync, sha256 } from './atomic.js';
import type { Subcommands } from './commands.js';
import { sanitizeTerminalText } from './sanitize.js';
import { globalPaths, projectAgentDirs, projectHookFiles, projectSkillDirs, workspacePaths } from './home.js';
import { isRecord, parseJson } from './util.js';
import { envFlag, SUBAGENT_ENV, TRUST_ROOT_ENV } from './env.js';

/**
 * Project files that start commands or steer the agent (Claude Code / Codex / Octocode hooks, `.pi/agents`, `.octocode/agents`, `.claude/skills`, `.octocode/skills`) load only after an explicit trust decision. `ctx.isProjectTrusted()` alone is not one: Pi (checked
 * against 0.x `hasTrustRequiringProjectResources`) reports a project trusted when it holds none of *Pi's* protected
 * resources (`.pi/settings.json`, `.pi/extensions`, `.agents/skills`, …), so a clone with only a `.claude/settings.json`
 * would run its hooks without ever asking. A project is trusted here when the user accepted these exact files: the decision is
 * stored in `<Octocode home>/pi-trust.json` with a SHA-256 of their content, so a later change needs `/octocode trust`
 * again. When Pi really resolved trust (it has protected resources, or a saved `/trust` decision) and Octocode has no
 * decision yet, the files present then are recorded as trusted; a later change is asked about like any other.
 */

type ProjectContext = Pick<ExtensionContext, 'cwd' | 'hasUI' | 'isProjectTrusted'> & { ui?: Pick<ExtensionContext['ui'], 'notify'> };

interface StoredDecision {
  sha: string;
  trusted: boolean;
}

/** Files listed without commands (agents, skills) past this many are summarized; command lines are never cut. */
const MAX_PLAIN_FILES = 40;

/** Every existing trust-gated project file, sorted: the same paths the hooks, profile and skill loaders read. */
export function projectConfigFiles(cwd: string): string[] {
  const files = [
    ...projectHookFiles(cwd),
    ...projectAgentDirs(cwd).flatMap((dir) => listDir(dir).filter((file) => file.endsWith('.md'))),
    ...projectSkillDirs(cwd).flatMap((dir) => skillFiles(dir, true, new Set())),
  ];
  // Real paths: the fingerprint hashes them, and a symlinked spelling of the same project must not ask again.
  return [...new Set(files.filter(isFile).map(realPath))].sort();
}

/**
 * SHA-256 over the paths and bytes of `files`: any added, removed or edited file changes it. `as` names each file
 * by another path (a worktree's file as its parent repository's).
 */
export function fingerprint(files: string[], as: (file: string) => string = (file) => file): string {
  return sha256(files.map((file) => `${as(file)}\0${readText(file)}`).join('\0\0'));
}

/**
 * The skill files Pi's loader (0.x `loadSkillsFromDir`) would read from `dir`: a directory holding `SKILL.md` is one
 * skill (that file, no recursion); otherwise the root's direct `.md` files and, at any depth, subdirectories' `SKILL.md`.
 * Dot entries and `node_modules` are skipped like Pi does. Pi's ignore files are not applied, so this is a superset.
 */
function skillFiles(dir: string, root: boolean, seen: Set<string>): string[] {
  const real = realPath(dir);
  if (seen.has(real)) return [];
  seen.add(real);
  let entries: string[];
  try {
    entries = fs.readdirSync(dir);
  } catch {
    return [];
  }
  const skill = path.join(dir, 'SKILL.md');
  if (entries.includes('SKILL.md') && isFile(skill)) return [skill];
  return entries.flatMap((name) => {
    if (name.startsWith('.') || name === 'node_modules') return [];
    const full = path.join(dir, name);
    const stat = fs.statSync(full, { throwIfNoEntry: false });
    if (stat?.isDirectory()) return skillFiles(full, false, seen);
    return root && stat?.isFile() && name.endsWith('.md') ? [full] : [];
  });
}

/** One dialog line: escape sequences stripped and newlines folded, so a value cannot draw or fake another line. */
const oneLine = (text: string): string => sanitizeTerminalText(text).replace(/\s*\n\s*/g, ' ⏎ ');

/**
 * What trusting would run: each file, with every `command`/`args`/`url` found in JSON configs (never cut) and the
 * environment variables their `headers`/`env` pass on. Files without commands past a cap are summarized.
 */
export function describeProjectConfig(cwd: string, files = projectConfigFiles(cwd)): string {
  const lines: string[] = [];
  let plain = 0;
  for (const file of files) {
    const shown = oneLine(path.relative(realPath(cwd), file) || file);
    const runs = file.endsWith('.json') ? commandsIn(parseJson(readText(file))) : [];
    if (runs.length === 0 && ++plain > MAX_PLAIN_FILES) continue;
    lines.push(`- ${shown}${runs.length ? ':' : ''}`, ...runs.map((run) => `    ${oneLine(run)}`));
  }
  if (plain > MAX_PLAIN_FILES) lines.push(`… ${plain - MAX_PLAIN_FILES} more file(s) without commands`);
  return lines.join('\n');
}

const ENV_REFERENCE = /\$\{([A-Za-z_][A-Za-z0-9_]*)(?::-[^}]*)?\}/g;

function commandsIn(value: unknown, out: string[] = []): string[] {
  if (Array.isArray(value)) for (const item of value) commandsIn(item, out);
  else if (isRecord(value)) {
    if (typeof value['command'] === 'string') out.push([value['command'], ...(Array.isArray(value['args']) ? value['args'].map(String) : [])].join(' '));
    if (typeof value['url'] === 'string') out.push(value['url']);
    const sends = envSent(value);
    if (sends) out.push(sends);
    for (const [key, child] of Object.entries(value)) if (key !== 'args' && key !== 'headers' && key !== 'env' && typeof child === 'object') commandsIn(child, out);
  }
  return out;
}

/** `  sends $A, $B (headers, env)` for the `${VAR}` references Octocode expands in a server's headers and env. */
function envSent(server: Record<string, unknown>): string | undefined {
  const names = new Set<string>();
  const where: string[] = [];
  for (const key of ['headers', 'env'] as const) {
    const block = server[key];
    if (!isRecord(block)) continue;
    const before = names.size;
    for (const text of Object.values(block)) for (const match of String(text).matchAll(ENV_REFERENCE)) names.add(match[1]!);
    if (names.size > before) where.push(key);
  }
  return names.size > 0 ? `  sends your environment variables ${[...names].map((name) => `$${name}`).join(', ')} (in ${where.join(', ')})` : undefined;
}

/**
 * Answers of `projectTrustNow` for a few seconds: a session start asks from four places (skills, hooks,
 * profiles, memory), and each answer re-reads and re-hashes the project's config files. A decision clears it.
 */
const TRUST_MEMO_MS = 5_000;
const trustMemo = new Map<string, { at: number; value: boolean | undefined }>();

/**
 * The trust decision when it needs no question: `true`/`false`, or `undefined` when the user must be asked (project
 * files exist, Pi did not decline, and no stored decision matches their current content).
 */
export function projectTrustNow(ctx: Pick<ProjectContext, 'cwd' | 'isProjectTrusted'>, home?: string): boolean | undefined {
  const files = projectConfigFiles(ctx.cwd);
  // Keyed by the files' size and mtime too (a stat, not a read), so an edited or added config file is never missed.
  const stamp = files.map((file) => {
    const stat = fs.statSync(file, { throwIfNoEntry: false });
    return `${file}:${stat?.size}:${stat?.mtimeMs}`;
  });
  // Pi's own decision (its `/trust`) and our store file (written by any process) are part of the key too.
  const store = fs.statSync(storePath(home), { throwIfNoEntry: false })?.mtimeMs;
  const memoKey = `${ctx.cwd}\0${process.env[TRUST_ROOT_ENV] ?? ''}\0${process.env[SUBAGENT_ENV] ?? ''}\0${home ?? ''}\0${ctx.isProjectTrusted()}\0${piDecided(ctx.cwd)}\0${store}\0${stamp.join('|')}`;
  const now = Date.now();
  const memo = trustMemo.get(memoKey);
  if (memo && now - memo.at < TRUST_MEMO_MS) return memo.value;
  for (const [key, entry] of trustMemo) if (now - entry.at >= TRUST_MEMO_MS) trustMemo.delete(key);
  const value = computeTrust(ctx, home);
  trustMemo.set(memoKey, { at: now, value });
  return value;
}

function computeTrust(ctx: Pick<ProjectContext, 'cwd' | 'isProjectTrusted'>, home?: string): boolean | undefined {
  const files = projectConfigFiles(ctx.cwd);
  // An isolated subagent works in a worktree of its parent's repository: it follows the decision stored for that
  // repository, and only for the same files (no one can be asked in a subagent, so anything else stays closed).
  const inherited = process.env[TRUST_ROOT_ENV];
  if (inherited && path.isAbsolute(inherited)) {
    const parent = realPath(inherited);
    const root = storeKey(ctx.cwd);
    if (!envFlag(process.env, SUBAGENT_ENV) || !isWorktreeOf(root, parent)) return files.length === 0;
    const stored = readStore(home)[parent];
    return files.length === 0 || (stored?.trusted === true && stored.sha === fingerprint(files, (file) => path.join(parent, path.relative(root, file))));
  }
  const sha = fingerprint(files);
  const trusted = ctx.isProjectTrusted();
  if (!trusted && files.length > 0) return false;
  const stored = trusted ? readStore(home)[storeKey(ctx.cwd)] : undefined;
  if (stored?.sha === sha) return stored.trusted;
  // Pi's trust covers the files present now (even none); recording them makes a later change, such as a pulled
  // hooks file, ask again.
  if (!stored && trusted && piDecided(ctx.cwd)) {
    saveDecision(ctx.cwd, sha, true, home);
    return true;
  }
  return files.length === 0 ? true : undefined;
}

/** Pi resolved trust itself: protected resources exist, or `/trust` saved a decision. */
function piDecided(cwd: string): boolean {
  try {
    return hasTrustRequiringProjectResources(cwd) || new ProjectTrustStore(getAgentDir()).get(cwd) === true;
  } catch {
    return false;
  }
}

/** Projects (by path and content) already told they are skipped, so every loader shares one notice. */
const noticed = new Set<string>();

/**
 * Whether trust-gated project files may load. An undecided project is skipped with one notice pointing at
 * `/octocode trust`, which shows exactly what would run and asks. Startup never opens a dialog: Pi (checked against
 * 0.x) ends an RPC session when a dialog is answered inside `session_start`, and print, JSON and subagent runs have
 * no one to ask.
 */
export async function projectTrust(ctx: ProjectContext, home?: string): Promise<boolean> {
  const now = projectTrustNow(ctx, home);
  if (now !== undefined) return now;
  const files = projectConfigFiles(ctx.cwd);
  const key = `${storeKey(ctx.cwd)}\0${fingerprint(files)}`;
  if (!noticed.has(key)) {
    noticed.add(key);
    ctx.ui?.notify?.(`Project config not loaded (${files.length} files). Run /octocode trust to review.`, 'warning');
  }
  return false;
}

/** Record a decision for the project files whose `fingerprint` is `sha` (the ones the user was shown). */
export function saveDecision(cwd: string, sha: string, trusted: boolean, home?: string): void {
  trustMemo.clear();
  const store = readStore(home);
  store[storeKey(cwd)] = { sha, trusted };
  const file = storePath(home);
  fs.mkdirSync(path.dirname(file), { recursive: true });
  atomicWriteFileSync(file, `${JSON.stringify(store, null, 2)}\n`, { mode: 0o600 });
}

/** `/octocode trust [off]`: show what the project would run and trust (or distrust) exactly that, then reload. */
export function registerTrustCommand(commands: Subcommands, home?: string): void {
  commands.add('trust', {
    description: 'trust [off] — load (or stop loading) this project\'s hooks, agents and skills',
    handler: async (args, ctx: ExtensionCommandContext) => {
      let files = projectConfigFiles(ctx.cwd);
      if (files.length === 0) return ctx.ui.notify('This project has no hook, agent or skill config to trust.', 'info');
      const off = args.trim() === 'off';
      if (!off && !ctx.isProjectTrusted()) return ctx.ui.notify('Pi does not trust this project; run /trust first.', 'warning');
      // The decision covers exactly what was shown: fingerprinted before the dialog and re-checked after it.
      let sha = fingerprint(files);
      let details = describeProjectConfig(ctx.cwd, files);
      for (let attempt = 1; !off && ctx.hasUI; attempt++) {
        const yes = await ctx.ui.confirm('Trust this project\'s config?', `${details}\n\nLoad and run these? You'll be asked again if they change.`);
        if (!yes) return ctx.ui.notify('Project config stays unloaded.', 'info');
        files = projectConfigFiles(ctx.cwd);
        const now = fingerprint(files);
        if (now === sha) break;
        if (attempt >= MAX_TRUST_ASKS) return ctx.ui.notify('Project config kept changing while you reviewed it; it stays unloaded. Run /octocode trust again.', 'warning');
        ctx.ui.notify('Project config changed while you reviewed it; review it again.', 'warning');
        sha = now;
        details = describeProjectConfig(ctx.cwd, files);
      }
      saveDecision(ctx.cwd, sha, !off, home);
      ctx.ui.notify(`${off ? 'Distrusted' : 'Trusted'} ${files.length} project files. Reloading…`, 'info');
      await ctx.reload();
    },
  });
}

/** Dialogs shown before giving up on config that keeps changing under review. */
const MAX_TRUST_ASKS = 3;

const storePath = (home?: string) => globalPaths(process.env, home).trust;

/** One decision per repository: the gated files hang from its root, whichever subfolder Pi starts in. */
function storeKey(cwd: string): string {
  return realPath(workspacePaths(cwd).root);
}

/** `root` is a linked worktree of the repository at `parent`: its `.git` file points into `<parent>/.git/worktrees/`. */
function isWorktreeOf(root: string, parent: string): boolean {
  const gitdir = /^gitdir:\s*(.+)$/m.exec(readText(path.join(root, '.git')))?.[1]?.trim();
  if (!gitdir) return false;
  const relative = path.relative(path.join(parent, '.git', 'worktrees'), realPath(path.resolve(root, gitdir)));
  return relative !== '' && !relative.startsWith('..') && !path.isAbsolute(relative);
}

function realPath(file: string): string {
  try {
    return fs.realpathSync(file);
  } catch {
    return path.resolve(file);
  }
}

function readStore(home?: string): Record<string, StoredDecision> {
  const parsed = parseJson(readText(storePath(home)));
  if (!isRecord(parsed)) return {};
  return Object.fromEntries(
    Object.entries(parsed).flatMap(([key, value]) => (isRecord(value) && typeof value['sha'] === 'string' && typeof value['trusted'] === 'boolean' ? [[key, { sha: value['sha'], trusted: value['trusted'] }]] : [])),
  );
}

function readText(file: string): string {
  try {
    return fs.readFileSync(file, 'utf8');
  } catch {
    return '';
  }
}

function listDir(dir: string): string[] {
  try {
    return fs.readdirSync(dir).map((name) => path.join(dir, name));
  } catch {
    return [];
  }
}

function isFile(file: string): boolean {
  try {
    return fs.statSync(file).isFile();
  } catch {
    return false;
  }
}
