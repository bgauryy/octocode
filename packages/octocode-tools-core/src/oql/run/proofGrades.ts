/**
 * Proof-grade inference: labels every result row with how strong its evidence
 * is (structural/text/semantic/graph/candidate/missing), defaulting any row a
 * backend didn't already grade.
 */
import type {
  OqlProofGrade,
  OqlProofGradedResultRow,
  OqlQuery,
  OqlResultRow,
} from '../types.js';

export function applyProofGrades(
  query: OqlQuery,
  results: OqlResultRow[]
): asserts results is OqlProofGradedResultRow[] {
  for (const row of results) {
    row.proofGrade ??= inferProofGrade(query, row);
  }
}

function inferProofGrade(query: OqlQuery, row: OqlResultRow): OqlProofGrade {
  if (row.kind === 'code') {
    if (
      hasPredicateKind(query.where, 'structural') ||
      row.metavars !== undefined ||
      row.metavarRanges !== undefined
    ) {
      return 'structural';
    }
    if (
      hasPredicateKind(query.where, 'text') ||
      hasPredicateKind(query.where, 'regex') ||
      row.line !== undefined ||
      row.snippet !== undefined ||
      row.matchIndices !== undefined
    ) {
      return 'text';
    }
    return 'candidate';
  }

  if (row.kind === 'content' || row.kind === 'file' || row.kind === 'tree') {
    return 'text';
  }

  if (row.kind !== 'record') {
    return 'candidate';
  }

  if (row.recordType === 'semantics') {
    return 'semantic';
  }
  if (row.recordType === 'graph') {
    return hasMissingProof(row.data) ? 'missing' : 'graph';
  }
  if (row.recordType === 'research') {
    if (hasMissingProof(row.data)) {
      return 'missing';
    }
    return row.data.mode === 'prove' ? 'graph' : 'candidate';
  }
  if (row.recordType === 'diff') {
    return 'text';
  }

  return 'candidate';
}

function hasPredicateKind(
  predicate: OqlQuery['where'],
  kind: 'text' | 'regex' | 'structural'
): boolean {
  if (!predicate) {
    return false;
  }
  if (predicate.kind === kind) {
    return true;
  }
  if (predicate.kind === 'all' || predicate.kind === 'any') {
    return predicate.of.some(child => hasPredicateKind(child, kind));
  }
  if (predicate.kind === 'not') {
    return hasPredicateKind(predicate.predicate, kind);
  }
  return false;
}

export function hasMissingProof(data: Record<string, unknown>): boolean {
  const missingProof = data.missingProof;
  if (Array.isArray(missingProof) && missingProof.length > 0) {
    return true;
  }

  const packets = data.packets;
  return (
    Array.isArray(packets) &&
    packets.some(packet => {
      if (!isRecord(packet)) {
        return false;
      }
      const packetMissing = packet.missingProof;
      return (
        packet.proofStatus === 'missing' ||
        (Array.isArray(packetMissing) && packetMissing.length > 0)
      );
    })
  );
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}
