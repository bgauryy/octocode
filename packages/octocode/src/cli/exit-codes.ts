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

// Shared error-text predicates for consistent CLI failure classification.
// Auth deliberately matches only
// genuine credential failures (401/403/unauthorized/forbidden/bad credentials)
// — NOT the bare word "authentication", which appears in the ambiguous
// "may not exist, require authentication, or be inaccessible" not-found
// message and would otherwise tell an already-authenticated user to set a token.
export function isRateLimitErrorText(text: string): boolean {
  return /\b(rate[ _-]?limit|429|quota)\b/i.test(text);
}

export function isAuthErrorText(text: string): boolean {
  return /\b(401|403|unauthoriz(?:ed|ation)|forbidden|bad credentials)\b/i.test(
    text
  );
}

export function isNotFoundErrorText(text: string): boolean {
  return (
    /\b(404|not[ _-]?found|no such)\b/i.test(text) ||
    /could not determine default branch|may not exist|inaccessible/i.test(text)
  );
}

export function classifyToolErrorText(text: string): ExitCode {
  if (isRateLimitErrorText(text)) {
    return EXIT.RATE_LIMIT;
  }
  if (isAuthErrorText(text)) {
    return EXIT.AUTH;
  }
  if (isNotFoundErrorText(text)) {
    return EXIT.NOT_FOUND;
  }
  return EXIT.TOOL;
}
