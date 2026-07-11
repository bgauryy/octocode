/**
 * `where` predicate lowering for `octocode search`.
 *
 * Turns the flat shorthand flags (--pattern/--rule/--regex/text, plus the
 * file-discovery field flags like --extension/--filename/--size-greater)
 * into the canonical `Predicate` tree. Predicate precedence within a search
 * term is pattern > rule > regex > text.
 */
import type { Predicate, StructuralRuleInput } from '../types.js';
import type { SearchShorthand } from './types.js';

export function buildPredicate(
  parts: SearchShorthand
): { where?: Predicate } | { error: string } {
  const fieldPredicates = buildFieldPredicates(parts);

  if (parts.filesWithoutMatch) {
    const inner = predicateFromSearchTerm(parts);
    if ('error' in inner || !inner.where) return inner;
    const negated: Predicate = { kind: 'not', predicate: inner.where };
    return {
      where: fieldPredicates.length
        ? { kind: 'all', of: [...fieldPredicates, negated] }
        : negated,
    };
  }

  const base =
    parts.search === 'path'
      ? pathPredicate(parts)
      : predicateFromSearchTerm(parts);
  if ('error' in base) return base;
  if (fieldPredicates.length && base.where)
    return { where: { kind: 'all', of: [...fieldPredicates, base.where] } };
  if (fieldPredicates.length === 1) return { where: fieldPredicates[0] };
  if (fieldPredicates.length > 1)
    return { where: { kind: 'all', of: fieldPredicates } };
  return base;
}

function buildFieldPredicates(parts: SearchShorthand): Predicate[] {
  const predicates: Predicate[] = [];
  if (parts.filesOnly) {
    predicates.push({
      kind: 'field',
      field: 'entryType',
      op: '=',
      value: 'file',
    });
  } else if (parts.directoriesOnly || parts.entryType) {
    predicates.push({
      kind: 'field',
      field: 'entryType',
      op: '=',
      value: parts.directoriesOnly ? 'directory' : parts.entryType,
    });
  }
  if (parts.extension) {
    predicates.push({
      kind: 'field',
      field: 'extension',
      op: '=',
      value: parts.extension.replace(/^\./, ''),
    });
  }
  if (parts.filename) {
    predicates.push({
      kind: 'field',
      field: 'basename',
      op: 'glob',
      value: globValue(parts.filename),
    });
  }
  if (parts.pathPattern) {
    predicates.push({
      kind: 'field',
      field: 'path',
      op: 'glob',
      value: globValue(parts.pathPattern),
    });
  }
  if (parts.sizeGreater) {
    predicates.push({
      kind: 'field',
      field: 'size',
      op: '>',
      value: parts.sizeGreater,
    });
  }
  if (parts.sizeLess) {
    predicates.push({
      kind: 'field',
      field: 'size',
      op: '<',
      value: parts.sizeLess,
    });
  }
  if (parts.modifiedWithin) {
    predicates.push({
      kind: 'field',
      field: 'modified',
      op: 'within',
      value: parts.modifiedWithin,
    });
  }
  if (parts.modifiedBefore) {
    predicates.push({
      kind: 'field',
      field: 'modified',
      op: 'before',
      value: parts.modifiedBefore,
    });
  }
  if (parts.accessedWithin) {
    predicates.push({
      kind: 'field',
      field: 'accessed',
      op: 'within',
      value: parts.accessedWithin,
    });
  }
  if (parts.empty) {
    predicates.push({ kind: 'field', field: 'empty', op: '=', value: true });
  }
  if (parts.permissions) {
    predicates.push({
      kind: 'field',
      field: 'permissions',
      op: '=',
      value: parts.permissions,
    });
  }
  for (const [field, enabled] of [
    ['executable', parts.executable],
    ['readable', parts.readable],
    ['writable', parts.writable],
  ] as const) {
    if (enabled)
      predicates.push({ kind: 'field', field, op: '=', value: true });
  }
  return predicates;
}

function predicateFromSearchTerm(
  parts: SearchShorthand
): { where?: Predicate } | { error: string } {
  const structuralLang = parts.lang;
  if (parts.pattern !== undefined) {
    if (!structuralLang)
      return { error: '--pattern requires --lang (e.g. --lang ts).' };
    return {
      where: {
        kind: 'structural',
        lang: structuralLang,
        pattern: parts.pattern,
      },
    };
  }
  if (parts.rule !== undefined) {
    if (!structuralLang)
      return { error: '--rule requires --lang (e.g. --lang ts).' };
    return {
      where: {
        kind: 'structural',
        lang: structuralLang,
        // parts.rule is JSON-parsed upstream (SearchShorthand.rule: unknown);
        // it is the pre-parsed rule object/string the structural predicate
        // accepts. Downstream compile validates it before use.
        rule: parts.rule as StructuralRuleInput,
      },
    };
  }
  if (parts.regex !== undefined) {
    if (parts.fixedString) {
      // --fixed wins over --regex: treat the pattern as a literal text term
      return {
        where: {
          kind: 'text',
          value: parts.regex,
          ...caseControl(parts),
          ...(parts.wholeWord ? { wholeWord: true } : {}),
        },
      };
    }
    return {
      where: {
        kind: 'regex',
        value: parts.regex,
        ...(parts.pcre2 ? { dialect: 'pcre2' } : {}),
        ...caseControl(parts),
        ...(parts.wholeWord ? { wholeWord: true } : {}),
        ...(parts.multiline ? { multiline: true } : {}),
        ...(parts.multilineDotall ? { dotAll: true } : {}),
      },
    };
  }
  if (parts.text !== undefined && parts.text !== '') {
    return {
      where: {
        kind: 'text',
        value: parts.text,
        ...caseControl(parts),
        ...(parts.wholeWord ? { wholeWord: true } : {}),
      },
    };
  }
  return {};
}

function pathPredicate(
  parts: SearchShorthand
): { where?: Predicate } | { error: string } {
  if (parts.regex !== undefined) {
    return {
      where: {
        kind: 'field',
        field: 'path',
        op: 'regex',
        value: parts.regex,
      },
    };
  }
  if (parts.text !== undefined && parts.text !== '') {
    const hasPathShape = /[/*?[\]]/.test(parts.text);
    return {
      where: {
        kind: 'field',
        field: hasPathShape ? 'path' : 'basename',
        op: 'glob',
        value: hasPathShape ? parts.text : `*${parts.text}*`,
      },
    };
  }
  return {};
}

function globValue(value: string): string {
  return /[*?[\]]/.test(value) ? value : `*${value}*`;
}

function caseControl(parts: SearchShorthand): {
  case?: 'sensitive' | 'insensitive';
} {
  if (parts.caseSensitive) return { case: 'sensitive' };
  if (parts.caseInsensitive) return { case: 'insensitive' };
  return {};
}
