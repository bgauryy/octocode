/**
 * Predicate classification helpers, field-predicate -> findFiles mapping,
 * backend-result diagnostics, and the shared provenance-record builder used
 * across the local adapter's execution paths.
 */
import { diagnostic } from '../../diagnostics.js';
import type {
  DiagnosticCode,
  FieldPredicate,
  OqlDiagnostic,
  Predicate,
  OqlProvenance,
  QuerySource,
} from '../../types.js';
import type { LocalFindToolQuery } from './findToolType.js';

/** A boolean `where` that needs multi-call evaluation (not a single leaf). */
export function needsBooleanEval(p: Predicate): boolean {
  if (p.kind === 'all' || p.kind === 'any') return true;
  // not(leaf) compiles to a single call; not(boolean) needs set algebra.
  if (p.kind === 'not') return isBooleanPredicate(p.predicate);
  return false;
}

export function isRipgrepContentLeaf(p: Predicate): boolean {
  return p.kind === 'text' || p.kind === 'regex';
}

export function isNegatedRipgrepContentLeaf(
  p: Predicate | undefined
): p is Predicate & { kind: 'not'; predicate: Predicate } {
  return p?.kind === 'not' && isRipgrepContentLeaf(p.predicate);
}

export function isBooleanPredicate(p: Predicate): boolean {
  return p.kind === 'all' || p.kind === 'any' || p.kind === 'not';
}

export function isContentPredicate(p: Predicate): boolean {
  if (p.kind === 'text' || p.kind === 'regex' || p.kind === 'structural') {
    return true;
  }
  if (p.kind === 'not') return isContentPredicate(p.predicate);
  return false;
}

export function applyFieldPredicate(
  where: Predicate,
  toolQuery: Partial<LocalFindToolQuery>,
  diags: OqlDiagnostic[]
): void {
  const negate = where.kind === 'not';
  const inner = where.kind === 'not' ? where.predicate : where;
  if (inner.kind !== 'field') {
    diags.push(
      diagnostic(
        'unsupportedPredicate',
        'Only field predicates (and field-negation) compile to the files backend.',
        { backend: 'localFindFiles' }
      )
    );
    return;
  }
  const f = inner as FieldPredicate;
  const value = f.value;
  switch (f.field) {
    case 'basename':
    case 'path':
      if (f.op === 'regex') toolQuery.regex = String(value);
      else if (f.op === 'glob' || f.op === '=' || f.op === 'in') {
        toolQuery.names = Array.isArray(value)
          ? value.map(String)
          : [String(value)];
      } else {
        diags.push(unsupportedField(f));
      }
      break;
    case 'extension': {
      const exts = (Array.isArray(value) ? value : [value]).map(
        v => `*.${String(v).replace(/^\./, '')}`
      );
      if (f.op === '=' || f.op === 'in' || f.op === 'glob')
        toolQuery.names = exts;
      else diags.push(unsupportedField(f));
      break;
    }
    case 'size':
      if (f.op === '>' || f.op === '>=') toolQuery.sizeGreater = String(value);
      else if (f.op === '<' || f.op === '<=')
        toolQuery.sizeLess = String(value);
      else diags.push(unsupportedField(f));
      break;
    case 'modified':
      // findFiles only has RELATIVE windows (modifiedWithin/Before take "7d").
      // It has no absolute-date filter, so >/>=/</<= (absolute timestamps)
      // are unsupported — mapping them to a duration field would be both a
      // type mismatch and a semantic inversion.
      if (f.op === 'within') toolQuery.modifiedWithin = String(value);
      else if (f.op === 'before') toolQuery.modifiedBefore = String(value);
      else
        diags.push(
          diagnostic(
            'unsupportedPredicate',
            'field "modified" supports only `within` / `before` (relative windows like "7d"); findFiles has no absolute-date filter for >/</>=/<=.',
            { backend: 'localFindFiles' }
          )
        );
      break;
    case 'accessed':
      if (f.op === 'within') toolQuery.accessedWithin = String(value);
      else diags.push(unsupportedField(f));
      break;
    case 'empty':
      toolQuery.empty = Boolean(value);
      break;
    case 'permissions':
      toolQuery.permissions = String(value);
      break;
    case 'executable':
    case 'readable':
    case 'writable':
      toolQuery[f.field] = Boolean(value);
      break;
    case 'entryType':
      toolQuery.entryType = String(value) === 'directory' ? 'd' : 'f';
      break;
    default:
      // Unmapped field: never silently drop the predicate — signal it so the
      // result is not mistaken for the unfiltered universe.
      diags.push(unsupportedField(f));
      break;
  }
  if (negate) {
    diags.push(
      diagnostic(
        'residualNotExact',
        'Negated field predicates over findFiles are best-effort.',
        { backend: 'localFindFiles', severity: 'warning' }
      )
    );
  }
}

function unsupportedField(f: FieldPredicate): OqlDiagnostic {
  return diagnostic(
    'unsupportedPredicate',
    `field "${f.field}" with operator "${f.op}" is not supported by the files backend.`,
    { backend: 'localFindFiles' }
  );
}

export function resultDiagnostics(
  result: { status?: string; error?: string; warnings?: string[] },
  backend: string
): OqlDiagnostic[] {
  const out: OqlDiagnostic[] = [];
  if (result.status === 'error') {
    out.push(
      diagnostic('invalidQuery', result.error ?? 'Backend error', { backend })
    );
  } else if (result.status === 'empty') {
    out.push(
      diagnostic('zeroMatches', 'Query ran and matched nothing.', {
        backend,
        severity: 'info',
        blocksAnswer: false,
      })
    );
  }
  for (const w of result.warnings ?? []) {
    out.push(
      diagnostic(classifyWarning(w), w, {
        backend,
        severity: 'warning',
        blocksAnswer: false,
      })
    );
  }
  return out;
}

/** Map a backend warning string to the closest typed diagnostic code. */
function classifyWarning(message: string): DiagnosticCode {
  const m = message.toLowerCase();
  if (m.includes('skipped parsing') || m.includes('parse error')) {
    return 'partialParse';
  }
  if (m.includes('capped') || m.includes('truncat')) return 'matchTruncated';
  if (m.includes('redact') || m.includes('sanitiz') || m.includes('secret')) {
    return 'sanitized';
  }
  return 'partialResult';
}

export function provenance(
  backend: string,
  source: QuerySource,
  where: Predicate | undefined
): OqlProvenance {
  return {
    backend,
    source,
    ...(where?.id ? { pushed: [where.id] } : {}),
  };
}
