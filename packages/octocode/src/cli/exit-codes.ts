export const EXIT = {
  OK: 0,
  GENERAL: 1,
  USAGE: 2,
  NOT_FOUND: 3,
  AUTH: 4,
  TOOL: 5,
  // Partial result: the response carries a re-runnable next.* continuation.
  // The native runtime emits this; kept here so the TS exit-code table matches
  // the documented contract (see native cli/mod.rs "EXIT CODES").
  PARTIAL: 6,
  RATE_LIMIT: 7,
} as const;

export type ExitCode = (typeof EXIT)[keyof typeof EXIT];
