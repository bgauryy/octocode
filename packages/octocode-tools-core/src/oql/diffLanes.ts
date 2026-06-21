/**
 * Single source of truth for the `target:"diff"` lane discriminant.
 *
 * `target:"diff"` has two typed execution lanes plus an invalid shape:
 *   - prPatch:    { prNumber, files? }        -> ghHistoryResearch (PR patches)
 *   - directFile: { baseRef, headRef, path }  -> 2× ghGetFileContent + line diff
 *   - neither:    repair diagnostic, no call
 *
 * Both the planner (to announce the right backend in `--explain --dry-run`) and
 * the diff adapter (to dispatch execution) classify a query off this one
 * helper, so the plan can never contradict execution. Pure: no imports beyond
 * its own return types.
 */

export type DiffLane =
  | { kind: 'prPatch'; prNumber: number; files?: string[] }
  | { kind: 'directFile'; baseRef: string; headRef: string; path: string }
  | { kind: 'neither' };

/** Unified lane discriminant for target:"diff". Single source of truth. */
export function classifyDiffLane(
  params: Record<string, unknown> | undefined
): DiffLane {
  const p = params ?? {};
  if (p.prNumber !== undefined && p.prNumber !== null) {
    return {
      kind: 'prPatch',
      prNumber: p.prNumber as number,
      ...(Array.isArray(p.files) ? { files: p.files as string[] } : {}),
    };
  }
  if (
    typeof p.baseRef === 'string' &&
    typeof p.headRef === 'string' &&
    typeof p.path === 'string'
  ) {
    return {
      kind: 'directFile',
      baseRef: p.baseRef,
      headRef: p.headRef,
      path: p.path,
    };
  }
  return { kind: 'neither' };
}

/** Backend name the planner should announce for a lane (`''` = no call). */
export function diffLaneBackend(lane: DiffLane): string {
  switch (lane.kind) {
    case 'prPatch':
      return 'ghHistoryResearch';
    case 'directFile':
      return 'ghGetFileContent';
    case 'neither':
      return '';
  }
}
