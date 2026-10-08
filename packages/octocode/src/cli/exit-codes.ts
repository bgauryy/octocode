/**
 * The exit codes the Node-owned commands emit. They follow the native exit
 * table (`octocode --help`), which owns the remaining codes.
 */
export const EXIT = {
  OK: 0,
  /** A skill operation failed. */
  GENERAL: 1,
  USAGE: 2,
  /** The native runtime is unavailable or failed. */
  TOOL: 5,
} as const;

/** The CLI-wide JSON error envelope, the native CLI's error shape. */
export function toolErrorJson(error: string): string {
  return JSON.stringify({ kind: 'octocode.toolError', version: 1, error });
}
