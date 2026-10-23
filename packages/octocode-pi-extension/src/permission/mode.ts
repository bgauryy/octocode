import { envChoice, PERMISSION_ENV, PERMISSION_GRANTS_ENV } from '../shared/env.js';
import { plural } from '../shared/render.js';

/**
 * Permission modes: which agent actions run, which ask the user first. One table decides every class in every mode;
 * the invariants (reservations, the Octocode home, redirects to private addresses, browser attach validation, an
 * untrusted project's shared memory, hook denies) run before it and no mode skips them. `full` allows every class
 * and records each call. Modes are not a sandbox: they decide what Octocode asks about, while the OS user's rights
 * stay what they are.
 */

export const PERMISSION_MODES = ['manual', 'workspace', 'full'] as const;
export type PermissionMode = (typeof PERMISSION_MODES)[number];

export const ACTION_CLASSES = [
  'read.outside',
  'file.workspace',
  'file.outside',
  'shell',
  'shell.destructive',
  'web',
  'browser',
  'browser.control',
  'net.private',
  'upload.outside',
  'mcp.write',
  'mcp.destructive',
  'agent.spawn',
] as const;
export type ActionClass = (typeof ACTION_CLASSES)[number];

type Rule = 'allow' | 'ask';

/**
 * Per class: what manual, workspace and full do, how the class reads in a message, `why` it asks (the reason line every
 * prompt shows), and `all`, the kind a session grant can cover ("Allow all shell commands for this session"). A class
 * without a kind (destructive shell, browser control, destructive MCP) asks on every call.
 */
const MATRIX: Record<ActionClass, { rules: [Rule, Rule, Rule]; noun: string; why: string; all?: string }> = {
  'read.outside': { rules: ['ask', 'allow', 'allow'], noun: 'reading outside the workspace', why: 'it reads files outside the repository', all: 'reads outside the workspace' },
  'file.workspace': { rules: ['ask', 'allow', 'allow'], noun: 'the file change', why: 'it changes files in the repository', all: 'file changes in the repository' },
  'file.outside': { rules: ['ask', 'ask', 'allow'], noun: 'changing a file outside the workspace', why: 'it changes a file outside the repository, or one that runs in later sessions', all: 'file changes outside the workspace' },
  shell: { rules: ['ask', 'allow', 'allow'], noun: 'the shell command', why: 'a shell command can do anything you can', all: 'shell commands' },
  'shell.destructive': { rules: ['ask', 'ask', 'allow'], noun: 'the destructive shell command', why: 'the command matches a common destructive pattern (wipes data, formats a disk, powers off); the check flags common cases and is not a security boundary' },
  web: { rules: ['ask', 'allow', 'allow'], noun: 'the web request', why: 'it sends a request to a website', all: 'web requests' },
  browser: { rules: ['ask', 'allow', 'allow'], noun: 'the browser action', why: 'it acts in a real browser', all: 'browser actions' },
  'browser.control': { rules: ['ask', 'ask', 'allow'], noun: 'taking control of the browser', why: 'it takes control of a browser: attaching to one, running script in a page or uploading files' },
  'net.private': { rules: ['ask', 'ask', 'allow'], noun: 'opening a private or local address', why: 'the address is private or local, so it can reach internal services', all: 'private or local addresses' },
  'upload.outside': { rules: ['ask', 'ask', 'allow'], noun: 'uploading files from outside the workspace', why: 'it sends files from outside the repository to a page', all: 'uploads from outside the workspace' },
  'mcp.write': { rules: ['ask', 'allow', 'allow'], noun: 'the MCP tool call', why: 'an MCP tool may change something outside this session', all: 'MCP tool calls' },
  'mcp.destructive': { rules: ['ask', 'ask', 'allow'], noun: 'the destructive MCP tool call', why: 'the MCP tool declares itself destructive' },
  'agent.spawn': { rules: ['ask', 'allow', 'allow'], noun: 'starting a subagent', why: 'a subagent acts on its own with tools of its own', all: 'subagents' },
};

/** Why `action` asks, as the prompt's reason line says it. */
export const actionWhy = (action: ActionClass): string => MATRIX[action].why;

/**
 * Reasons tighten a call. They are not action classes and never a session grant. A provenance hit, a hook ask and a
 * profile that declares permissions ask in manual and workspace; full runs them and records the reason. A reason forces
 * one ask on the call's effect. It does not replace that effect, and a session grant of the effect does not answer it.
 */
export const ASK_REASONS = ['hook.ask', 'provenance', 'profile.permissions'] as const;
export type AskReason = (typeof ASK_REASONS)[number];

const REASON_RULES: Record<AskReason, [Rule, Rule, Rule]> = {
  'hook.ask': ['ask', 'ask', 'allow'],
  provenance: ['ask', 'ask', 'allow'],
  'profile.permissions': ['ask', 'ask', 'allow'],
};

/** Whether `reason` forces an ask in `mode`. */
export function reasonAsks(mode: PermissionMode, reason: AskReason): boolean {
  return REASON_RULES[reason][PERMISSION_MODES.indexOf(mode)] === 'ask';
}

/** The least permissive mode in which none of `reasons` ask: full. */
export function modeAllowingReasons(reasons: readonly AskReason[]): PermissionMode | undefined {
  return PERMISSION_MODES.find((mode) => reasons.every((reason) => !reasonAsks(mode, reason)));
}

/** The kind a session-wide grant of `action` covers ("shell commands"); undefined for the per-call tripwires. */
export const kindOf = (action: ActionClass): string | undefined => MATRIX[action].all;

export function decide(mode: PermissionMode, action: ActionClass): Rule {
  return MATRIX[action].rules[PERMISSION_MODES.indexOf(mode)]!;
}

/** How `action` reads in a message ("the shell command"). */
export const actionNoun = (action: ActionClass): string => MATRIX[action].noun;

/** The least permissive mode that runs `action` without asking. */
export const modeAllowing = (action: ActionClass): PermissionMode | undefined => PERMISSION_MODES.find((mode) => decide(mode, action) === 'allow');

/** Asks that a session grant may never answer: their content differs every time, or the hint says it destroys data. */
export const UNGRANTABLE: ReadonlySet<ActionClass> = new Set(ACTION_CLASSES.filter((action) => kindOf(action) === undefined));

/** Classes a profile's `permissions:` front matter may declare: what its child needs and the mode would otherwise ask about. */
export const PROFILE_DECLARABLE: readonly ActionClass[] = ['browser.control', 'net.private', 'upload.outside'];

/**
 * Classes a parent may hand a subagent, which cannot ask anyone: every class with a session kind (what the user allowed
 * for the session), plus the profile-declarable ones. Destructive shell and MCP never pass down.
 */
export const SPAWN_GRANTABLE: ReadonlySet<ActionClass> = new Set([...ACTION_CLASSES.filter((action) => kindOf(action) !== undefined), ...PROFILE_DECLARABLE]);

/** What a profile's `permissions:` front matter declares: the allowlisted classes, and the names left out (not declarable, or not a name at all). */
export function declaredPermissions(value: unknown): { declared: ActionClass[]; ignored: string[] } {
  const names = (Array.isArray(value) ? value : typeof value === 'string' ? value.split(',') : value === undefined ? [] : [value]).map((name) => (typeof name === 'string' ? name.trim() : String(name))).filter(Boolean);
  const declared = new Set<ActionClass>();
  const ignored: string[] = [];
  for (const name of names) {
    if (PROFILE_DECLARABLE.includes(name as ActionClass)) declared.add(name as ActionClass);
    else ignored.push(name);
  }
  return { declared: [...declared], ignored };
}

/** One line per mode for `/octocode permission`: what it asks about. */
export function asksIn(mode: PermissionMode): ActionClass[] {
  return ACTION_CLASSES.filter((action) => decide(mode, action) === 'ask');
}

/** What a mode means, in one sentence: the model note and `/octocode permission` say it. */
export function modeSummary(mode: PermissionMode): string {
  if (mode === 'full') return 'nothing asks: destructive commands (rm -rf ~), private web fetches, browser control and a hook\'s ask all run without a prompt. Safeguards: the ⚠ counter, warning-colored activity lines and the audit log (/octocode permission log); a run with no UI has only the log. Invariants and hook denies still refuse.';
  return `it asks before ${asksIn(mode).join(', ')}; hook asks, provenance hits and profile permissions also ask.`;
}

/**
 * The mode a session starts in: `full` (no routine asks), unless the user exported `OCTOCODE_PERMISSION`. An invalid
 * value starts in `manual` with a warning (stricter, never looser).
 */
export function startMode(env: NodeJS.ProcessEnv): { mode: PermissionMode; warning?: string } {
  const chosen = envChoice(env, PERMISSION_ENV, PERMISSION_MODES);
  if (!chosen) return { mode: 'full' };
  if ('invalid' in chosen) return { mode: 'manual', warning: `${PERMISSION_ENV}=${chosen.invalid} is not manual, workspace or full; starting in manual.` };
  return { mode: chosen.value };
}

/**
 * The classes a parent handed this subagent: its spawn grant (workspace classes its profile reaches and the classes its
 * profile declared) and every kind the user allowed for the parent's session. A name outside `SPAWN_GRANTABLE` (an ask
 * reason, destructive shell, an unknown name) is dropped, whatever the variable says.
 */
export function parseGrants(env: NodeJS.ProcessEnv): Set<ActionClass> {
  const names = (env[PERMISSION_GRANTS_ENV] ?? '').split(',').map((name) => name.trim());
  return new Set(names.filter((name): name is ActionClass => SPAWN_GRANTABLE.has(name as ActionClass)));
}

/** The footer chip's status key (`ui/footer` places it first after the model), the audit entries' type and the mode notes' message type. */
export const PERMISSION_ID = 'octocode-permission';

/** The editor-scoped key that cycles the mode; native selectors retain their own input handling. */
export const PERMISSION_SHORTCUT = 'shift+tab';
export const PERMISSION_SHORTCUT_LABEL = 'Shift+Tab';

/** The order the editor shortcut cycles modes in, from the default (no routine asks) to the strictest. */
export const PERMISSION_CYCLE: readonly PermissionMode[] = ['full', 'workspace', 'manual'];

/** The chip per mode: its text, its short form for a narrow footer, and its theme colour. */
export const CHIPS: Record<PermissionMode, { text: string; short: string; color: 'accent' | 'muted' | 'error' }> = {
  manual: { text: `⏸ permission: manual · ${PERMISSION_SHORTCUT_LABEL}`, short: '⏸ manual', color: 'accent' },
  workspace: { text: `permission: workspace · ${PERMISSION_SHORTCUT_LABEL}`, short: 'workspace', color: 'muted' },
  full: { text: `⏵⏵ permission: full · ${PERMISSION_SHORTCUT_LABEL}`, short: '⏵⏵ full', color: 'error' },
};

/** What full mode did this session: calls it allowed, and how many of them were ⚠ (destructive, browser control, private network). */
export interface ActionCount {
  actions: number;
  warnings: number;
}

/** The chip text of `mode`; in full, once a call ran, it counts them: `⏵⏵ permission: full · 12 actions · 2 ⚠ · Shift+Tab`. */
export function chipText(mode: PermissionMode, count?: ActionCount): string {
  if (mode !== 'full' || !count || count.actions === 0) return CHIPS[mode].text;
  const warned = count.warnings > 0 ? ` · ${count.warnings} ⚠` : '';
  return `⏵⏵ permission: full · ${plural(count.actions, 'action')}${warned} · ${PERMISSION_SHORTCUT_LABEL}`;
}

const FULL_CHIP = /⏵⏵ permission: full( · \d+ actions?(?: · \d+ ⚠)?)? · Shift\+Tab/;

/** A chip status (coloured or not) with its text replaced by the short form; a full chip keeps its counters. */
export function shortChip(status: string): string {
  const full = FULL_CHIP.exec(status);
  if (full) return status.replace(FULL_CHIP, `${CHIPS.full.short}${full[1] ?? ''}`);
  for (const chip of [CHIPS.manual, CHIPS.workspace]) if (status.includes(chip.text)) return status.replace(chip.text, chip.short);
  return status;
}
