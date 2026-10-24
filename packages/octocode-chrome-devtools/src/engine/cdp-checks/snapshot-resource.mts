import { readFileSync } from 'node:fs';
import type { Payload } from '../cdp-connection.mjs';

/** What snapshot resources need from a connection: its target and, under the runner, the resource map. */
type SnapshotSession = {
  targetInfo: Payload;
  resourcesFile?: string;
  upsertResourceMap?(key: string, details: Record<string, unknown>): unknown;
};

function key(cdp: SnapshotSession) {
  const id = cdp.targetInfo?.id;
  if (typeof id !== 'string' || !id)
    throw new Error('Snapshot resources require a target ID');
  return `page-snapshot:${id}`;
}

export function readTargetSnapshot(cdp: SnapshotSession) {
  try {
    const map = JSON.parse(readFileSync(cdp.resourcesFile ?? '', 'utf8'));
    const path = map.resources?.[key(cdp)]?.artifactPath;
    if (!path) return null;
    const data = JSON.parse(readFileSync(path, 'utf8'));
    return data.targetId === cdp.targetInfo.id ? { path, data } : null;
  } catch {
    return null;
  }
}

export function recordTargetSnapshot(
  cdp: SnapshotSession,
  details: Record<string, unknown>
) {
  return cdp.upsertResourceMap?.(key(cdp), {
    ...details,
    targetId: cdp.targetInfo.id,
  });
}
