import type {
  HintableToolResult,
  RemoteLocation,
  RemoteMaterialization,
} from './types.js';

export function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function locationPayload(location: RemoteLocation): Record<string, unknown> {
  return {
    kind: location.kind,
    localPath: location.localPath,
    ...(location.repoRoot ? { repoRoot: location.repoRoot } : {}),
    ...(location.requestedPath
      ? { requestedPath: location.requestedPath }
      : {}),
    ...(location.source ? { source: location.source } : {}),
    ...(location.cached !== undefined ? { cached: location.cached } : {}),
    ...(location.complete !== undefined ? { complete: location.complete } : {}),
    ...(location.verified !== undefined ? { verified: location.verified } : {}),
    ...(location.commitSha ? { commitSha: location.commitSha } : {}),
    ...(location.hasSubdirectories ? { hasSubdirectories: true } : {}),
    ...(location.skippedSummary &&
    Object.keys(location.skippedSummary).length > 0
      ? { skippedSummary: location.skippedSummary }
      : {}),
    ...(location.resolvedBranch
      ? { resolvedBranch: location.resolvedBranch }
      : {}),
  };
}

/**
 * Renders the structured `location` of a materialization as a compact text
 * block. The shape mirrors `structuredContent.location` — agents should read
 * the typed fields, not parse prose hints (the old behavior).
 */
export function formatMaterializationHints(
  materialized: RemoteMaterialization
): string {
  const { location } = materialized;
  const lines = [
    'location:',
    `  kind: ${JSON.stringify(location.kind)}`,
    `  localPath: ${JSON.stringify(location.localPath)}`,
  ];
  if (location.repoRoot) {
    lines.push(`  repoRoot: ${JSON.stringify(location.repoRoot)}`);
  }
  if (location.requestedPath) {
    lines.push(`  requestedPath: ${JSON.stringify(location.requestedPath)}`);
  }
  if (location.source) {
    lines.push(`  source: ${JSON.stringify(location.source)}`);
  }
  if (location.resolvedBranch) {
    lines.push(`  resolvedBranch: ${JSON.stringify(location.resolvedBranch)}`);
  }
  if (location.cached !== undefined) {
    lines.push(`  cached: ${location.cached}`);
  }
  if (location.complete !== undefined) {
    lines.push(`  complete: ${location.complete}`);
  }
  if (location.verified !== undefined) {
    lines.push(`  verified: ${location.verified}`);
  }
  if (location.commitSha) {
    lines.push(`  commitSha: ${location.commitSha}`);
  }
  if (location.hasSubdirectories) {
    lines.push(`  hasSubdirectories: true`);
  }
  if (
    location.skippedSummary &&
    Object.keys(location.skippedSummary).length > 0
  ) {
    lines.push(`  skippedSummary: ${JSON.stringify(location.skippedSummary)}`);
  }
  return lines.join('\n');
}

/** Remote context stamped into every `next.*` query so agents following
 *  pagination to page 2+ can see the provenance even though localSearchCode
 *  won't re-emit the full `location` block. */
function remoteAnnotation(m: RemoteMaterialization): Record<string, unknown> {
  return {
    _remote: {
      owner: m.owner,
      repo: m.repo,
      source: m.source,
      localPath: m.localPath,
      ...(m.branch ? { branch: m.branch } : {}),
      ...(m.commitSha ? { commitSha: m.commitSha } : {}),
    },
  };
}

/** Walk results[*].data.next.* and inject _remote into each `query` object. */
function annotateNextPointers(
  sc: Record<string, unknown>,
  annotation: Record<string, unknown>
): Record<string, unknown> {
  const results = sc.results;
  if (!Array.isArray(results)) return sc;
  const patched = results.map(item => {
    if (!isRecord(item)) return item;
    const data = item.data;
    if (!isRecord(data)) return item;
    const next = data.next;
    if (!isRecord(next)) return item;
    const patchedNext = Object.fromEntries(
      Object.entries(next).map(([k, v]) => {
        if (!isRecord(v)) return [k, v];
        const query = v.query;
        if (!isRecord(query)) return [k, v];
        return [k, { ...v, query: { ...query, ...annotation } }];
      })
    );
    return { ...item, data: { ...data, next: patchedNext } };
  });
  return { ...sc, results: patched };
}

export function withMaterializationHints<T extends HintableToolResult>(
  result: T,
  materialized: RemoteMaterialization
): T {
  const structuredRecord = isRecord(result.structuredContent)
    ? result.structuredContent
    : { data: result.structuredContent };
  const withLocation = {
    ...structuredRecord,
    location: locationPayload(materialized.location),
  };
  const structuredContent = annotateNextPointers(
    withLocation,
    remoteAnnotation(materialized)
  );
  const locationBlock = formatMaterializationHints(materialized);
  const content = result.content?.map(item =>
    item.type === 'text' && typeof item.text === 'string'
      ? { ...item, text: `${item.text.trimEnd()}\n${locationBlock}` }
      : item
  );

  return {
    ...result,
    structuredContent,
    ...(content ? { content } : {}),
  };
}
