/**
 * Approval gate — context-aware, per-session consent for sensitive actions.
 *
 * Protected action classes: installs, mutating git, file deletion, sudo,
 * publishing (packages/releases/images), local system-state changes
 * (processes/services/schedulers), and cloud/infra mutation. Instead of
 * assuming consent, we ask the user through an interactive prompt:
 *
 *   • Yes (once)              → approve this one action.
 *   • No                      → decline; the caller must not proceed.
 *   • Always allow (session)  → approve and remember this *class* of action for
 *                               the rest of the session, so we never re-prompt it.
 *
 * A session-scoped permission LEVEL tunes the gate (`/octocode-permissions`):
 * strict re-prompts everything (no memory), default is the flow above, relaxed
 * auto-approves install/git while still prompting for deletes / sudo / publish /
 * system / infra. OCTOCODE_PERMISSION_LEVEL pins the starting level.
 *
 * All decisions live in module-level session state cleared on `session_start`
 * (see resetApprovalStore) — nothing persists to disk. Non-interactive hosts
 * (rpc / json / print) cannot prompt, so the gate denies and tells the agent to
 * confirm inline before retrying — it never silently proceeds.
 */

import type { PiContext } from '../types.js';
import {
  PERMISSION_LEVELS,
  type ApprovalClass,
  type PermissionLevel,
} from '@octocodeai/agent-contracts/protocols';
import {
  APPROVAL_CHOICE_ALWAYS,
  APPROVAL_CHOICE_NO,
  APPROVAL_CHOICE_YES,
  APPROVAL_TITLE_SHELL_PERSISTENCE,
  APPROVAL_TITLES,
} from '../tui/content.js';

/**
 * Classes auto-approved under `relaxed` — routine local-dev actions only.
 * Deliberately EXCLUDES fs-delete: path-guard bounds writes, not deletions, so
 * an unprompted delete under relaxed would be the one unguarded destructive
 * path (e.g. `rm -rf ~/anything` inside the allowed roots). Deletes always ask.
 */
const RELAXED_AUTO_CLASSES: ReadonlySet<ApprovalClass> = new Set(['install', 'git-write']);

export interface ApprovalRequest {
  /** Which class of action this is — the key remembered by "always allow". */
  actionClass: ApprovalClass;
  /** Short human title for the prompt, e.g. "Run git command". */
  title: string;
  /** The concrete detail shown to the user (the command, package, etc.). */
  detail: string;
}

export interface ApprovalOutcome {
  approved: boolean;
  /** True when approval came from a remembered "always allow" decision. */
  remembered: boolean;
  /** True when the user chose "always allow" during this prompt. */
  always: boolean;
  /** False when the host could not prompt (non-interactive). */
  interactive: boolean;
}

/** Per-session set of action classes the user chose to "always allow". */
const alwaysAllowed = new Set<ApprovalClass>();

/** The session's permission level. In-memory only, reset on session_start. */
let permissionLevel: PermissionLevel = 'default';

/** Clear all remembered approvals AND reset the level. Called on session_start. */
export function resetApprovalStore(): void {
  alwaysAllowed.clear();
  permissionLevel = 'default';
}

/** Whether a class has a remembered "always allow" for this session. */
export function isAlwaysAllowed(cls: ApprovalClass): boolean {
  return alwaysAllowed.has(cls);
}

/** Remember an "always allow" decision for a class (session-scoped). */
export function allowAlways(cls: ApprovalClass): void {
  alwaysAllowed.add(cls);
}

/** Drop one remembered class (it will prompt again). */
export function revokeAlways(cls: ApprovalClass): void {
  alwaysAllowed.delete(cls);
}

/** Snapshot of remembered classes — for status/session-state display. */
export function approvedClasses(): ApprovalClass[] {
  return [...alwaysAllowed];
}

export function getPermissionLevel(): PermissionLevel {
  return permissionLevel;
}

export function setPermissionLevel(level: PermissionLevel): void {
  permissionLevel = level;
}

/** Parse a user-supplied level name; undefined for anything unrecognized. */
export function parsePermissionLevel(value: string | undefined): PermissionLevel | undefined {
  const normalized = (value ?? '').trim().toLowerCase();
  return (PERMISSION_LEVELS as readonly string[]).includes(normalized)
    ? (normalized as PermissionLevel)
    : undefined;
}

/**
 * Apply a startup permission level from the environment (OCTOCODE_PERMISSION_LEVEL).
 * Called on session_start AFTER resetApprovalStore, so an operator/CI can pin a
 * session's starting level without touching the in-session controls.
 */
export function applyStartupPermissionLevel(env: NodeJS.ProcessEnv = process.env): void {
  const level = parsePermissionLevel(env['OCTOCODE_PERMISSION_LEVEL']);
  if (level) permissionLevel = level;
}

/**
 * Cycle default → relaxed → strict → default (the shift+tab order: the most
 * common mid-session wish from default is "stop prompting me", so relaxed
 * comes first — mirroring Claude Code's default → acceptEdits direction).
 */
export function cyclePermissionLevel(): PermissionLevel {
  const order: readonly PermissionLevel[] = ['default', 'relaxed', 'strict'];
  permissionLevel = order[(order.indexOf(permissionLevel) + 1) % order.length]!;
  return permissionLevel;
}


/** Git subcommands that mutate history, refs, the worktree, or remotes. */
const MUTATING_GIT_SUBCOMMANDS = new Set([
  'commit', 'push', 'reset', 'rebase', 'checkout', 'switch', 'merge', 'cherry-pick',
  'revert', 'clean', 'restore', 'rm', 'mv', 'am', 'apply', 'stash', 'tag', 'branch',
  'filter-branch', 'filter-repo', 'gc', 'prune', 'reflog', 'remote', 'update-ref',
  'fetch', 'pull', 'clone', 'init', 'add', 'config',
]);

/** Package/tool install commands. `pkg add`/`install` etc. */
const INSTALL_RE =
  /\b(?:npm|pnpm|yarn|bun)\s+(?:add|install|i|global\s+add|dlx)\b|\b(?:pip3?|pipx)\s+install\b|\b(?:brew|apt|apt-get|dnf|yum|pacman|zypper|apk)\s+(?:install|add)\b|\b(?:cargo|go|gem)\s+install\b|\bgo\s+get\b/;

/** curl|wget piped into a shell — remote-code install pattern. */
const PIPE_TO_SHELL_RE = /\b(?:curl|wget)\b[^|]*\|\s*(?:sudo\s+)?(?:bash|sh|zsh)\b/;

/** Octocode's own dogfood CLIs run constantly — never prompt for them. */
function isOctocodeDogfoodInstall(command: string): boolean {
  return /\bnpx\s+(?:-y\s+|--yes\s+)?(?:@octocodeai\/|octocode\b|octocode-mcp\b)/.test(command);
}

/**
 * Classify a shell command into a sensitive action class, or null when it needs
 * no approval. Best-effort static scan — designed to fail *open* only for the
 * ordinary read/build commands, and to catch the clearly-sensitive ones.
 */
export function classifySensitiveCommand(command: string): ApprovalRequest | null {
  const cmd = command.trim();

  // sudo — highest priority; privilege escalation of any kind.
  if (/(^|[;|&(`\n])\s*sudo\b/.test(cmd)) {
    return { actionClass: 'sudo', title: APPROVAL_TITLES.sudo!, detail: cmd };
  }

  // Installs — checked per shell segment so that an Octocode dogfood segment (e.g.
  // `npx octocode`) cannot exempt a separate install or pipe-to-shell segment in the
  // same command (e.g. `npx octocode; npm install evil` or `npx octocode && curl x | sh`).
  for (const seg of cmd.split(/[;\n]|\s*&&\s*|\s*\|\|\s*/).map((s) => s.trim()).filter(Boolean)) {
    if ((INSTALL_RE.test(seg) || PIPE_TO_SHELL_RE.test(seg)) && !isOctocodeDogfoodInstall(seg)) {
      return { actionClass: 'install', title: APPROVAL_TITLES.install!, detail: cmd };
    }
  }

  // Outward publication — packages, releases, images. Irreversible-ish and
  // world-visible, so it gets its own class rather than riding on `install`.
  if (
    /\b(?:npm|pnpm|yarn)\s+publish\b|\bcargo\s+publish\b|\bgem\s+push\b|\btwine\s+upload\b|\bdocker\s+push\b|\bgh\s+release\s+(?:create|upload|edit|delete)\b/.test(cmd)
  ) {
    return { actionClass: 'publish', title: APPROVAL_TITLES.publish!, detail: cmd };
  }

  // Cloud / infra mutation and remote-resource destruction.
  if (
    /\bterraform\s+(?:apply|destroy)\b|\bkubectl\s+(?:delete|drain)\b|\bdocker\s+(?:system\s+prune|volume\s+(?:rm|prune))\b|\bgh\s+repo\s+(?:delete|archive)\b|\baws\s+s3\s+(?:rm|rb)\b|\bgcloud\s+\S+.*\bdelete\b/.test(cmd)
  ) {
    return { actionClass: 'infra', title: APPROVAL_TITLES.infra!, detail: cmd };
  }

  // Local system state — processes, services, schedulers, recursive ownership.
  if (
    /(^|[;|&(`\n])\s*(?:kill|pkill|killall)\b/.test(cmd) ||
    /\b(?:systemctl|launchctl|service)\s+(?:stop|start|restart|disable|enable|unload|load|kickstart)\b/.test(cmd) ||
    /\bcrontab\b/.test(cmd) ||
    /\b(?:chmod|chown)\s+(?:-[^\s]*R[^\s]*|--recursive)\b/.test(cmd) ||
    /\bdefaults\s+write\b|\bdiskutil\b/.test(cmd)
  ) {
    return { actionClass: 'system', title: APPROVAL_TITLES.system!, detail: cmd };
  }

  // Shell-startup persistence: redirecting/appending into rc/profile files
  // survives the session and runs on every future shell — a classic
  // persistence vector (Claude Code guards these paths as dangerous files).
  if (
    /(?:>>?|\btee\s+(?:-a\s+)?)[^;|&\n]*(?:\.(?:bashrc|zshrc|bash_profile|profile|zshenv|zprofile|zlogin)\b|\/etc\/(?:profile|bashrc|zshenv|zprofile))/.test(cmd)
  ) {
    return { actionClass: 'system', title: APPROVAL_TITLE_SHELL_PERSISTENCE, detail: cmd };
  }

  // Mutating git — scan each command segment for `git <subcommand>`.
  // The pre-subcommand flag run must consume git's global options that take a
  // SEPARATE argument (`git -C <path> push`, `git -c core.hooksPath=x commit`,
  // `git --git-dir <dir> reset`) — otherwise the path/config token stops the
  // scan and the subcommand is never classified, silently skipping approval for
  // destructive `-C`/`-c`-prefixed git (reset --hard, clean, push).
  for (const seg of cmd.split(/[;|&\n]+/)) {
    const m = /\bgit\s+(?:(?:-[Cc]|--git-dir|--work-tree|--namespace|--exec-path|--super-prefix|--config-env)\s+\S+\s+|-[^\s]+\s+)*([a-z][a-z-]*)/.exec(seg.trim());
    if (m && MUTATING_GIT_SUBCOMMANDS.has(m[1]!)) {
      return { actionClass: 'git-write', title: `Run sensitive git command (git ${m[1]})`, detail: seg.trim() };
    }
  }

  // File removal — direct commands, find -delete/-exec rm, xargs rm, and
  // secure-delete/unlink variants (gemini-cli marks the same find flags unsafe).
  if (
    /(^|[;|&(`\n])\s*(?:rm|rmdir|shred|unlink|trash)\s+/.test(cmd) ||
    /\bfind\b[^;|&\n]*\s-(?:delete|exec(?:dir)?\s[^;|&\n]*\brm\b)/.test(cmd) ||
    /\bxargs\b[^;|&\n]*\brm\b/.test(cmd)
  ) {
    return { actionClass: 'fs-delete', title: APPROVAL_TITLES['fs-delete']!, detail: cmd };
  }

  return null;
}

function canPrompt(ctx?: PiContext): boolean {
  return Boolean(ctx?.hasUI && typeof ctx.ui?.select === 'function');
}

const YES = APPROVAL_CHOICE_YES;
const NO = APPROVAL_CHOICE_NO;
const ALWAYS = APPROVAL_CHOICE_ALWAYS;

/**
 * Request approval for a sensitive action, honoring the session permission
 * level:
 * - relaxed: routine local-dev classes auto-approve without a prompt.
 * - default: remembered classes auto-approve; otherwise Yes / No / Always.
 * - strict: always prompts (memory ignored, Always not offered).
 */
export async function requestApproval(
  ctx: PiContext | undefined,
  request: ApprovalRequest,
): Promise<ApprovalOutcome> {
  const level = getPermissionLevel();
  if (level === 'relaxed' && RELAXED_AUTO_CLASSES.has(request.actionClass)) {
    return { approved: true, remembered: true, always: false, interactive: true };
  }
  if (level !== 'strict' && isAlwaysAllowed(request.actionClass)) {
    return { approved: true, remembered: true, always: false, interactive: true };
  }
  if (!canPrompt(ctx)) {
    return { approved: false, remembered: false, always: false, interactive: false };
  }

  const prompt = request.detail ? `${request.title}\n${request.detail}` : request.title;
  const choices = level === 'strict' ? [YES, NO] : [YES, NO, ALWAYS];
  const choice = await ctx!.ui!.select!(prompt, choices);

  if (choice === ALWAYS) {
    allowAlways(request.actionClass);
    // Immediate feedback: what was remembered and how to undo it — a silent
    // session-wide grant is the one consent state the user must not lose track of.
    ctx?.ui?.notify?.(
      `Always-allow remembered for "${request.actionClass}" this session — /octocode-permissions revoke ${request.actionClass} to undo.`,
      'info',
    );
    return { approved: true, remembered: false, always: true, interactive: true };
  }
  if (choice === YES) {
    return { approved: true, remembered: false, always: false, interactive: true };
  }
  // No, or dismissed (undefined) → decline.
  return { approved: false, remembered: false, always: false, interactive: true };
}
