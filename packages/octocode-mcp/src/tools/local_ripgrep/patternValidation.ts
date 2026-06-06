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

function detectRegexError(pattern: string): string | null {
  try {
    new RegExp(pattern);
    return null;
  } catch (err) {
    return err instanceof Error ? err.message : 'unknown regex parse error';
  }
}

function looksLikeLiteralSearch(pattern: string): boolean {
  if (/[\\^$|()[\]{}+*?]/.test(pattern)) {
    return false;
  }
  if (pattern.includes('.') && /^[\w.\-/:]+$/.test(pattern)) {
    return true;
  }
  return false;
}

function containsLookaround(pattern: string): boolean {
  return /\(\?[=!<]/.test(pattern);
}
