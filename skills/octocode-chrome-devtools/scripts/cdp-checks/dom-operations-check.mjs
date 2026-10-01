import { writeFileSync, readFileSync } from 'fs';
import { join, resolve, isAbsolute } from 'path';
import { pathToFileURL } from 'url';

const helper = (name) => import(pathToFileURL(resolve(process.cwd(), '.octocode', name)).href);
const { ACTIONABILITY_HELPERS_JS, waitForPageReady } = await helper('dom-actionability.mjs');
const { buildElementClickSequence, buildMouseMoveEvents, buildTypingEvents, buildKeyPressEvents, buildScrollEvents, runEventSequence } = await helper('human-input.mjs');
const { collectRefs } = await helper('ax-snapshot.mjs');

// Env (one step):
//   DOM_REF | DOM_SELECTOR  target (snapshot ref, or CSS; default first control)
//   DOM_ACTION    inspect | click | dblclick | fill | type | press | hover | select | check | uncheck |
//                 focus | scroll | upload | drag | wait
//   DOM_VALUE     fill/type text, select option, scroll deltaY, upload paths ('|'-separated), wait text
//   DOM_KEY       press: Enter, Tab, Escape, ArrowDown, a, Control+A, Shift+Tab…
//   DOM_TO_REF | DOM_TO_SELECTOR  drag destination
// Env (batch): DOM_STEPS='[{"ref":"e1","action":"fill","value":"a"},{"ref":"e4","action":"click"}]'
//   (step keys: ref, selector, action, value, key, toRef, toSelector); stops at the first failed step.
// Shared:
//   DOM_WAIT_TEXT text that must appear after the step(s) ('|' = any of); DOM_WAIT_MS (8000)
//   DOM_INPUT     trusted (default: real CDP mouse/keyboard, isTrusted=true) | js (DOM calls)
//   DOM_SETTLE_MS wait before verifying (default 500)
//   DOM_DIALOG    dismiss (default) | accept, for alert/confirm/prompt opened by the action
//   DOM_DIFF=0    skip the after-action [NEW] ref diff
const MODE = process.env.DOM_INPUT === 'js' ? 'js' : 'trusted';
const STABILITY_MS = Number.parseInt(process.env.DOM_STABILITY_MS ?? '150', 10);
const SETTLE_MS = Number.parseInt(process.env.DOM_SETTLE_MS ?? '500', 10);
const WAIT_MS = Number.parseInt(process.env.DOM_WAIT_MS ?? '8000', 10);
const WAIT_TEXT = process.env.DOM_WAIT_TEXT ?? '';
const ACCEPT_DIALOG = process.env.DOM_DIALOG === 'accept';
const DIFF = process.env.DOM_DIFF !== '0';
const DEFAULT_SELECTOR = 'button, [role="button"], input, textarea, select, a[href]';
const ACTIONS = ['inspect', 'click', 'dblclick', 'fill', 'type', 'press', 'hover', 'select', 'check', 'uncheck', 'focus', 'scroll', 'upload', 'drag', 'wait'];
const DONE = {
  click: 'clicked', dblclick: 'double-clicked', fill: 'filled', type: 'typed', press: 'pressed', hover: 'hovered', select: 'selected',
  check: 'checked', uncheck: 'unchecked', focus: 'focused', scroll: 'scrolled', upload: 'uploaded', drag: 'dragged', wait: 'waited',
};

function readSteps() {
  let steps;
  if (process.env.DOM_STEPS) {
    try { steps = JSON.parse(process.env.DOM_STEPS); } catch (error) { throw new Error(`DOM_STEPS is not valid JSON: ${error.message}`); }
    if (!Array.isArray(steps) || !steps.length) throw new Error('DOM_STEPS must be a non-empty JSON array of steps');
  } else {
    steps = [{
      ref: process.env.DOM_REF, selector: process.env.DOM_SELECTOR, action: process.env.DOM_ACTION ?? 'inspect',
      value: process.env.DOM_VALUE, key: process.env.DOM_KEY, toRef: process.env.DOM_TO_REF, toSelector: process.env.DOM_TO_SELECTOR,
    }];
  }
  return steps.map((s, i) => {
    const step = {
      ref: s.ref || '', selector: s.selector || (s.ref ? '' : DEFAULT_SELECTOR), action: s.action ?? 'inspect',
      value: s.value == null ? '' : String(s.value), key: s.key || 'Enter', toRef: s.toRef || '', toSelector: s.toSelector || '', index: i + 1,
    };
    if (!ACTIONS.includes(step.action)) throw new Error(`Unsupported action "${step.action}" (step ${step.index}). Use ${ACTIONS.join(', ')}.`);
    if (step.action === 'press') buildKeyPressEvents(step.key); // fail fast on an unknown key
    if (step.action === 'drag' && !step.toRef && !step.toSelector) throw new Error('drag needs DOM_TO_REF or DOM_TO_SELECTOR (toRef/toSelector in DOM_STEPS)');
    if (step.action === 'wait' && !step.value && !WAIT_TEXT) throw new Error('wait needs DOM_VALUE (or DOM_WAIT_TEXT): text to wait for, "|" = any of');
    return step;
  });
}
const STEPS = readSteps();

// Page-side helpers shared by the act and verify phases.
const STATE_JS = `
  function shortText(value) { return String(value ?? '').replace(/\\s+/g, ' ').trim().slice(0, 160); }
  function describeEl(el) {
    if (!el || el === el.ownerDocument.body || el === el.ownerDocument.documentElement) return null;
    return (el.localName + (el.id ? '#' + el.id : '') + (el.getAttribute('name') ? '[name=' + el.getAttribute('name') + ']' : '')).slice(0, 80);
  }
  function stateOf(el) {
    const s = {};
    if (el.isContentEditable) s.value = el.innerText.replace(/\\n$/, '').slice(0, 300);
    else if (el.type === 'file') s.files = [...(el.files ?? [])].map(f => f.name).join(', ');
    else if ('value' in el && !['button', 'li', 'option'].includes(el.localName) && el.type !== 'checkbox' && el.type !== 'radio') s.value = String(el.value).slice(0, 300);
    if (el.type === 'checkbox' || el.type === 'radio') s.checked = el.checked;
    else if (el.hasAttribute('aria-checked')) s.checked = el.getAttribute('aria-checked') === 'true';
    if (el.localName === 'select') s.selected = [...el.selectedOptions].map(o => o.label.trim()).join(', ');
    if (el.hasAttribute('aria-expanded')) s.expanded = el.getAttribute('aria-expanded') === 'true';
    return s;
  }
  function openDialogs() {
    return [...document.querySelectorAll('dialog[open], [role=dialog], [role=alertdialog], [aria-modal=true]')]
      .filter(d => { const r = d.getBoundingClientRect(); return r.width && r.height; }).length;
  }
`;

const CORE_BODY_JS = `
  ${ACTIONABILITY_HELPERS_JS}
  ${STATE_JS}
  const cssEscape = globalThis.CSS?.escape ?? ((value) => String(value).replace(/[^a-zA-Z0-9_-]/g, '\\$&'));
  function elementPath(element) {
    const parts = [];
    let node = element;
    while (node && node.nodeType === Node.ELEMENT_NODE && parts.length < 8) {
      const tag = node.localName;
      const id = node.id ? '#' + cssEscape(node.id) : '';
      const testId = node.getAttribute('data-testid') ? '[data-testid="' + node.getAttribute('data-testid').replace(/"/g, '\\\\"') + '"]' : '';
      let nth = '';
      if (!id && !testId && node.parentElement) {
        const siblings = [...node.parentElement.children].filter(child => child.localName === tag);
        if (siblings.length > 1) nth = ':nth-of-type(' + (siblings.indexOf(node) + 1) + ')';
      }
      parts.unshift(tag + id + testId + nth);
      const root = node.getRootNode();
      if (root instanceof ShadowRoot) { parts.unshift('::shadow'); node = root.host; } else { node = node.parentElement; }
    }
    return parts.join(' > ');
  }
  function accessibleNameGuess(element) {
    const labelledBy = element.getAttribute('aria-labelledby');
    if (labelledBy) {
      const text = labelledBy.split(/\\s+/).map(id => element.ownerDocument.getElementById(id)?.textContent ?? '').join(' ');
      if (shortText(text)) return shortText(text);
    }
    const aria = element.getAttribute('aria-label');
    if (aria) return shortText(aria);
    if (element.labels?.length) return shortText([...element.labels].map((label) => { const c = label.cloneNode(true); c.querySelectorAll('select,textarea,input,option').forEach((n) => n.remove()); return c.textContent; }).join(' '));
    if (element.alt) return shortText(element.alt);
    if (element.title) return shortText(element.title);
    if (element.placeholder) return shortText(element.placeholder);
    return shortText(element.innerText || element.textContent || element.value);
  }
  async function stableRect(element, stabilityMs) {
    const first = element.getBoundingClientRect();
    await new Promise(resolve => setTimeout(resolve, stabilityMs));
    const second = element.getBoundingClientRect();
    const delta = Math.abs(first.x - second.x) + Math.abs(first.y - second.y) + Math.abs(first.width - second.width) + Math.abs(first.height - second.height);
    return { stable: delta < 1, first, second };
  }
  function isEditable(el) {
    if (el.isContentEditable) return true;
    if (el.localName === 'textarea') return true;
    if (el.localName !== 'input') return false;
    return !['checkbox', 'radio', 'button', 'submit', 'reset', 'file', 'image', 'range', 'color', 'hidden'].includes((el.type || 'text').toLowerCase());
  }
  function isCheckable(el) {
    return el.type === 'checkbox' || el.type === 'radio' || ['checkbox', 'radio', 'switch', 'menuitemcheckbox'].includes(el.getAttribute('role'));
  }
  function setNativeValue(el, value) {
    if (el.isContentEditable) { el.textContent = value; return; }
    // React tracks the instance setter; the prototype setter makes onChange fire.
    const win = el.ownerDocument.defaultView ?? globalThis;
    const proto = el instanceof win.HTMLTextAreaElement ? win.HTMLTextAreaElement.prototype : el instanceof win.HTMLInputElement ? win.HTMLInputElement.prototype : null;
    const setter = proto ? Object.getOwnPropertyDescriptor(proto, 'value')?.set : null;
    if (setter) setter.call(el, value); else el.value = value;
  }
  async function checkElement(element, action, value, stabilityMs, mode, key) {
    globalThis.__octoTarget = element;
    if (!globalThis.__octoMut) {
      globalThis.__octoMut = { n: 0 };
      new MutationObserver(list => { globalThis.__octoMut.n += list.length; })
        .observe(document, { subtree: true, childList: true, attributes: true, characterData: true });
    }
    if (action !== 'upload') element.scrollIntoView({ block: 'center', inline: 'center', behavior: 'instant' });
    const style = getComputedStyle(element);
    const rectCheck = await stableRect(element, stabilityMs);
    const rect = rectCheck.second;
    const doc = element.ownerDocument;
    const hit = doc.elementFromPoint(rect.left + rect.width / 2, rect.top + rect.height / 2);
    const coveredBy = hit && hit !== element && !element.contains(hit) && !hit.contains(element) ? elementPath(hit) : null;
    const details = {
      found: true, action, mode, location: location.href,
      tag: element.localName, path: elementPath(element), id: element.id || null,
      name: element.getAttribute('name'), type: element.getAttribute('type'),
      role: element.getAttribute('role') || element.localName,
      accessibleNameGuess: accessibleNameGuess(element),
      text: shortText(element.innerText || element.textContent),
      visible: isVisible(element, rect, style), disabled: isDisabled(element),
      stable: rectCheck.stable, covered: Boolean(coveredBy), coveredBy,
      bbox: { x: Math.round(rect.x), y: Math.round(rect.y), width: Math.round(rect.width), height: Math.round(rect.height) },
      style: { display: style.display, visibility: style.visibility, opacity: style.opacity, pointerEvents: style.pointerEvents, position: style.position, zIndex: style.zIndex },
      before: stateOf(element), mutationsBefore: globalThis.__octoMut.n,
      canOperate: false, operation: null, pending: false,
    };
    const pointer = ['click', 'dblclick', 'check', 'uncheck', 'hover', 'fill', 'type', 'drag'].includes(action);
    // File inputs are usually visually hidden behind a styled button; upload sets files directly.
    details.canOperate = action === 'scroll' || action === 'upload' || (details.visible && !details.disabled && (!pointer || (!details.covered && details.stable)));

    if (action === 'inspect') details.operation = 'inspected';
    else if (action === 'upload') {
      if (element.localName === 'input' && element.type === 'file') details.pending = true;
      else details.operation = 'not-file-input';
    }
    else if (!details.canOperate) details.operation = 'blocked-by-actionability';
    else if ((action === 'fill' || action === 'type') && !isEditable(element)) details.operation = 'not-fillable';
    else if ((action === 'check' || action === 'uncheck') && !isCheckable(element)) details.operation = 'not-checkable';
    else if (action === 'select') {
      if (element.localName !== 'select') details.operation = 'not-selectable';
      else {
        const want = String(value).trim();
        const opts = [...element.options];
        const opt = opts.find(o => o.label.trim() === want) || opts.find(o => o.value === want) || opts.find(o => o.label.trim().toLowerCase() === want.toLowerCase());
        if (!opt) { details.operation = 'option-not-found'; details.options = opts.slice(0, 20).map(o => o.label.trim()); }
        else {
          element.focus();
          element.selectedIndex = opt.index;
          element.dispatchEvent(new Event('input', { bubbles: true }));
          element.dispatchEvent(new Event('change', { bubbles: true }));
          details.operation = 'selected';
        }
      }
    } else if ((action === 'check' || action === 'uncheck') && stateOf(element).checked === (action === 'check')) {
      details.operation = 'already-' + action + 'ed';
    } else if (action === 'focus') { element.focus(); details.operation = 'focused'; }
    else if (action === 'scroll' && !Number(value)) details.operation = 'scrolled';
    else if (mode === 'trusted' || action === 'drag') { if (action === 'press') element.focus(); details.pending = true; }
    else if (action === 'click' || action === 'check' || action === 'uncheck') { element.click(); details.operation = action === 'click' ? 'clicked' : action + 'ed'; }
    else if (action === 'dblclick') {
      element.click(); element.click();
      element.dispatchEvent(new MouseEvent('dblclick', { bubbles: true, composed: true, detail: 2 }));
      details.operation = 'double-clicked';
    } else if (action === 'fill' || action === 'type') {
      element.focus();
      setNativeValue(element, value);
      element.dispatchEvent(new InputEvent('input', { bubbles: true, inputType: 'insertText', data: value }));
      element.dispatchEvent(new Event('change', { bubbles: true }));
      details.operation = action === 'fill' ? 'filled' : 'typed';
    } else if (action === 'hover') {
      for (const type of ['pointerover', 'pointerenter', 'mouseover', 'mouseenter', 'mousemove']) {
        element.dispatchEvent(new MouseEvent(type, { bubbles: !type.endsWith('enter'), composed: true }));
      }
      details.operation = 'hovered';
    } else if (action === 'press') {
      element.focus();
      const k = String(key).split('+').pop();
      for (const type of ['keydown', 'keyup']) element.dispatchEvent(new KeyboardEvent(type, { key: k, bubbles: true, composed: true }));
      details.operation = 'pressed';
    } else if (action === 'scroll') { element.scrollBy?.(0, Number(value)); details.operation = 'scrolled'; }
    return details;
  }
`;

const SELECT_ALL_FN = `function() {
  const el = this;
  if (!el?.isConnected) return false;
  el.focus();
  const doc = el.ownerDocument;
  if (el.isContentEditable) {
    const range = doc.createRange();
    range.selectNodeContents(el);
    const sel = doc.getSelection(); sel.removeAllRanges(); sel.addRange(range);
  } else { try { el.select(); } catch { el.setSelectionRange?.(0, String(el.value).length); } }
  return doc.activeElement === el || el.contains(doc.activeElement);
}`;

const VERIFY_FN = `function() {
  ${STATE_JS}
  const el = this;
  const doc = el?.ownerDocument ?? document;
  const connected = Boolean(el?.isConnected);
  return { url: location.href, connected, state: connected ? stateOf(el) : null,
    mutations: globalThis.__octoMut?.n ?? 0, scrollY: Math.round(scrollY), focus: describeEl(doc.activeElement), dialogs: openDialogs() };
}`;

const PAGE_STATE_JS = `(() => ({ url: location.href, mutations: globalThis.__octoMut?.n ?? 0, scrollY: Math.round(scrollY) }))()`;

function loadSnapshot(cdp) {
  try {
    const resourceMap = JSON.parse(readFileSync(cdp.resourcesFile, 'utf8'));
    const path = resourceMap.resources?.['page-snapshot']?.artifactPath;
    return path ? { path, data: JSON.parse(readFileSync(path, 'utf8')) } : null;
  } catch {
    return null;
  }
}

// Refs come from the latest page-snapshot.json (or refs added by an earlier [NEW] diff).
function resolveRefEntry(cdp, ref) {
  const snap = loadSnapshot(cdp);
  if (!snap) throw new Error('No page-snapshot resource found for this session — run scripts/cdp-checks/page-snapshot.mjs on the same --port first.');
  const entry = snap.data.refs?.[ref];
  if (!entry) throw new Error(`Ref ${ref} not found in ${snap.path}. Available: ${Object.keys(snap.data.refs ?? {}).slice(0, 40).join(', ')}`);
  return entry;
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const valueOf = (res) => res?.result?.value ?? { found: false, error: res?.exceptionDetails?.exception?.description?.split('\n')[0] ?? 'No value returned' };

// Returns { label, details, objectId }; objectId is the element handle for geometry, files, verify.
async function locateAndCheck(cdp, step) {
  const args = [step.action, step.value, STABILITY_MS, MODE, step.key].map((v) => JSON.stringify(v)).join(', ');
  const fromEvaluate = async (label, expression) => {
    const details = valueOf(await cdp.send('Runtime.evaluate', { awaitPromise: true, returnByValue: true, expression }));
    const handle = details.found ? await cdp.send('Runtime.evaluate', { expression: 'globalThis.__octoTarget' }).catch(() => null) : null;
    return { label, details, objectId: handle?.result?.objectId ?? null };
  };
  if (!step.ref) {
    return fromEvaluate(`selector:${step.selector}`, `(async () => {
      ${CORE_BODY_JS}
      const element = document.querySelector(${JSON.stringify(step.selector)});
      if (!element) return { selector: ${JSON.stringify(step.selector)}, found: false };
      const details = await checkElement(element, ${args});
      details.selector = ${JSON.stringify(step.selector)};
      return details;
    })()`);
  }
  const label = `ref:${step.ref}`;
  const entry = resolveRefEntry(cdp, step.ref);
  let object = null;
  try {
    ({ object } = await cdp.send('DOM.resolveNode', { backendNodeId: entry.backendDOMNodeId }));
  } catch {
    // Client-rendered pages replace nodes after the snapshot; recover by role+name.
    console.log(`[FINDING] STALE_SNAPSHOT_REF ${step.ref}; recovering by role=${JSON.stringify(entry.role)} name=${JSON.stringify(entry.name)}`);
  }
  if (object?.objectId) {
    // callFunctionOn runs in the element's own realm, so iframe elements use the iframe's document.
    const details = valueOf(await cdp.send('Runtime.callFunctionOn', {
      objectId: object.objectId, awaitPromise: true, returnByValue: true,
      functionDeclaration: `async function() {
        ${CORE_BODY_JS}
        const details = await checkElement(this, ${args});
        details.ref = ${JSON.stringify(step.ref)};
        return details;
      }`,
    }));
    return { label, details, objectId: object.objectId };
  }
  return fromEvaluate(label, `(async () => {
    ${CORE_BODY_JS}
    const wantedRole = ${JSON.stringify(entry.role)};
    const wantedName = ${JSON.stringify(entry.name)};
    function semanticRole(element) {
      const explicit = element.getAttribute('role');
      if (explicit) return explicit;
      if (element.matches('a[href]')) return 'link';
      if (element.matches('button, input[type="button"], input[type="submit"], input[type="reset"]')) return 'button';
      if (/^h[1-6]$/.test(element.localName)) return 'heading';
      if (element.matches('input[type="checkbox"]')) return 'checkbox';
      if (element.matches('input[type="search"]')) return 'searchbox';
      if (element.matches('select')) return 'combobox';
      if (element.matches('input:not([type]), input[type="text"], input[type="email"], input[type="password"], textarea, [contenteditable="true"]')) return 'textbox';
      return element.localName;
    }
    const element = [...document.querySelectorAll('*')].find((candidate) =>
      semanticRole(candidate) === wantedRole && accessibleNameGuess(candidate) === wantedName);
    if (!element) return { ref: ${JSON.stringify(step.ref)}, found: false, error: 'stale ref and no current element has the captured role/name', recoveredFromStaleRef: false };
    const details = await checkElement(element, ${args});
    details.ref = ${JSON.stringify(step.ref)};
    details.recoveredFromStaleRef = true;
    return details;
  })()`);
}

// Main-frame viewport coordinates; also right inside same-process iframes, where
// getBoundingClientRect is frame-local.
async function centerOf(cdp, target, fallbackRect) {
  const quads = target ? await cdp.send('DOM.getContentQuads', target).catch(() => null) : null;
  const q = quads?.quads?.[0];
  if (q) {
    const xs = [q[0], q[2], q[4], q[6]];
    const ys = [q[1], q[3], q[5], q[7]];
    const x = Math.min(...xs);
    const y = Math.min(...ys);
    const width = Math.max(...xs) - x;
    const height = Math.max(...ys) - y;
    return { cx: Math.round(x + width / 2), cy: Math.round(y + height / 2), rect: { x, y, width, height } };
  }
  if (!fallbackRect) throw new Error('element has no box (not rendered)');
  const r = fallbackRect;
  return { cx: Math.round(r.x + r.width / 2), cy: Math.round(r.y + r.height / 2), rect: r };
}

async function dropPoint(cdp, step) {
  if (step.toRef) return centerOf(cdp, { backendNodeId: resolveRefEntry(cdp, step.toRef).backendDOMNodeId });
  const { root } = await cdp.send('DOM.getDocument', { depth: 0 });
  const { nodeId } = await cdp.send('DOM.querySelector', { nodeId: root.nodeId, selector: step.toSelector });
  if (!nodeId) throw new Error(`drag destination ${step.toSelector} matched nothing`);
  return centerOf(cdp, { nodeId });
}

// Pointer drag; native HTML5 drag-and-drop is intercepted and replayed as dragEnter/dragOver/drop.
async function performDrag(cdp, from, to) {
  let dragData = null;
  const onDrag = ({ data }) => { dragData = data; };
  cdp.on('Input.dragIntercepted', onDrag);
  await cdp.send('Input.setInterceptDrags', { enabled: true }).catch(() => {});
  try {
    await cdp.send('Input.dispatchMouseEvent', { type: 'mouseMoved', x: from.cx, y: from.cy });
    await cdp.send('Input.dispatchMouseEvent', { type: 'mousePressed', x: from.cx, y: from.cy, button: 'left', buttons: 1, clickCount: 1 });
    const n = 12;
    for (let i = 1; i <= n; i++) {
      const x = Math.round(from.cx + ((to.cx - from.cx) * i) / n);
      const y = Math.round(from.cy + ((to.cy - from.cy) * i) / n);
      await cdp.send('Input.dispatchMouseEvent', { type: 'mouseMoved', x, y, button: 'left', buttons: 1 });
      await sleep(16);
    }
    if (dragData) {
      for (const type of ['dragEnter', 'dragOver', 'drop']) await cdp.send('Input.dispatchDragEvent', { type, x: to.cx, y: to.cy, data: dragData });
    }
    await cdp.send('Input.dispatchMouseEvent', { type: 'mouseReleased', x: to.cx, y: to.cy, button: 'left', buttons: 0, clickCount: 1 });
  } finally {
    await cdp.send('Input.setInterceptDrags', { enabled: false }).catch(() => {});
    cdp.off('Input.dragIntercepted', onDrag);
  }
  return dragData ? 'html5' : 'pointer';
}

// Real CDP input: isTrusted events with mousedown/up, keydown/up, hover.
// The page keeps the last trusted mouse position, so the next approach starts where the pointer is.
async function lastMouse(cdp) {
  const res = await cdp.send('Runtime.evaluate', { expression: 'globalThis.__octoMouse ?? null', returnByValue: true }).catch(() => null);
  return res?.result?.value ?? null;
}
async function rememberMouse(cdp, x, y) {
  await cdp.send('Runtime.evaluate', { expression: `globalThis.__octoMouse = { x: ${x}, y: ${y} }` }).catch(() => {});
}

async function performTrusted(cdp, step, details, objectId) {
  const action = step.action;
  if (action === 'upload') {
    const files = step.value.split('|').map((f) => f.trim()).filter(Boolean).map((f) => (isAbsolute(f) ? f : resolve(f)));
    if (!files.length) return 'no-files';
    await cdp.send('DOM.setFileInputFiles', { files, objectId });
    return DONE.upload;
  }
  const target = objectId ? { objectId } : null;
  let at = await centerOf(cdp, target, details.bbox);
  const pointerAction = !['press', 'scroll', 'drag'].includes(action);
  if (pointerAction) {
    // Approach from where the mouse really is; leaving a hover menu can shift layout, so re-aim.
    const from = await lastMouse(cdp) ?? { x: Math.max(0, at.cx - 120 - Math.round(Math.random() * 120)), y: Math.max(0, at.cy + 60 + Math.round(Math.random() * 80)) };
    await runEventSequence(cdp, buildMouseMoveEvents(from.x, from.y, at.cx, at.cy));
    if (from.x === at.cx && from.y === at.cy) await cdp.send('Input.dispatchMouseEvent', { type: 'mouseMoved', x: at.cx, y: at.cy });
    const again = await centerOf(cdp, target, null);
    if (again && Math.abs(again.cx - at.cx) + Math.abs(again.cy - at.cy) > 2) {
      details.reaimed = true;
      await runEventSequence(cdp, buildMouseMoveEvents(at.cx, at.cy, again.cx, again.cy));
      at = again;
    }
  }
  const { cx, cy, rect } = at;
  await rememberMouse(cdp, cx, cy);
  if (action === 'hover') {
    // already there
  } else if (action === 'press') {
    await runEventSequence(cdp, buildKeyPressEvents(step.key));
  } else if (action === 'scroll') {
    await runEventSequence(cdp, buildScrollEvents(cx, cy, Number(step.value)));
  } else if (action === 'drag') {
    const to = await dropPoint(cdp, step);
    details.dragKind = await performDrag(cdp, at, to);
    await rememberMouse(cdp, to.cx, to.cy);
  } else if (action === 'dblclick') {
    for (const clickCount of [1, 2]) {
      await cdp.send('Input.dispatchMouseEvent', { type: 'mousePressed', x: cx, y: cy, button: 'left', buttons: 1, clickCount });
      await cdp.send('Input.dispatchMouseEvent', { type: 'mouseReleased', x: cx, y: cy, button: 'left', buttons: 0, clickCount });
    }
  } else {
    await runEventSequence(cdp, buildElementClickSequence(cx, cy, rect, action === 'fill' || action === 'type'));
  }
  if (action === 'fill' || action === 'type') {
    const focused = objectId
      ? (await cdp.send('Runtime.callFunctionOn', { objectId, functionDeclaration: SELECT_ALL_FN, returnByValue: true }).catch(() => null))?.result?.value
      : false;
    if (!focused) return 'not-focused';
    const hadText = (details.before?.value ?? '') !== '';
    if (hadText || step.value === '') await runEventSequence(cdp, buildKeyPressEvents('Delete'));
    if (step.value !== '') {
      if (action === 'fill') await cdp.send('Input.insertText', { text: step.value });
      else await runEventSequence(cdp, buildTypingEvents(step.value, { mistakeChance: 0, wpmBase: 300, wpmVariance: 50, burstPauseMs: [0, 20] }));
    }
  }
  return DONE[action];
}

function expectation(step) {
  if (step.action === 'fill' || step.action === 'type') return { field: 'value', want: step.value };
  if (step.action === 'check' || step.action === 'uncheck') return { field: 'checked', want: step.action === 'check' };
  if (step.action === 'select') return { field: 'selected', want: step.value, loose: true };
  if (step.action === 'upload') return { field: 'files', want: step.value.split('|').map((f) => f.trim().split('/').pop()).filter(Boolean).join(', ') };
  return null;
}

async function waitForText(cdp, text) {
  const wanted = text.split('|').map((t) => t.trim()).filter(Boolean);
  const start = Date.now();
  while (Date.now() - start < WAIT_MS) {
    const res = await cdp.send('Runtime.evaluate', {
      returnByValue: true,
      expression: `(() => { const t = document.body?.innerText ?? ''; return ${JSON.stringify(wanted)}.find((w) => t.includes(w)) ?? null; })()`,
    }).catch(() => null);
    const hit = res?.result?.value;
    if (hit) return { hit, ms: Date.now() - start };
    await sleep(150);
  }
  return { hit: null, ms: Date.now() - start };
}

const REASONS = {
  'blocked-by-actionability': (d) => `DOM_BLOCKED ${[!d.visible && 'not visible (hidden, zero-size or off-screen; re-snapshot: refs may have shifted)', d.disabled && 'disabled', d.covered && `covered by ${d.coveredBy}`, d.stable === false && 'still moving (animation)'].filter(Boolean).join('; ') || 'actionability check failed'}`,
  'not-fillable': (d) => `DOM_NOT_FILLABLE ${d.role} is not an editable field; pick a textbox/searchbox/contenteditable ref`,
  'not-checkable': (d) => `DOM_NOT_CHECKABLE ${d.role} is not a checkbox/radio/switch`,
  'not-selectable': (d) => `DOM_NOT_SELECTABLE ${d.role} is not a native <select>; click the custom combobox, then click its option ref`,
  'option-not-found': (d, s) => `DOM_OPTION_NOT_FOUND ${JSON.stringify(s.value)}; options: ${JSON.stringify(d.options ?? [])}`,
  'not-focused': () => 'DOM_NOT_FOCUSED click did not focus the field (overlay or focus trap); retry with DOM_INPUT=js',
  'not-file-input': (d) => `DOM_NOT_FILE_INPUT ${d.role} is not <input type=file>; target the file input (often hidden next to the styled button)`,
  'no-files': () => 'DOM_NO_FILES upload needs DOM_VALUE=/abs/path[|/abs/path2]',
};

// Prints one step; returns false when the batch should stop.
function reportStep(step, label, details, effects, prefix) {
  if (!details.found) {
    console.log(`[FINDING] ${prefix}DOM target not found target=${JSON.stringify(label)}${details.error ? ` error=${JSON.stringify(details.error)}` : ''}`);
    return false;
  }
  if (details.acted) {
    const how = MODE === 'trusted' && !['select', 'focus', 'upload'].includes(step.action) ? 'trusted input' : 'DOM';
    const valuePart = ['fill', 'type', 'select', 'upload'].includes(step.action) ? ` value=${JSON.stringify(step.value)}`
      : step.action === 'press' ? ` key=${JSON.stringify(step.key)}`
        : step.action === 'drag' ? ` to=${JSON.stringify(step.toRef || step.toSelector)} (${details.dragKind ?? 'pointer'})` : '';
    console.log(`[ACTION] ${prefix}${details.operation} ${JSON.stringify(details.accessibleNameGuess)} (${details.role}) via ${how}${valuePart}`);
    console.log(`[CODE] ${prefix}locator=${JSON.stringify(details.path)} action=${step.action}${valuePart}`);
    const a = details.after ?? {};
    const mutations = Math.max(0, (a.mutations ?? 0) - (details.mutationsBefore ?? 0));
    const parts = [];
    if (details.verified !== undefined) parts.push(details.verified ? 'ok' : 'MISMATCH');
    if (effects.navigatedTo) parts.push(`navigated=${effects.navigatedTo}`);
    else if (a.url && a.url !== details.location) parts.push(`url=${a.url}`);
    if (!effects.navigatedTo) parts.push(`mutations=${mutations}`);
    if (a.state && Object.keys(a.state).length) parts.push(`state=${JSON.stringify(a.state)}`);
    if (step.action === 'scroll') parts.push(`scrollY=${a.scrollY}`);
    if (a.focus) parts.push(`focus=${a.focus}`);
    if (a.dialogs) parts.push(`dialogs=${a.dialogs}`);
    if (effects.popup) parts.push(`popup=${effects.popup}`);
    if (a.connected === false && !effects.navigatedTo) parts.push('target-removed');
    console.log(`[VERIFY] ${prefix}${parts.join(' ')}`);
    if (effects.dialog) console.log(`[FINDING] ${prefix}JS_DIALOG ${effects.dialog.type} ${JSON.stringify(effects.dialog.message)} ${ACCEPT_DIALOG ? 'accepted' : 'dismissed (DOM_DIALOG=accept to accept)'}`);
    if (details.verified === false) console.log(`[FINDING] ${prefix}VERIFY_MISMATCH expected=${JSON.stringify(details.expected)}${MODE === 'trusted' ? ' — retry with DOM_ACTION=type or DOM_INPUT=js' : ''}`);
    const noEffect = !effects.navigatedTo && !effects.popup && !effects.dialog && mutations === 0 && a.url === details.location;
    details.noEffect = noEffect;
    if (noEffect && ['click', 'dblclick', 'press', 'drag'].includes(step.action)) console.log(`[FINDING] ${prefix}NO_VISIBLE_EFFECT nothing changed in the DOM or URL; the target may need another action, or the handler is async (DOM_WAIT_TEXT or DOM_SETTLE_MS)`);
    return details.verified !== false;
  }
  console.log(`[METRIC] ${prefix}DOM target=${JSON.stringify(label)} found=true visible=${details.visible} disabled=${details.disabled} stable=${details.stable} covered=${details.covered} canOperate=${details.canOperate}`);
  console.log(`[METRIC] ${prefix}DOM role=${JSON.stringify(details.role)} name=${JSON.stringify(details.accessibleNameGuess)} bbox=${JSON.stringify(details.bbox)}${details.before && Object.keys(details.before).length ? ` state=${JSON.stringify(details.before)}` : ''}`);
  if (details.coveredBy) console.log(`[FINDING] ${prefix}DOM element is covered by ${details.coveredBy}`);
  if (details.operation?.startsWith('already-')) {
    console.log(`[ACTION] ${prefix}${details.operation} ${JSON.stringify(details.accessibleNameGuess)} (no change needed)`);
    return true;
  }
  const reason = REASONS[details.operation];
  if (reason) console.log(`[FINDING] ${prefix}${reason(details, step)}`);
  return step.action === 'inspect';
}

const currentRefs = async (cdp) => (await collectRefs(cdp, {}).catch(() => ({ useful: [] }))).useful;

// Name the refs an action revealed (menu items, dialog fields) and append them to the snapshot
// so the next step can use them without a new full snapshot.
function reportNewRefs(cdp, before, after) {
  const had = new Set(before.map((u) => u.backendDOMNodeId));
  const added = after.filter((u) => !had.has(u.backendDOMNodeId));
  const nowIds = new Set(after.map((u) => u.backendDOMNodeId));
  const gone = before.filter((u) => !nowIds.has(u.backendDOMNodeId)).length;
  if (!added.length && !gone) return 0;
  const snap = loadSnapshot(cdp);
  const data = snap?.data ?? { url: cdp.targetInfo.url, refs: {}, regions: {} };
  data.refs ??= {};
  let next = Math.max(0, ...Object.keys(data.refs).map((k) => Number(k.slice(1)) || 0)) + 1;
  const known = new Map(Object.entries(data.refs).map(([k, v]) => [v.backendDOMNodeId, k]));
  const lines = [];
  for (const u of added) {
    let ref = known.get(u.backendDOMNodeId);
    if (!ref) {
      ref = `e${next++}`;
      data.refs[ref] = { backendDOMNodeId: u.backendDOMNodeId, role: u.role, name: u.fullName };
    }
    const role = u.role === 'heading' ? (u.level ? `h${u.level}` : 'heading') : u.role;
    lines.push(`[NEW] [${ref}] ${role}${u.name ? ` "${u.name}"` : ''}`);
  }
  const path = snap?.path ?? join(cdp.outputDir, 'page-snapshot.json');
  writeFileSync(path, `${JSON.stringify(data, null, 2)}\n`, { mode: 0o600 });
  if (!snap) cdp.upsertResourceMap?.('page-snapshot', { type: 'page-snapshot', targetUrl: cdp.targetInfo.url, refCount: Object.keys(data.refs).length, artifactPath: path });
  console.log(`[DIFF] new=${added.length} gone=${gone}`);
  for (const line of lines.slice(0, 20)) console.log(line);
  if (lines.length > 20) console.log(`[NEW] … ${lines.length - 20} more (run page-snapshot)`);
  return added.length;
}

async function runStep(cdp, step, effects) {
  const { label, details, objectId } = await locateAndCheck(cdp, step);
  if (details.pending) {
    details.pending = false;
    try {
      details.operation = await performTrusted(cdp, step, details, objectId);
    } catch (error) {
      details.operation = 'input-failed';
      details.error = String(error.message ?? error).slice(0, 200);
      console.log(`[FINDING] DOM_INPUT_FAILED ${details.error}`);
    }
  }
  details.acted = details.operation === DONE[step.action];
  if (details.acted) {
    await sleep(SETTLE_MS);
    if (effects.navigatedTo) await waitForPageReady(cdp, 8000);
    const res = !effects.navigatedTo && objectId
      ? await cdp.send('Runtime.callFunctionOn', { objectId, functionDeclaration: VERIFY_FN, returnByValue: true }).catch(() => null)
      : null;
    details.after = res?.result?.value
      ?? (await cdp.send('Runtime.evaluate', { expression: PAGE_STATE_JS, returnByValue: true }).catch(() => null))?.result?.value
      ?? null;
    details.effects = { ...effects };
    const exp = expectation(step);
    if (exp && details.after?.state) {
      const got = details.after.state[exp.field];
      const ok = exp.loose ? String(got ?? '').toLowerCase().includes(String(exp.want).toLowerCase()) || got === exp.want : got === exp.want;
      details.verified = ok;
      details.expected = { [exp.field]: exp.want, got };
    }
  }
  return { label, details };
}

export async function run(cdp) {
  await cdp.send('Runtime.enable');
  await cdp.send('DOM.enable');
  await cdp.send('Accessibility.enable');
  await cdp.send('Page.enable');

  const ready = await waitForPageReady(cdp);
  if (!ready) console.log('[FINDING] PAGE_NOT_FULLY_LOADED document.readyState never reached "complete" — a not-found/blocked result below may reflect a page that hasn\'t rendered yet, not a real absence');

  // Effects the action may cause outside the element.
  const effects = { navigatedTo: null, dialog: null, popup: null };
  let navigatedAny = false;
  const onNav = ({ frame }) => { if (!frame.parentId) { effects.navigatedTo = frame.url; navigatedAny = true; } };
  const onDialog = async ({ type, message }) => {
    effects.dialog = { type, message: String(message).slice(0, 160), accepted: ACCEPT_DIALOG };
    await cdp.send('Page.handleJavaScriptDialog', { accept: ACCEPT_DIALOG }).catch(() => {});
  };
  const onPopup = ({ url }) => { effects.popup = url; };
  cdp.on('Page.frameNavigated', onNav);
  cdp.on('Page.javascriptDialogOpening', onDialog);
  cdp.on('Page.windowOpen', onPopup);

  const mutating = STEPS.some((s) => !['inspect', 'wait'].includes(s.action));
  const before = DIFF && mutating ? await currentRefs(cdp) : null;
  const results = [];
  let ok = true;
  for (const step of STEPS) {
    const prefix = STEPS.length > 1 ? `#${step.index} ` : '';
    Object.assign(effects, { navigatedTo: null, dialog: null, popup: null });
    if (step.action === 'wait') {
      const { hit, ms } = await waitForText(cdp, step.value || WAIT_TEXT);
      console.log(hit ? `[WAIT] ${prefix}found ${JSON.stringify(hit)} after ${ms}ms` : `[FINDING] ${prefix}WAIT_TIMEOUT none of ${JSON.stringify(step.value || WAIT_TEXT)} appeared within ${WAIT_MS}ms`);
      results.push({ step, found: Boolean(hit), ms });
      if (!hit) { ok = false; break; }
      continue;
    }
    const { label, details } = await runStep(cdp, step, effects);
    results.push({ step, label, details });
    if (!reportStep(step, label, details, effects, prefix)) { ok = false; break; }
  }
  if (ok && WAIT_TEXT && STEPS.at(-1).action !== 'wait') {
    const { hit, ms } = await waitForText(cdp, WAIT_TEXT);
    console.log(hit ? `[WAIT] found ${JSON.stringify(hit)} after ${ms}ms` : `[FINDING] WAIT_TIMEOUT none of ${JSON.stringify(WAIT_TEXT)} appeared within ${WAIT_MS}ms`);
  }
  const added = before && !navigatedAny ? reportNewRefs(cdp, before, await currentRefs(cdp)) : 0;
  const lastHover = results.at(-1)?.step.action === 'hover' ? results.at(-1).details : null;
  if (lastHover?.noEffect && !added) console.log('[FINDING] NO_VISIBLE_EFFECT hover changed nothing and revealed no refs; the menu may open on click');
  if (before && navigatedAny) console.log('[NEXT] page navigated: run page-snapshot for fresh refs');
  if (STEPS.length > 1) console.log(`[METRIC] STEPS done=${results.filter((r) => r.found || r.details?.acted || r.step.action === 'inspect').length}/${STEPS.length}${ok ? '' : ' stopped'}`);

  cdp.off('Page.frameNavigated', onNav);
  cdp.off('Page.javascriptDialogOpening', onDialog);
  cdp.off('Page.windowOpen', onPopup);

  const artifactPath = join(cdp.outputDir, 'dom-check.json');
  const artifact = STEPS.length === 1 ? results[0]?.details ?? results[0] : results.map((r) => ({ step: r.step, ...(r.details ?? { found: r.found, ms: r.ms }) }));
  writeFileSync(artifactPath, `${JSON.stringify(artifact, null, 2)}\n`, { mode: 0o600 });
  cdp.upsertResourceMap?.('dom-operation-check', { type: 'dom-operation-check', target: results[0]?.label ?? null, action: STEPS.map((s) => s.action).join(','), artifactPath, targetUrl: cdp.targetInfo.url });
  console.log(`[ARTIFACT] DOM_CHECK ${artifactPath}`);
  if (!ok) process.exitCode = 1;
}
