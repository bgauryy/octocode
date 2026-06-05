// Typed exit codes for the agent-facing surface (tool execution + dispatch).
// Agents branch on the exit code instead of parsing stderr text. Management
// commands (install/auth/etc.) currently use OK/GENERAL only.
export const EXIT = {
  OK: 0,
  GENERAL: 1, // unspecified failure
  USAGE: 2, // bad flags / invalid or unparseable input
  NOT_FOUND: 3, // unknown tool or command
  AUTH: 4, // authentication / authorization failure
  TOOL: 5, // tool/API execution error
  RATE_LIMIT: 7, // rate limited / quota exhausted
} as const;

export type ExitCode = (typeof EXIT)[keyof typeof EXIT];

// Best-effort classification of a failed tool result into a typed exit code,
// based on the rendered error text. Defaults to TOOL when nothing matches.
export function classifyToolErrorText(text: string): ExitCode {
  if (/\b(rate[ _-]?limit|429|quota)\b/i.test(text)) {
    return EXIT.RATE_LIMIT;
  }
  if (
    /\b(401|403|unauthor|forbidden|authentication|bad credentials)\b/i.test(
      text
    )
  ) {
    return EXIT.AUTH;
  }
  return EXIT.TOOL;
}
