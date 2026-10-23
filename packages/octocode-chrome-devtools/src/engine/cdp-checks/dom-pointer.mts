// Trusted CDP input for one located element: pointer approach, clicks, drag, keys, typing and uploads.
import { isAbsolute, resolve } from 'path';
import {
  buildTypingEvents,
  buildKeyPressEvents,
  buildScrollEvents,
  runEventSequence,
} from '../human-input.mjs';
import { DONE } from './dom-config.mjs';
import { SELECT_ALL_FN } from './dom-page-scripts.mjs';
import { resolveRefEntry } from './dom-locate.mjs';
import { setTimeout as sleep } from 'node:timers/promises';

const mouse = (cdp, params) => cdp.send('Input.dispatchMouseEvent', params);

// Main-frame viewport coordinates; also right inside same-process iframes, where
// getBoundingClientRect is frame-local.
async function centerOf(
  cdp,
  target,
  fallbackRect?: { x: number; y: number; width: number; height: number }
) {
  const quads = target
    ? await cdp.send('DOM.getContentQuads', target).catch(() => null)
    : null;
  const q = quads?.quads?.[0];
  if (q) {
    const xs = [q[0], q[2], q[4], q[6]];
    const ys = [q[1], q[3], q[5], q[7]];
    const x = Math.min(...xs);
    const y = Math.min(...ys);
    const width = Math.max(...xs) - x;
    const height = Math.max(...ys) - y;
    return {
      cx: Math.round(x + width / 2),
      cy: Math.round(y + height / 2),
      rect: { x, y, width, height },
    };
  }
  if (!fallbackRect) throw new Error('element has no box (not rendered)');
  const r = fallbackRect;
  return {
    cx: Math.round(r.x + r.width / 2),
    cy: Math.round(r.y + r.height / 2),
    rect: r,
  };
}

async function dropPoint(cdp, step) {
  if (step.toRef)
    return centerOf(cdp, {
      backendNodeId: resolveRefEntry(cdp, step.toRef).backendDOMNodeId,
    });
  const { root } = await cdp.send('DOM.getDocument', { depth: 0 });
  const { nodeId } = await cdp.send('DOM.querySelector', {
    nodeId: root.nodeId,
    selector: step.toSelector,
  });
  if (!nodeId)
    throw new Error(`drag destination ${step.toSelector} matched nothing`);
  return centerOf(cdp, { nodeId });
}

async function dragPath(cdp, from, to, dragData) {
  await mouse(cdp, { type: 'mouseMoved', x: from.cx, y: from.cy });
  await mouse(cdp, {
    type: 'mousePressed',
    x: from.cx,
    y: from.cy,
    button: 'left',
    buttons: 1,
    clickCount: 1,
  });
  const n = 12;
  for (let i = 1; i <= n; i++) {
    const x = Math.round(from.cx + ((to.cx - from.cx) * i) / n);
    const y = Math.round(from.cy + ((to.cy - from.cy) * i) / n);
    await mouse(cdp, { type: 'mouseMoved', x, y, button: 'left', buttons: 1 });
    await sleep(16);
  }
  const data = dragData();
  if (data)
    for (const type of ['dragEnter', 'dragOver', 'drop'])
      await cdp.send('Input.dispatchDragEvent', {
        type,
        x: to.cx,
        y: to.cy,
        data,
      });
  await mouse(cdp, {
    type: 'mouseReleased',
    x: to.cx,
    y: to.cy,
    button: 'left',
    buttons: 0,
    clickCount: 1,
  });
}

// Pointer drag; native HTML5 drag-and-drop is intercepted and replayed as dragEnter/dragOver/drop.
async function performDrag(cdp, from, to) {
  let dragData = null;
  const onDrag = ({ data }) => {
    dragData = data;
  };
  cdp.on('Input.dragIntercepted', onDrag);
  await cdp.send('Input.setInterceptDrags', { enabled: true }).catch(() => {});
  try {
    await dragPath(cdp, from, to, () => dragData);
  } finally {
    await cdp
      .send('Input.setInterceptDrags', { enabled: false })
      .catch(() => {});
    cdp.off('Input.dragIntercepted', onDrag);
  }
  return dragData ? 'html5' : 'pointer';
}

// Real CDP input: isTrusted events with mousedown/up, keydown/up, hover.
// The page keeps the last trusted mouse position, so the next approach starts where the pointer is.
async function rememberMouse(cdp, x, y) {
  await cdp
    .send('Runtime.evaluate', {
      expression: `globalThis.__octoMouse = { x: ${x}, y: ${y} }`,
    })
    .catch(() => {});
}

// Is the element under (x, y) the target or inside it? Unknown (e.g. cross-frame) counts as yes.
async function hitsTarget(cdp, objectId, x, y) {
  const loc = await cdp
    .send('DOM.getNodeForLocation', {
      x: Math.round(x),
      y: Math.round(y),
      includeUserAgentShadowDOM: true,
      ignorePointerEventsNone: true,
    })
    .catch(() => null);
  const hit = loc?.backendNodeId
    ? await cdp
        .send('DOM.resolveNode', { backendNodeId: loc.backendNodeId })
        .catch(() => null)
    : null;
  if (!hit?.object?.objectId) return true;
  const res = await cdp
    .send('Runtime.callFunctionOn', {
      objectId,
      functionDeclaration:
        'function (h) { return this === h || this.contains(h) || h.control === this || (h.closest && h.closest("label")?.control === this); }',
      arguments: [{ objectId: hit.object.objectId }],
      returnByValue: true,
    })
    .catch(() => null);
  return res?.result?.value ?? true;
}

// Move directly to the observed point, then re-aim with straight moves until the point under
// the mouse is the target (a hover effect can shift layout).
async function approach(cdp, start, { target, objectId, details }) {
  let at = start;
  await mouse(cdp, { type: 'mouseMoved', x: at.cx, y: at.cy });
  for (let attempt = 0; attempt < 3; attempt++) {
    const again = (await centerOf(cdp, target, null)) ?? at;
    const moved = Math.abs(again.cx - at.cx) + Math.abs(again.cy - at.cy) > 2;
    if (
      !moved &&
      (!objectId || (await hitsTarget(cdp, objectId, again.cx, again.cy)))
    )
      break;
    details.reaimed = true;
    await mouse(cdp, { type: 'mouseMoved', x: again.cx, y: again.cy });
    await sleep(50);
    at = again;
  }
  return at;
}

async function click(cdp, { cx, cy }, clickCount) {
  const at = { x: cx, y: cy, button: 'left' };
  await mouse(cdp, { type: 'mousePressed', ...at, buttons: 1, clickCount });
  await mouse(cdp, { type: 'mouseReleased', ...at, buttons: 0, clickCount });
}

async function dispatchAction(cdp, step, details, at) {
  const action = step.action;
  if (action === 'hover') return; // already there
  if (action === 'press')
    await runEventSequence(cdp, buildKeyPressEvents(step.key));
  else if (action === 'scroll')
    await runEventSequence(
      cdp,
      buildScrollEvents(at.cx, at.cy, Number(step.value))
    );
  else if (action === 'drag') {
    const to = await dropPoint(cdp, step);
    details.dragKind = await performDrag(cdp, at, to);
    await rememberMouse(cdp, to.cx, to.cy);
  } else
    for (const clickCount of action === 'dblclick' ? [1, 2] : [1])
      await click(cdp, at, clickCount);
}

/** Selects the field's text and replaces it; false when the click did not focus it. */
async function enterText(cdp, step, details, objectId) {
  const focused = objectId
    ? (
        await cdp
          .send('Runtime.callFunctionOn', {
            objectId,
            functionDeclaration: SELECT_ALL_FN,
            returnByValue: true,
          })
          .catch(() => null)
      )?.result?.value
    : false;
  if (!focused) return false;
  const hadText = (details.before?.value ?? '') !== '';
  if (hadText || step.value === '')
    await runEventSequence(cdp, buildKeyPressEvents('Delete'));
  if (step.value === '') return true;
  if (step.action === 'fill')
    await cdp.send('Input.insertText', { text: step.value });
  else
    await runEventSequence(
      cdp,
      buildTypingEvents(step.value, { instant: true })
    );
  return true;
}

async function upload(cdp, step, objectId) {
  const files = step.value
    .split('|')
    .map(f => f.trim())
    .filter(Boolean)
    .map(f => (isAbsolute(f) ? f : resolve(f)));
  if (!files.length) return 'no-files';
  await cdp.send('DOM.setFileInputFiles', { files, objectId });
  return DONE.upload;
}

/** Performs a pending step with trusted input; returns its operation (DONE on success). */
export async function performTrusted(cdp, step, details, objectId) {
  const action = step.action;
  if (action === 'upload') return upload(cdp, step, objectId);
  const target = objectId ? { objectId } : null;
  let at = await centerOf(cdp, target, details.bbox);
  if (!['press', 'scroll', 'drag'].includes(action))
    at = await approach(cdp, at, { target, objectId, details });
  await rememberMouse(cdp, at.cx, at.cy);
  await dispatchAction(cdp, step, details, at);
  if (
    (action === 'fill' || action === 'type') &&
    !(await enterText(cdp, step, details, objectId))
  )
    return 'not-focused';
  return DONE[action];
}
