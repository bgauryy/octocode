// Where a plan step runs: the attached target, an explicit flattened session, or a child frame
// (an isolated iframe target, or a same-process frame selected by its owner element).
import { pageOwner } from '../target-owner.mjs';
import { setTimeout as sleep } from 'node:timers/promises';

/** A connection view that routes commands and events through one flattened session. */
export function scopedSession(cdp, target, sessionId) {
  const listeners = new Map();
  return {
    ...cdp,
    targetInfo: { ...target, id: target.targetId, sessionId },
    async foreground() {
      if (cdp.targetInfo.type === 'browser') {
        const { targetInfo } = await cdp.send(
          'Target.getTargetInfo',
          {},
          sessionId
        );
        const { targetInfos } = await cdp.send('Target.getTargets');
        const owner = pageOwner(targetInfos, targetInfo);
        if (!owner)
          throw new Error(
            'Cannot resolve the top-level page owner for this session; select the exact parent page target and scope its child frame'
          );
        return cdp.send('Target.activateTarget', { targetId: owner.targetId });
      }
      // Explicit sessions can belong to OOPIFs; foregrounding is parent-owned
      // unless the scoped target is known to be a top-level page.
      return target.type === 'page'
        ? cdp.send('Page.bringToFront', {}, sessionId)
        : cdp.foreground();
    },
    send(method, params = {}, childSession) {
      return cdp.send(method, params, childSession ?? sessionId);
    },
    on(event, fn) {
      // The handler's result is kept: the connection reports rejected async handlers.
      const wrapped = (params, meta: { sessionId?: string } = {}) =>
        meta.sessionId === sessionId ? fn(params, meta) : undefined;
      listeners.set(fn, { event, wrapped });
      cdp.on(event, wrapped);
    },
    off(event, fn) {
      const entry = listeners.get(fn);
      if (entry) {
        cdp.off(event, entry.wrapped);
        listeners.delete(fn);
      }
    },
    dispose() {
      for (const entry of listeners.values())
        cdp.off(entry.event, entry.wrapped);
      listeners.clear();
    },
  };
}

function inlineScope(cdp, frameTree, executionContextId) {
  return {
    ...cdp,
    targetInfo: {
      ...cdp.targetInfo,
      frameId: frameTree.frame.id,
      url: frameTree.frame.url,
    },
    send(method, params = {}, sessionId) {
      if (sessionId) return cdp.send(method, params, sessionId);
      if (method === 'Runtime.evaluate')
        params = { ...params, contextId: executionContextId };
      if (method === 'Accessibility.getFullAXTree')
        params = { ...params, frameId: frameTree.frame.id };
      if (method === 'Page.getFrameTree') return Promise.resolve({ frameTree });
      return cdp.send(method, params);
    },
    dispose() {},
  };
}

const findFrame = (tree, frameId) =>
  tree.frame.id === frameId
    ? tree
    : (tree.childFrames || [])
        .map(child => findFrame(child, frameId))
        .find(Boolean);

// A same-process frame, evaluated in an isolated world of the main connection.
async function selectorFrameScope(cdp, selector) {
  await cdp.send('DOM.enable');
  await cdp.send('Page.enable');
  const { root } = await cdp.send('DOM.getDocument');
  const { nodeIds } = await cdp.send('DOM.querySelectorAll', {
    nodeId: root.nodeId,
    selector,
  });
  if (nodeIds.length !== 1)
    throw new Error(
      `Frame selector needs exactly one match; found ${nodeIds.length}`
    );
  const { node } = await cdp.send('DOM.describeNode', { nodeId: nodeIds[0] });
  if (!node.frameId)
    throw new Error('Frame selector did not match a frame owner');
  const { frameTree } = await cdp.send('Page.getFrameTree');
  const selected = findFrame(frameTree, node.frameId);
  if (!selected)
    throw new Error(
      'Frame is outside this session; use isolated frame id or URL'
    );
  const { executionContextId } = await cdp.send('Page.createIsolatedWorld', {
    frameId: node.frameId,
    worldName: 'octocode-browser-execute',
  });
  return inlineScope(cdp, selected, executionContextId);
}

/** The attached target and every target nested below it. */
function relatedTargets(targetInfos, rootId) {
  const related = new Set([rootId]);
  for (let changed = true; changed;) {
    changed = false;
    for (const target of targetInfos)
      if (related.has(target.parentId) && !related.has(target.targetId)) {
        related.add(target.targetId);
        changed = true;
      }
  }
  return related;
}

async function matchingFrames(cdp, frame) {
  const { targetInfos } = await cdp.send('Target.getTargets');
  const related = relatedTargets(targetInfos, cdp.targetInfo.id);
  const matches = targetInfos.filter(
    t =>
      t.type === 'iframe' &&
      related.has(t.targetId) &&
      (!frame.id || t.targetId === frame.id) &&
      (!frame.url || t.url.includes(frame.url))
  );
  if (matches.length > 1)
    throw new Error(
      `Ambiguous iframe target; select a candidate with frame.id or a unique frame.url: ${JSON.stringify(matches)}`
    );
  return matches;
}

async function attachFrame(cdp, target, scopes) {
  if (scopes.has(target.targetId)) return scopes.get(target.targetId).scope;
  const { sessionId } = await cdp.send('Target.attachToTarget', {
    targetId: target.targetId,
    flatten: true,
  });
  const scope = scopedSession(cdp, target, sessionId);
  scopes.set(target.targetId, { scope, sessionId });
  return scope;
}

/** The scope for a step's frame; isolated iframe targets are polled until timeoutMs and attached once. */
export async function frameScope(cdp, frame, { timeoutMs, scopes, log }) {
  if (frame.selector) return selectorFrameScope(cdp, frame.selector);
  const started = Date.now();
  let nextProgress = 0;
  for (;;) {
    const matches = await matchingFrames(cdp, frame);
    if (matches.length === 1) return attachFrame(cdp, matches[0], scopes);
    if (Date.now() - started >= timeoutMs)
      throw new Error('Iframe target not found within timeout');
    if (Date.now() - started >= nextProgress) {
      log(`[PROGRESS] waiting iframe elapsedMs=${Date.now() - started}`);
      nextProgress += 1000;
    }
    await sleep(100);
  }
}
