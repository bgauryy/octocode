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

import { type ApprovalClass } from '../contracts/protocols.js';

// ─── Brand ─────────────────────────────────────────────────────────────────────

export const TAGLINE = 'Your AI coding agent';

/** Beta notice shown under the banner; the URL is the issue tracker. */
export const BETA_LABEL = 'BETA VERSION';
export const BETA_ISSUES_PREFIX = 'for issues:';
export const BETA_ISSUES_URL = 'https://github.com/bgauryy/octocode/issues';

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

// ─── Widget chrome ─────────────────────────────────────────────────────────────

/** Framed ask-user header label (rendered as `╭─ ◆ <label> ─…`). */
export const ASK_HEADER_LABEL = 'Input needed';

export const OVERLAY_HELP_SELECT = '↑↓ navigate • enter select • esc cancel';
export const OVERLAY_HELP_SELECT_FILTER = '↑↓ navigate • type to filter • enter select • esc cancel';
export const OVERLAY_HELP_MULTI = '↑↓ navigate • space toggle • enter confirm • esc cancel';

// ─── Plan approval ─────────────────────────────────────────────────────────────

export const PLAN_APPROVE_LABEL = 'Start implementation';
export const PLAN_APPROVE_DESC = 'approve this plan and begin the first runnable step';
export const PLAN_REJECT_LABEL = 'Request changes';
export const PLAN_REJECT_DESC = 'return the plan to draft for revision';
/** Free-text row doubles as the adjust channel; the question advertises it. */
export const PLAN_PROPOSE_HINT = 'type feedback to request changes';

// ─── Context-specific ask-widget header labels for plan prompts ───────────────

/** Ask-widget header for the RFC-backed propose review surface picker. */
export const PLAN_RFC_REVIEW_HEADER = 'Plan Ready to Start';
/** Ask-widget header for the plan approve/reject flow. */
export const PLAN_APPROVAL_HEADER = 'Plan Approval';
