/**
 * content — the single source of stable user-facing COPY for the Octocode TUI.
 *
 * Rules:
 *   - Strings only: no logic, no colors, no layout (palette.ts owns design
 *     constants; this file owns the words a user reads).
 *   - Every module that shows one of these strings imports it from here — never
 *     restates it inline, so wording
 *     can be edited (or one day translated) in one place.
 *   - Dynamic sentences (counts, names, paths interpolated at runtime) stay at
 *     their call sites; only stable copy lives here.
 */

import type { ApprovalClass, PermissionLevel } from '../tools/approval.js';

// ─── Brand ─────────────────────────────────────────────────────────────────────

export const TAGLINE = 'Your AI coding agent';

/** Beta notice shown under the banner; the URL is the issue tracker. */
export const BETA_LABEL = 'BETA VERSION';
export const BETA_ISSUES_PREFIX = 'for issues:';
export const BETA_ISSUES_URL = 'https://github.com/bgauryy/octocode/issues';

/** Word shown in the live working line while a turn is active. */
export const WORKING_WORD = 'Thinking';

// ─── Approval gate ─────────────────────────────────────────────────────────────

export const APPROVAL_CHOICE_YES = 'Yes (run once)';
export const APPROVAL_CHOICE_NO = 'No, do not run';
export const APPROVAL_CHOICE_ALWAYS = 'Always allow this session';

/** Prompt titles for the statically-titled approval classes (git-write builds its title from the subcommand). */
export const APPROVAL_TITLES: Partial<Record<ApprovalClass, string>> = {
  sudo: 'Run command with sudo (elevated privileges)',
  install: 'Install packages / tools',
  publish: 'Publish package / release / image',
  infra: 'Mutate cloud / infra resources',
  system: 'Change system / process state',
  'fs-delete': 'Delete files / directories',
};

/** Title for shell-startup persistence writes (a `system`-class trigger with its own wording). */
export const APPROVAL_TITLE_SHELL_PERSISTENCE = 'Modify shell startup files (persistence)';

/** One-line meaning per permission level, shared by the cycle notify and status output. */
export const PERMISSION_LEVEL_SUMMARY: Record<PermissionLevel, string> = {
  strict: 'prompt for every sensitive action, no session memory',
  default: 'prompt once per class; "always allow" remembered this session',
  relaxed: 'auto-approve install/git; still prompt deletes/sudo/publish/system/infra',
};

// ─── Widget chrome ─────────────────────────────────────────────────────────────

/** Framed ask-user header label (rendered as `╭─ ◆ <label> ─…`). */
export const ASK_HEADER_LABEL = 'Input needed';

export const OVERLAY_HELP_SELECT = '↑↓ navigate • enter select • esc cancel';
export const OVERLAY_HELP_SELECT_FILTER = '↑↓ navigate • type to filter • enter select • esc cancel';
export const OVERLAY_HELP_MULTI = '↑↓ navigate • space toggle • enter confirm • esc cancel';

// ─── Plan approval ─────────────────────────────────────────────────────────────

export const PLAN_APPROVE_LABEL = 'Approve plan';
export const PLAN_APPROVE_DESC = 'begin executing the steps';
export const PLAN_REJECT_LABEL = 'Reject plan';
export const PLAN_REJECT_DESC = 'do not execute';
/** Free-text row doubles as the adjust channel; the question advertises it. */
export const PLAN_PROPOSE_HINT = 'type feedback to request changes';

// ─── Context-specific ask-widget header labels for plan prompts ───────────────

/** Ask-widget header for the RFC-backed propose review surface picker. */
export const PLAN_RFC_REVIEW_HEADER = 'RFC Ready for Review';
/** Ask-widget header for the plan approve/reject flow. */
export const PLAN_APPROVAL_HEADER = 'Plan Approval';

// ─── Footer legend ─────────────────────────────────────────────────────────────

/**
 * Plain-language meaning of every footer segment, for `/octocode-footer legend`
 * — the footer vocabulary is terse by design, so the decoder ring lives one
 * command away instead of cluttering the toolbar itself.
 */
export const FOOTER_LEGEND: ReadonlyArray<readonly [segment: string, meaning: string]> = [
  ['context ▓▓░░ 25% · 250k/1M', 'context-window fill: gauge, percent, tokens used / window size'],
  ['turn 8 · 14s', 'live: the turn now running and its elapsed time'],
  ['turns 7 · last 12s', 'idle: completed turns and the previous turn duration'],
  ['session 1h 2m', 'session uptime'],
  ['agents 3 (2 live)', 'spawned workers: tracked total and live count; per-agent rows below show each worker activity'],
  ['mail 2', 'unread peer messages for this agent — open /octocode-inbox'],
  ['blocked 1 / failed 1', 'blocked / failed workers (bold) — inspect via /octocode-agents'],
  ['dial deep', 'effort-dial preset (/octocode-dial)'],
  ['perm default|relaxed|strict +2', 'approval-gate mode (always shown; +N = always-allowed classes) — /octocode-permissions'],
  ['prompt ~12k', 'estimated per-turn harness prompt overhead (system + MCP + skills)'],
  ['main (5 changed)', 'git branch and the number of changed files in the working tree'],
  ['keys shift+tab think · ctrl+shift+a perm · ctrl+l model · …', 'second row: individually coloured shortcut keycaps and action words (hidden at compact density)'],
];
