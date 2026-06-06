const QUANTIFIER_CHARS = new Set(['+', '*', '?']);

const MAX_PATTERN_LENGTH = 1000;

type ScanStep = { unsafe: boolean; next: number };

function skipCharClass(pattern: string, pos: number): number {
  let i = pos + 1;
  while (i < pattern.length && pattern[i] !== ']') {
    if (pattern[i] === '\\') i++;
    i++;
  }
  return i + 1;
}

function isQuantifierAt(pattern: string, pos: number): boolean {
  const ch = pattern[pos];
  if (ch === undefined) return false;
  return (
    QUANTIFIER_CHARS.has(ch) ||
    (ch === '{' && isRepetitionQuantifier(pattern, pos))
  );
}

function handleCloseGroup(
  pattern: string,
  i: number,
  stack: boolean[]
): ScanStep {
  const innerHasQuantifier = stack.pop() ?? false;
  const isQuantified = isQuantifierAt(pattern, i + 1);

  if (isQuantified && innerHasQuantifier) {
    return { unsafe: true, next: i + 1 };
  }

  if ((isQuantified || innerHasQuantifier) && stack.length > 0) {
    stack[stack.length - 1] = true;
  }

  return {
    unsafe: false,
    next: isQuantified ? skipQuantifier(pattern, i + 1) : i + 1,
  };
}

function handleQuantifierAtom(
  pattern: string,
  i: number,
  stack: boolean[]
): ScanStep {
  if (stack.some(g => g)) {
    return { unsafe: true, next: i + 1 };
  }
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

  const groupHasQuantifier: boolean[] = [];
  let i = 0;

  while (i < pattern.length) {
    const ch = pattern[i];

    if (ch === '\\') {
      i += 2;
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

function isRepetitionQuantifier(pattern: string, pos: number): boolean {
  if (pattern[pos] !== '{') return false;
  const closeBrace = pattern.indexOf('}', pos);
  if (closeBrace === -1) return false;
  return /^\{\d+,?\d*\}$/.test(pattern.slice(pos, closeBrace + 1));
}

function skipQuantifier(pattern: string, pos: number): number {
  const ch = pattern[pos];
  if (ch === '{') {
    pos = pattern.indexOf('}', pos) + 1;
  } else {
    pos++;
  }
  if (pos < pattern.length && (pattern[pos] === '?' || pattern[pos] === '+')) {
    pos++;
  }
  return pos;
}

export function createSafeRegExp(pattern: string, flags?: string): RegExp {
  const safety = checkRegexSafety(pattern);
  if (!safety.safe) {
    throw new Error(safety.reason);
  }
  return new RegExp(pattern, flags);
}
