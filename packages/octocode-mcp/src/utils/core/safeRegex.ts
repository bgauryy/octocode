/**
 * Lightweight ReDoS detection for user-supplied regex patterns.
 *
 * Detects the most common ReDoS vectors: nested quantifiers (star height > 1).
 * Patterns like (a+)+, (a*)+, (.*a)* cause exponential backtracking.
 */

/** Quantifier characters that indicate repetition */
const QUANTIFIER_CHARS = new Set(['+', '*', '?']);

/** Maximum pattern length to prevent excessive parsing time */
const MAX_PATTERN_LENGTH = 1000;

/**
 * Check if a regex pattern is likely safe from catastrophic backtracking.
 *
 * Uses a simple heuristic: track whether each group contains a quantified
 * sub-expression. If a group that contains a quantifier is itself quantified,
 * the pattern has star height > 1 and is flagged as unsafe.
 *
 * @returns `{ safe: true }` or `{ safe: false, reason: string }`
 */
/**
 * Result of advancing the ReDoS scanner past one construct: either an
 * unsafe verdict, or the next index to resume scanning from. Handlers mutate
 * the shared `groupHasQuantifier` stack in place.
 */
type ScanStep = { unsafe: boolean; next: number };

/** Skip a character class `[...]`; quantifiers inside `[]` are literals. */
function skipCharClass(pattern: string, pos: number): number {
  let i = pos + 1;
  while (i < pattern.length && pattern[i] !== ']') {
    if (pattern[i] === '\\') i++;
    i++;
  }
  return i + 1; // skip closing ]
}

/** True when the construct at `pos` is a quantifier (`+ * ?` or `{n,m}`). */
function isQuantifierAt(pattern: string, pos: number): boolean {
  const ch = pattern[pos];
  if (ch === undefined) return false;
  return (
    QUANTIFIER_CHARS.has(ch) ||
    (ch === '{' && isRepetitionQuantifier(pattern, pos))
  );
}

/** Handle a closing `)`: detect star height > 1 and propagate to parent. */
function handleCloseGroup(
  pattern: string,
  i: number,
  stack: boolean[]
): ScanStep {
  const innerHasQuantifier = stack.pop() ?? false;
  const isQuantified = isQuantifierAt(pattern, i + 1);

  // Star height > 1: a quantified group that itself contains a quantifier.
  if (isQuantified && innerHasQuantifier) {
    return { unsafe: true, next: i + 1 };
  }

  // Propagate: parent group now contains a quantified sub-expression.
  if ((isQuantified || innerHasQuantifier) && stack.length > 0) {
    stack[stack.length - 1] = true;
  }

  // Skip the quantifier and any lazy/possessive modifier.
  return {
    unsafe: false,
    next: isQuantified ? skipQuantifier(pattern, i + 1) : i + 1,
  };
}

/** Handle a quantifier after a non-group atom (e.g. `a+`, `\d*`, `.+`). */
function handleQuantifierAtom(
  pattern: string,
  i: number,
  stack: boolean[]
): ScanStep {
  // Inside a group that already holds a quantifier → nested quantifiers.
  if (stack.some(g => g)) {
    return { unsafe: true, next: i + 1 };
  }
  // Mark the innermost enclosing group as containing a quantifier.
  if (stack.length > 0) {
    stack[stack.length - 1] = true;
  }
  return { unsafe: false, next: skipQuantifier(pattern, i) };
}

export function checkRegexSafety(pattern: string): {
  safe: boolean;
  reason?: string;
} {
  if (pattern.length > MAX_PATTERN_LENGTH) {
    return { safe: false, reason: 'Pattern too long (max 1000 characters)' };
  }

  const REDOS_MSG =
    'Nested quantifiers detected (potential ReDoS). Simplify the pattern.';

  // Stack tracks whether each open group contains a quantified sub-expression.
  // When a group closes, its flag propagates to the parent group.
  const groupHasQuantifier: boolean[] = [];
  let i = 0;

  while (i < pattern.length) {
    const ch = pattern[i];

    if (ch === '\\') {
      i += 2; // skip escaped character
      continue;
    }

    if (ch === '[') {
      i = skipCharClass(pattern, i);
      continue;
    }

    if (ch === '(') {
      groupHasQuantifier.push(false);
      i++;
      continue;
    }

    if (ch === ')') {
      const step = handleCloseGroup(pattern, i, groupHasQuantifier);
      if (step.unsafe) return { safe: false, reason: REDOS_MSG };
      i = step.next;
      continue;
    }

    if (isQuantifierAt(pattern, i)) {
      const step = handleQuantifierAtom(pattern, i, groupHasQuantifier);
      if (step.unsafe) return { safe: false, reason: REDOS_MSG };
      i = step.next;
      continue;
    }

    i++;
  }

  return { safe: true };
}

/** Check if pattern[pos] starts a {n,m} repetition quantifier */
function isRepetitionQuantifier(pattern: string, pos: number): boolean {
  if (pattern[pos] !== '{') return false;
  const closeBrace = pattern.indexOf('}', pos);
  if (closeBrace === -1) return false;
  return /^\{\d+,?\d*\}$/.test(pattern.slice(pos, closeBrace + 1));
}

/** Skip past a quantifier (including lazy/possessive modifier) */
function skipQuantifier(pattern: string, pos: number): number {
  const ch = pattern[pos];
  if (ch === '{') {
    pos = pattern.indexOf('}', pos) + 1;
  } else {
    pos++; // skip +, *, or ?
  }
  // Skip lazy (?) or possessive (+) modifier
  if (pos < pattern.length && (pattern[pos] === '?' || pattern[pos] === '+')) {
    pos++;
  }
  return pos;
}

/**
 * Create a RegExp from a user-supplied pattern, rejecting unsafe patterns.
 *
 * @throws Error if the pattern is invalid or has ReDoS risk
 */
export function createSafeRegExp(pattern: string, flags?: string): RegExp {
  const safety = checkRegexSafety(pattern);
  if (!safety.safe) {
    throw new Error(safety.reason);
  }
  return new RegExp(pattern, flags);
}
