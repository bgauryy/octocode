import { readFileSync } from 'node:fs';

function key(cdp) {
  const id = cdp.targetInfo?.id;
  if (typeof id !== 'string' || !id)
    throw new Error('Snapshot resources require a target ID');
  return `page-snapshot:${id}`;
}

export function readTargetSnapshot(cdp) {
  try {
    const map = JSON.parse(readFileSync(cdp.resourcesFile, 'utf8'));
    const path = map.resources?.[key(cdp)]?.artifactPath;
    if (!path) return null;
    const data = JSON.parse(readFileSync(path, 'utf8'));
    return data.targetId === cdp.targetInfo.id ? { path, data } : null;
  } catch {
    return null;
  }
}

export function recordTargetSnapshot(cdp, details) {
  return cdp.upsertResourceMap?.(key(cdp), {
    ...details,
    targetId: cdp.targetInfo.id,
  });
}
