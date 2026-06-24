/**
 * Compile a canonical OQL `where` predicate into local-search (ripgrep /
 * structural) query fields, or report why it can't be compiled.
 *
 * The local-search adapter evaluates one effective leaf per call (ripgrep
 * `keywords` is a single pattern). Supported shapes:
 *   - a single leaf (text / regex / structural)
 *   - not(leaf)            -> negated (invertMatch or filesWithoutMatch)
 * Anything else (boolean all/any, nested negation) returns `unsupportedBoolean`
 * so execution never silently drops predicates.
 */
import { structuralRuleToYaml } from './ruleYaml.js';
import type { DiagnosticCode, Predicate } from '../types.js';

export interface CompiledMatch {
  keywords?: string;
  fixedString?: boolean;
  perlRegex?: boolean;
  caseInsensitive?: boolean;
  caseSensitive?: boolean;
  wholeWord?: boolean;
  multiline?: boolean;
  multilineDotall?: boolean;
  mode?: 'structural';
  pattern?: string;
  rule?: unknown;
  langType?: string;
}

export interface CompileResult {
  match?: CompiledMatch;
  negate?: boolean;
  unsupported?: { code: DiagnosticCode; message: string; predicateId?: string };
}

function applyCase(
  out: CompiledMatch,
  c: 'smart' | 'sensitive' | 'insensitive' | undefined
): void {
  if (c === 'sensitive') out.caseSensitive = true;
  else if (c === 'insensitive') out.caseInsensitive = true;
  // 'smart' (default) => leave both unset; ripgrep smart-case applies.
}

function compileLeaf(p: Predicate): CompileResult {
  switch (p.kind) {
    case 'text': {
      const out: CompiledMatch = { keywords: p.value, fixedString: true };
      applyCase(out, p.case);
      if (p.wholeWord) out.wholeWord = true;
      return { match: out };
    }
    case 'regex': {
      const out: CompiledMatch = { keywords: p.value };
      if (p.dialect === 'pcre2') out.perlRegex = true;
      applyCase(out, p.case);
      if (p.wholeWord) out.wholeWord = true;
      if (p.multiline) out.multiline = true;
      if (p.dotAll) out.multilineDotall = true;
      return { match: out };
    }
    case 'structural': {
      const out: CompiledMatch = { mode: 'structural', langType: p.lang };
      if (typeof p.pattern === 'string') out.pattern = p.pattern;
      // The engine's `rule` field is a YAML string. OQL accepts either the
      // grep-compatible YAML string directly or the JSON object form.
      if (typeof p.rule === 'string') out.rule = p.rule;
      else if (p.rule !== undefined) out.rule = structuralRuleToYaml(p.rule);
      return { match: out };
    }
    case 'field':
      return {
        unsupported: {
          code: 'unsupportedPredicate',
          message:
            'Field predicates compile through the files backend, not local code search.',
          predicateId: p.id,
        },
      };
    default:
      return {
        unsupported: {
          code: 'unsupportedBoolean',
          message: `Boolean predicate "${p.kind}" is not compilable to a single local-search call.`,
        },
      };
  }
}

export function compileWhere(where: Predicate): CompileResult {
  if (
    where.kind === 'text' ||
    where.kind === 'regex' ||
    where.kind === 'structural' ||
    where.kind === 'field'
  ) {
    return compileLeaf(where);
  }
  if (where.kind === 'not') {
    const inner = where.predicate;
    if (
      inner.kind === 'text' ||
      inner.kind === 'regex' ||
      inner.kind === 'field'
    ) {
      const compiled = compileLeaf(inner);
      if (compiled.unsupported) return compiled;
      return { ...compiled, negate: true };
    }
    return {
      unsupported: {
        code: 'unsupportedBoolean',
        message:
          'not() over structural/boolean predicates is not supported by the local-search adapter.',
      },
    };
  }
  return {
    unsupported: {
      code: 'unsupportedBoolean',
      message: `Boolean predicate "${where.kind}" requires multiple backend calls; not supported by the single-call local-search adapter.`,
    },
  };
}
