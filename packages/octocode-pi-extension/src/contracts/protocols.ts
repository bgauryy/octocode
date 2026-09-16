/** Prompt composition order shared by every Octocode host adapter. */
export const PROMPT_MODES = ['append', 'octocode-first'] as const;
export type PromptMode = (typeof PROMPT_MODES)[number];
export const DEFAULT_OCTOCODE_PROMPT_MODE: PromptMode = 'octocode-first';

/** Stable identifiers for sensitive-action approval classes. */
export const APPROVAL_CLASSES = [
  'install',
  'git-write',
  'fs-delete',
  'sudo',
  'publish',
  'system',
  'infra',
] as const;
export type ApprovalClass = (typeof APPROVAL_CLASSES)[number];

/** Session-local approval policies shared by terminal and web settings. */
export const PERMISSION_LEVELS = ['strict', 'default', 'relaxed'] as const;
export type PermissionLevel = (typeof PERMISSION_LEVELS)[number];

