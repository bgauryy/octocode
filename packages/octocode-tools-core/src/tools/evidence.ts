import type {
  EvidenceMetadata,
  ProcessedBulkResult,
} from '../types/toolResults.js';

type BuildEvidenceMetadataArgs = {
  readonly kind: NonNullable<EvidenceMetadata['kind']>;
  readonly answerReady: boolean;
  readonly incompleteReasons?: readonly string[];
  readonly emptyReason: string;
  readonly confidence?: NonNullable<EvidenceMetadata['confidence']>;
};

type BuildCollectionEvidenceArgs = {
  readonly result: unknown;
  readonly collectionField: string;
  readonly totalKeys: readonly string[];
  readonly paginationMoreReason: string;
  readonly kind: NonNullable<EvidenceMetadata['kind']>;
  readonly emptyReason: string;
};

export function attachEvidence<T extends ProcessedBulkResult>(
  result: T,
  evidence: EvidenceMetadata
): T {
  if (result.status === 'error' || Object.keys(evidence).length === 0) {
    return result;
  }
  (result as Record<string, unknown>).evidence = evidence;
  return result;
}

export function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null;
}

export function records(value: unknown): Record<string, unknown>[] {
  return Array.isArray(value) ? value.filter(isRecord) : [];
}

export function hasMorePagination(value: unknown): boolean {
  return isRecord(value) && value.hasMore === true;
}

export function paginationTotal(value: unknown, ...keys: string[]): number {
  if (!isRecord(value)) return 0;
  for (const key of keys) {
    const n = value[key];
    if (typeof n === 'number' && Number.isFinite(n)) return n;
  }
  return 0;
}

export function buildEvidenceMetadata({
  kind,
  answerReady,
  incompleteReasons = [],
  emptyReason,
  confidence = 'medium',
}: BuildEvidenceMetadataArgs): EvidenceMetadata {
  const reasons = Array.from(
    new Set(incompleteReasons.map(reason => reason.trim()).filter(Boolean))
  );

  return {
    kind,
    answerReady,
    complete: reasons.length === 0,
    ...(reasons.length > 0
      ? { confidence, reason: reasons.join(' ') }
      : answerReady
        ? {}
        : { reason: emptyReason }),
  };
}

export function incompleteHintReasons(value: unknown): string[] {
  if (!isRecord(value) || !Array.isArray(value.hints)) {
    return [];
  }
  const hasIncompleteHint = value.hints.some(
    hint =>
      typeof hint === 'string' && /\b(capped|limited|truncated)\b/i.test(hint)
  );
  return hasIncompleteHint
    ? ['Result hints report capped, limited, or truncated output.']
    : [];
}

export function buildCollectionEvidence({
  result,
  collectionField,
  totalKeys,
  paginationMoreReason,
  kind,
  emptyReason,
}: BuildCollectionEvidenceArgs): EvidenceMetadata {
  const data = isRecord(result) ? result : {};
  const items = records(data[collectionField]);
  const hasResults =
    items.length > 0 || paginationTotal(data.pagination, ...totalKeys) > 0;
  const reasons: string[] = [];
  if (hasMorePagination(data.pagination)) {
    reasons.push(paginationMoreReason);
  }
  reasons.push(...incompleteHintReasons(data));

  return buildEvidenceMetadata({
    kind,
    answerReady: hasResults,
    incompleteReasons: reasons,
    emptyReason,
  });
}
