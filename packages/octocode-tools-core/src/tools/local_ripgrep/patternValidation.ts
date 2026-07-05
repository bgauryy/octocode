import { contextUtils } from '../../utils/contextUtils.js';

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

  const nativeValidation = contextUtils.validateRipgrepPattern(
    pattern,
    input.fixedString,
    input.perlRegex
  );
  if (!nativeValidation.valid) {
    errors.push(
      `invalid regex: ${nativeValidation.error ?? 'unknown regex parse error'}`
    );
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
