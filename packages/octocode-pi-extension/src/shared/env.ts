/** Environment switches: one spelling rule for every `OCTOCODE_*` flag and number. */

/** Set in a `visibleBrowser` profile's child (webLive): `browser` opens a visible Chrome on a persistent profile. */
export const BROWSER_VISIBLE_ENV = 'OCTOCODE_BROWSER_VISIBLE';

/** `1` runs Claude Code / Codex command hooks (off by default; subagents run them only when their profile opts in). */
export const HOOKS_ENV = 'OCTOCODE_HOOKS';

/** `0` keeps the Octocode MCP server unregistered (set for a profile with `mcp: false`, e.g. the browser subagents). */
export const MCP_ENV = 'OCTOCODE_MCP';

/** `1` declares every Octocode MCP tool directly; by default its GitHub and package registry tools wait for `tool_search`. */
export const MCP_DIRECT_ENV = 'OCTOCODE_MCP_DIRECT';

/** `0` turns automatic memory injection off for this process (`/octocode memory auto off` turns it off everywhere). */
export const MEMORY_AUTO_ENV = 'OCTOCODE_MEMORY_AUTO';

/** Days kept output and bash logs survive in a kept session folder (default 30); `0` turns the session sweep off. */
export const CLEANUP_DAYS_ENV = 'OCTOCODE_CLEANUP_DAYS';

/** Set in child processes so a subagent cannot spawn further subagents (and skips the root session's extras). */
export const SUBAGENT_ENV = 'OCTOCODE_SUBAGENT';

/** Set for an isolated subagent: the parent's repository root, whose stored trust decision the worktree follows. */
export const TRUST_ROOT_ENV = 'OCTOCODE_TRUST_ROOT';

/** `off`, `unfocused` (default: only while the terminal is in the background) or `always`: answer-ready notifications. */
export const NOTIFY_ENV = 'OCTOCODE_NOTIFY';

/** `osc9`, `osc777`, `osc99` or `bel`; unset picks one from the terminal's variables. */
export const NOTIFY_METHOD_ENV = 'OCTOCODE_NOTIFY_METHOD';

/** `0` turns the extra skill directories off everywhere. */
export const EXTRA_SKILLS_ENV = 'OCTOCODE_EXTRA_SKILLS';

const TRUE = /^(1|true|on|yes)$/i;
const FALSE = /^(0|false|off|no)$/i;

/** `1|true|on|yes` → true, `0|false|off|no` → false (any case, surrounding space ignored); unset or anything else → `fallback`. */
export function envFlag(env: NodeJS.ProcessEnv, name: string, fallback = false): boolean {
  const value = env[name]?.trim() ?? '';
  if (TRUE.test(value)) return true;
  if (FALSE.test(value)) return false;
  return fallback;
}

/** A whole number from the environment, clamped to `[min, max]`; unset or not an integer → `fallback`. */
export function envInt(env: NodeJS.ProcessEnv, name: string, fallback: number, bounds: { min?: number; max?: number } = {}): number {
  const value = env[name]?.trim() ?? '';
  if (!/^[+-]?\d+$/.test(value)) return fallback;
  const parsed = Number.parseInt(value, 10);
  return Math.min(bounds.max ?? Number.POSITIVE_INFINITY, Math.max(bounds.min ?? Number.NEGATIVE_INFINITY, parsed));
}
