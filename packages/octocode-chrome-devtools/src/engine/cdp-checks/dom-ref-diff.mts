// Refs an action revealed (menu items, dialog fields), appended to the target's snapshot so the
// next step can use them without a new full snapshot.
import { writeFileSync } from 'fs';
import { join } from 'path';
import type { Session } from '../cdp-connection.mjs';
import type { RefUnit, SnapshotRef } from './dom-types.mjs';
import {
  readTargetSnapshot,
  recordTargetSnapshot,
} from './snapshot-resource.mjs';

function targetSnapshotData(cdp: Session): {
  refs: Record<string, SnapshotRef>;
} {
  const snap = readTargetSnapshot(cdp);
  const data =
    snap && snap.data?.targetId === cdp.targetInfo.id
      ? snap.data
      : {
          targetId: cdp.targetInfo.id,
          url: cdp.targetInfo.url,
          refs: {},
          regions: {},
        };
  data.refs ??= {};
  return data;
}

/** Names a node by its existing ref, or adds the next free ref for it. */
function refNamer(refs: Record<string, SnapshotRef>) {
  let next =
    Math.max(0, ...Object.keys(refs).map(k => Number(k.slice(1)) || 0)) + 1;
  const known = new Map(
    Object.entries(refs).map(([k, v]) => [v.backendDOMNodeId, k])
  );
  return (u: RefUnit) => {
    let ref = known.get(u.backendDOMNodeId);
    if (!ref) {
      ref = `e${next++}`;
      refs[ref] = {
        backendDOMNodeId: u.backendDOMNodeId,
        role: u.role,
        name: u.fullName,
        frame: u.frame,
      };
    }
    return ref;
  };
}

function newRefLine(ref: string, u: RefUnit) {
  const role =
    u.role === 'heading' ? (u.level ? `h${u.level}` : 'heading') : u.role;
  return `[NEW] [${ref}] ${role}${u.name ? ` "${u.name}"` : ''}`;
}

/**
 * Compares the useful refs before and after the steps, records the added ones in
 * page-snapshot.json, and returns { added, gone, lines }; null when nothing changed.
 */
export function recordNewRefs(
  cdp: Session,
  before: RefUnit[],
  after: RefUnit[]
) {
  const had = new Set(before.map(u => u.backendDOMNodeId));
  const added = after.filter(u => !had.has(u.backendDOMNodeId));
  const nowIds = new Set(after.map(u => u.backendDOMNodeId));
  const gone = before.filter(u => !nowIds.has(u.backendDOMNodeId)).length;
  if (!added.length && !gone) return null;
  const data = targetSnapshotData(cdp);
  const name = refNamer(data.refs);
  const lines = added.map(u => newRefLine(name(u), u));
  const path = join(cdp.outputDir, 'page-snapshot.json');
  writeFileSync(path, `${JSON.stringify(data, null, 2)}\n`, { mode: 0o600 });
  recordTargetSnapshot(cdp, {
    type: 'page-snapshot',
    targetUrl: cdp.targetInfo.url,
    refCount: Object.keys(data.refs).length,
    artifactPath: path,
  });
  return { added: added.length, gone, lines };
}
