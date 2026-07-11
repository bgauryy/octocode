/**
 * `select`: project result rows to the requested fields/continuations.
 * Projection only — never adds data or changes the result domain.
 */
import { diagnostic } from '../diagnostics.js';
import type { OqlQuery, OqlResultEnvelope, OqlResultRow } from '../types.js';

// Row identity always survives projection (needed to cite + continue).
const SELECT_ALWAYS_KEEP = new Set([
  'kind',
  'source',
  'recordType',
  'id',
  'proofGrade',
]);

// Projectable per-row fields across all row kinds.
const SELECTABLE_ROW_FIELDS = new Set([
  'path',
  'line',
  'endLine',
  'column',
  'snippet',
  'matchIndices',
  'metadata',
  'content',
  'contentView',
  'range',
  'metavars',
  'metavarRanges',
  'matchKind',
  'scoreHint',
  'proofGrade',
  'size',
  'modified',
  'entryType',
  'depth',
  'children',
  'data',
]);

// Record-data sub-domains (research/graph detailed payloads). A bare selector
// like "symbols" or "files" sub-projects WITHIN `data` — the research/graph
// adapter performs that projection — so here the token just keeps the carrying
// `data` field and never warns (P1: narrow `select` drops unrequested domains).
const RECORD_DATA_SUBFIELDS = new Set([
  'manifests',
  'files',
  'dependencies',
  'symbols',
  'graphFacts',
  'packets',
  'nodes',
  'edges',
  'facts',
]);

// Envelope-level select tokens: recognized, no per-row effect (the envelope
// always carries them). `repo`/`localPath` are identity carried by `source`.
const SELECT_ENVELOPE_TOKENS = new Set([
  'pagination',
  'diagnostics',
  'provenance',
  'evidence',
  'repo',
  'localPath',
]);

/**
 * Project result rows to the requested `select` fields. Projection only: it
 * filters which fields/continuations appear, never adds data or changes the
 * result domain. Identity fields always survive. Unknown selectors yield a
 * non-blocking `unknownField` diagnostic. Dotted record-data selectors
 * (e.g. `data.summary`) are accepted but not sub-projected (the whole `data`
 * stays if `data` is selected).
 */
export function applySelect(
  query: OqlQuery,
  results: OqlResultRow[]
): OqlResultEnvelope['diagnostics'] {
  const select = query.select;
  if (!select || select.length === 0) return [];

  const nextKeys = new Set<string>();
  const rowFields = new Set<string>();
  let keepAllNext = false;
  const unknown: string[] = [];

  for (const raw of select) {
    const token = raw.trim();
    if (token === 'next') {
      keepAllNext = true;
    } else if (token.startsWith('next.')) {
      nextKeys.add(token);
    } else if (SELECTABLE_ROW_FIELDS.has(token)) {
      rowFields.add(token);
    } else if (RECORD_DATA_SUBFIELDS.has(token)) {
      // bare record-data sub-domain → keep `data`; adapter sub-projects it.
      rowFields.add('data');
    } else if (SELECT_ENVELOPE_TOKENS.has(token)) {
      // recognized envelope token — no row projection needed
    } else if (token.includes('.')) {
      // dotted record-data selector (e.g. packets.subject / data.summary):
      // keep the carrying field; do not sub-project.
      rowFields.add('data');
    } else {
      unknown.push(token);
    }
  }

  for (const row of results) {
    const r = row as unknown as Record<string, unknown>;
    for (const key of Object.keys(r)) {
      if (SELECT_ALWAYS_KEEP.has(key)) continue;
      if (key === 'next') {
        if (keepAllNext) continue;
        const next = r.next as Record<string, unknown> | undefined;
        if (!next) continue;
        if (nextKeys.size === 0) {
          delete r.next;
          continue;
        }
        for (const nk of Object.keys(next)) {
          if (!nextKeys.has(nk)) delete next[nk];
        }
        if (Object.keys(next).length === 0) delete r.next;
        continue;
      }
      if (!rowFields.has(key)) delete r[key];
    }
  }

  return unknown.length
    ? [
        diagnostic(
          'unknownField',
          `select contains unknown field(s): ${unknown.join(', ')}. They were ignored.`,
          { queryPath: 'select', severity: 'warning', blocksAnswer: false }
        ),
      ]
    : [];
}
