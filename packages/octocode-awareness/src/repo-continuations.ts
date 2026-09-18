import type { AwarenessQueryParams, AwarenessQueryView, QueryContinuationState } from './repo-model.js';

/** Preserve executable read scope as a bounded query expands. */
export function queryContinuation(_database: string, _workspace: string | null, view: AwarenessQueryView,
  params: AwarenessQueryParams, requestedLimit: number, partial: boolean): QueryContinuationState {
  if (!partial) return {};
  if (view === 'agents') {
    const offset = params.offset == null || !Number.isFinite(params.offset)
      ? 0
      : Math.max(0, Math.floor(params.offset));
    return { next: { list: { operation: 'work.list', params: {
      kind: 'agents',
      limit: requestedLimit,
      offset: offset + requestedLimit,
      ...(params.artifact ? { artifact: params.artifact } : {}),
      ...(params.repo ? { repo: params.repo } : {}),
      ...(params.ref ? { ref: params.ref } : {}),
      ...(params.query ? { query: params.query } : {}),
    } } } };
  }
  const cap = view === 'workboard' ? 50 : 500;
  if (requestedLimit >= cap) return { terminal_limit: { code: 'QUERY_VIEW_LIMIT', view, limit: cap } };
  if (view !== 'workboard') return { terminal_limit: { code: 'QUERY_VIEW_LIMIT', view, limit: cap } };
  const common = {
    kind: view,
    limit: Math.min(cap, requestedLimit * 2),
    ...(params.artifact ? { artifact: params.artifact } : {}),
    ...(params.repo ? { repo: params.repo } : {}),
    ...(params.ref ? { ref: params.ref } : {}),
    ...(params.query ? { query: params.query } : {}),
  };
  return { next: { list: { operation: 'work.list', params: {
    ...common,
    ...(params.state ? { state: params.state } : {}),
    ...(params.label ? { label: params.label } : {}),
    ...(params.file ? { file: params.file } : {}),
    ...(params.since ? { since: params.since } : {}),
    ...(params.includeBodies ? { include_bodies: true } : {}),
  } } } };
}
