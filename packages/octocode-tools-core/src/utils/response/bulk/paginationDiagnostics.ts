function recordTreeSome(
  value: unknown,
  predicate: (record: Record<string, unknown>) => boolean
): boolean {
  if (value === null || typeof value !== 'object') return false;
  if (Array.isArray(value)) {
    return value.some(item => recordTreeSome(item, predicate));
  }
  const record = value as Record<string, unknown>;
  return (
    predicate(record) ||
    Object.values(record).some(item => recordTreeSome(item, predicate))
  );
}

function executableContinuationKeySome(
  value: unknown,
  predicate: (normalizedKey: string) => boolean
): boolean {
  if (value === null || typeof value !== 'object') return false;
  if (Array.isArray(value)) {
    return value.some(item => executableContinuationKeySome(item, predicate));
  }
  return Object.entries(value as Record<string, unknown>).some(
    ([key, item]) => {
      if (item !== null && typeof item === 'object' && !Array.isArray(item)) {
        const record = item as Record<string, unknown>;
        if (
          predicate(key.toLowerCase()) &&
          typeof record.tool === 'string' &&
          record.query !== null &&
          typeof record.query === 'object'
        ) {
          return true;
        }
      }
      return executableContinuationKeySome(item, predicate);
    }
  );
}

function hasBoundedAxis(record: Record<string, unknown>): boolean {
  return (
    record.truncated === true ||
    record.capReached === true ||
    record.capped === true ||
    record.capturesTruncated === true ||
    record.totalMatchesCapped === true ||
    record.complete === false ||
    record.incompleteResults === true ||
    record.incompleteTree === true ||
    record.possiblyTruncated === true ||
    record.truncatedByDepth === true ||
    record.truncatedByBudget === true ||
    (Array.isArray(record.partialTreeFailures) &&
      record.partialTreeFailures.length > 0)
  );
}

export function isPartialResult(data: Record<string, unknown>): boolean {
  return recordTreeSome(
    data,
    record =>
      record.isPartial === true ||
      record.hasMore === true ||
      hasBoundedAxis(record)
  );
}

export function buildPaginationDiagnosticCodes(
  data: Record<string, unknown>
): string[] {
  if (!isPartialResult(data)) return [];
  if (recordTreeSome(data, record => record.terminalLimit === true)) {
    return ['terminalLimitReached'];
  }

  const hasPageableAxis = recordTreeSome(
    data,
    record => record.hasMore === true
  );
  const hasBound = recordTreeSome(data, hasBoundedAxis);
  const hasPartialContentAxis = recordTreeSome(
    data,
    record => record.isPartial === true
  );
  const hasPageContinuation = executableContinuationKeySome(
    data,
    key => key.startsWith('next') || key.startsWith('continue')
  );
  const hasExpansionContinuation = executableContinuationKeySome(
    data,
    key =>
      key.startsWith('expand') ||
      key.startsWith('retry') ||
      key.startsWith('narrow') ||
      key.startsWith('fallback') ||
      key.startsWith('escalate') ||
      key.startsWith('read') ||
      key.includes('search') ||
      key.includes('completeness')
  );
  const missingContinuation =
    (hasPageableAxis && !hasPageContinuation) ||
    (hasBound && !hasExpansionContinuation) ||
    (hasPartialContentAxis &&
      !hasPageContinuation &&
      !hasExpansionContinuation);
  return missingContinuation ? ['continuationMissing'] : [];
}
