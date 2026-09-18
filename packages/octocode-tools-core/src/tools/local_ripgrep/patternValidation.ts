interface RipgrepPatternInput {
  pattern: string;
  fixedString?: boolean;
  perlRegex?: boolean;
}

export interface RipgrepPatternValidation {
  isValid: boolean;
  errors: string[];
  warnings: string[];
}

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

  // Matcher syntax is compiled once at the native execution boundary. Doing
  // the same native compilation here doubled setup work for every valid query.

  if (!input.fixedString && looksLikeLiteralSearch(pattern)) {
    warnings.push(
      `pattern '${pattern}' looks literal — set regex:"literal" to skip regex parsing and avoid accidental wildcards`
    );
  }

  if (!input.fixedString && !input.perlRegex && containsLookaround(pattern)) {
    warnings.push(
      'pattern uses lookaround (?= / ?! / ?<= / ?<!) which requires regex:"pcre2"; ripgrep will refuse it otherwise'
    );
  }

  return { isValid: errors.length === 0, errors, warnings };
}

function looksLikeLiteralSearch(pattern: string): boolean {
  // If the pattern contains any regex special chars, it's intentionally a regex.
  if (/[\\^$|()[\]{}+*?]/.test(pattern)) {
    return false;
  }
  // Patterns with a dot + word chars look like filenames/method names (e.g. fs.readFile).
  if (pattern.includes('.') && /^[\w.\-/:]+$/.test(pattern)) {
    return true;
  }
  // Plain identifiers (word chars only, length > 2) are almost always literal searches.
  if (pattern.length > 2 && /^\w+$/.test(pattern)) {
    return true;
  }
  return false;
}

function containsLookaround(pattern: string): boolean {
  return /\(\?[=!<]/.test(pattern);
}
