export const EXIT = {
  OK: 0,
  GENERAL: 1,
  USAGE: 2,
  NOT_FOUND: 3,
  AUTH: 4,
  TOOL: 5,
  RATE_LIMIT: 7,
} as const;

export type ExitCode = (typeof EXIT)[keyof typeof EXIT];

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
