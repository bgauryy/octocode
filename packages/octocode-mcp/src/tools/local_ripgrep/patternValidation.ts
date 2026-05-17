/**
 * Pre-launch validation for ripgrep query patterns.
 *
 * The goal is to fail fast (and cheaply) on patterns that ripgrep will
 * reject anyway, and to nudge agents toward `fixedString: true` when
 * their input looks literal — saving server cycles and regex
 * backtracking risk.
 *
 * This module is intentionally side-effect free so it can be tested
 * without a filesystem or process boundary.
 *
 * @module tools/local_ripgrep/patternValidation
 */

export interface RipgrepPatternInput {
  pattern: string;
  fixedString?: boolean;
  perlRegex?: boolean;
}

export interface RipgrepPatternValidation {
  isValid: boolean;
  errors: string[];
  warnings: string[];
}

/**
 * Run a cheap syntactic sanity check on a ripgrep pattern.
 *
 * Important: this is **not** a full ripgrep grammar parser. It only
 * catches obvious failure modes (empty pattern, unmatched parens,
 * dangling escapes, lookaround without -P) so we can short-circuit
 * before spawning rg and burning a process.
 */
export function preflightValidateRipgrepPattern(
  input: RipgrepPatternInput
): RipgrepPatternValidation {
  const errors: string[] = [];
  const warnings: string[] = [];
  const pattern = input.pattern;

  if (typeof pattern !== 'string' || pattern.length === 0) {
    errors.push('pattern is empty — provide a non-empty search string');
    return { isValid: false, errors, warnings };
  }

  if (!input.fixedString) {
    const regexError = detectRegexError(pattern);
    if (regexError) {
      errors.push(`invalid regex: ${regexError}`);
    }
  }

  if (!input.fixedString && looksLikeLiteralSearch(pattern)) {
    warnings.push(
      `pattern '${pattern}' looks literal — pass fixedString: true to skip regex parsing and avoid accidental wildcards`
    );
  }

  if (!input.perlRegex && containsLookaround(pattern)) {
    warnings.push(
      'pattern uses lookaround (?= / ?! / ?<= / ?<!) which requires perlRegex: true; ripgrep will refuse it otherwise'
    );
  }

  return { isValid: errors.length === 0, errors, warnings };
}

/**
 * Attempt to compile `pattern` with the JS engine. JS regex syntax is a
 * permissive superset of rust-regex (rg's engine) so a JS error here is
 * a strong signal rg will reject the pattern too. Lookaround / named
 * groups specific to PCRE are surfaced separately.
 */
function detectRegexError(pattern: string): string | null {
  try {
    new RegExp(pattern);
    return null;
  } catch (err) {
    return err instanceof Error ? err.message : 'unknown regex parse error';
  }
}

/**
 * Heuristic: does `pattern` look like a literal search the agent
 * accidentally fed to a regex engine? Used purely to issue a friendly
 * warning — never an error.
 */
function looksLikeLiteralSearch(pattern: string): boolean {
  // Patterns with explicit regex metacharacters are clearly intentional.
  if (/[\\^$|()[\]{}+*?]/.test(pattern)) {
    return false;
  }
  // Patterns containing `.` (dot) where the rest is identifier-like are
  // a classic agent-foot-gun: `console.log` matches `console-log` too.
  if (pattern.includes('.') && /^[\w.\-/:]+$/.test(pattern)) {
    return true;
  }
  return false;
}

function containsLookaround(pattern: string): boolean {
  return /\(\?[=!<]/.test(pattern);
}
