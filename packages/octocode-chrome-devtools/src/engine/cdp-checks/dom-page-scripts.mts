// Page-side JavaScript the DOM operations evaluate: element checks, state reads and text waits.
import { ACTIONABILITY_HELPERS_JS } from '../dom-actionability.mjs';

// Page-side helpers shared by the act and verify phases.
const STATE_JS = `
  function shortText(value) { return String(value ?? '').replace(/\\s+/g, ' ').trim().slice(0, 160); }
  function describeEl(el) {
    if (!el || el === el.ownerDocument.body || el === el.ownerDocument.documentElement) return null;
    return (el.localName + (el.id ? '#' + el.id : '') + (el.getAttribute('name') ? '[name=' + el.getAttribute('name') + ']' : '')).slice(0, 80);
  }
  function stateOf(el) {
    const s = {};
    if (el.isContentEditable) s.value = el.innerText.replace(/\\n$/, '');
    else if (el.type === 'file') s.files = [...(el.files ?? [])].map(f => f.name).join(', ');
    else if ('value' in el && !['button', 'li', 'option'].includes(el.localName) && el.type !== 'checkbox' && el.type !== 'radio') s.value = String(el.value);
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

// checkElement() and its helpers; the trace slot takes the run's traceEvents value.
const CORE_BODY_TEMPLATE = `
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
    if (__OCTO_TRACE_EVENTS__ && !globalThis.__octoTrace) {
      const events = [];
      const root = element.getRootNode();
      const types = ['pointerdown', 'pointerup', 'mousedown', 'mouseup', 'click', 'dblclick', 'keydown', 'keyup', 'beforeinput', 'input', 'change', 'focus', 'blur', 'dragstart', 'drop', 'compositionstart', 'compositionend'];
      const handler = event => {
        const target = event.composedPath()[0] || event.target;
        events.push({ type: event.type, trusted: event.isTrusted, at: event.timeStamp, wallTime: performance.timeOrigin + event.timeStamp, frameURL: location.href, target: elementPath(target), inputType: event.inputType || null });
      };
      for (const type of types) root.addEventListener(type, handler, true);
      globalThis.__octoTrace = { events, stop() { for (const type of types) root.removeEventListener(type, handler, true); delete globalThis.__octoTrace; return events; } };
    }
    if (!globalThis.__octoMut) {
      globalThis.__octoMut = { n: 0 };
      new MutationObserver(list => { globalThis.__octoMut.n += list.length; })
        .observe(document, { subtree: true, childList: true, attributes: true, characterData: true });
    }
    if (!['upload', 'wait'].includes(action)) element.scrollIntoView({ block: 'center', inline: 'center', behavior: 'instant' });
    const style = getComputedStyle(element);
    const rectCheck = await stableRect(element, stabilityMs);
    const rect = rectCheck.second;
    const doc = element.ownerDocument;
    const hit = doc.elementFromPoint(rect.left + rect.width / 2, rect.top + rect.height / 2);
    let shadowHost = element.getRootNode()?.host;
    let hitIsShadowHost = false;
    while (shadowHost) { if (hit === shadowHost) hitIsShadowHost = true; shadowHost = shadowHost.getRootNode()?.host; }
    const coveredBy = hit && !hitIsShadowHost && hit !== element && !element.contains(hit) && !hit.contains(element) ? elementPath(hit) : null;
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
    details.canOperate = action === 'scroll' || action === 'upload' || (details.visible && !details.disabled && (!pointer || (!details.covered && details.stable && style.pointerEvents !== 'none')));

    if (action === 'inspect' || action === 'wait') {
      details.operation = 'inspected';
      details.waitTextMatches = !value || (element.innerText || '').includes(value);
    }
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
        if (!opt) { details.operation = 'option-not-found'; details.options = opts.map(o => o.label.trim()); }
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
      element.dispatchEvent(new InputEvent('input', { bubbles: true, composed: true, inputType: 'insertText', data: value }));
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
const TRACE_SLOT = '__OCTO_TRACE_EVENTS__';

/** The element-check body with event tracing on or off. */
export const coreBodyJs = traceEvents =>
  CORE_BODY_TEMPLATE.replace(TRACE_SLOT, () => `${traceEvents}`);

/** Focuses the element and selects its whole value, so typed text replaces it. */
export const SELECT_ALL_FN = `function() {
  const el = this;
  if (!el?.isConnected) return false;
  el.focus();
  const doc = el.ownerDocument;
  if (el.isContentEditable) {
    const range = doc.createRange();
    range.selectNodeContents(el);
    const sel = doc.getSelection(); sel.removeAllRanges(); sel.addRange(range);
  } else { try { el.select(); } catch { el.setSelectionRange?.(0, String(el.value).length); } }
  return doc.activeElement === el || el.getRootNode().activeElement === el || el.contains(doc.activeElement);
}`;

/** The element and page state after an action. */
export const VERIFY_FN = `function() {
  ${STATE_JS}
  const el = this;
  const doc = el?.ownerDocument ?? document;
  const connected = Boolean(el?.isConnected);
  return { url: location.href, connected, state: connected ? stateOf(el) : null,
    mutations: globalThis.__octoMut?.n ?? 0, scrollY: Math.round(scrollY), focus: describeEl(doc.activeElement), dialogs: openDialogs(), events: globalThis.__octoTrace?.stop() };
}`;

/** Page state when the acted element is unavailable. */
export const PAGE_STATE_JS = `(() => ({ url: location.href, mutations: globalThis.__octoMut?.n ?? 0, scrollY: Math.round(scrollY) }))()`;

/** Stops event tracing and returns the recorded events. */
export const TRACE_STOP_FN =
  'function() { return globalThis.__octoTrace?.stop(); }';

/** Finds the first wanted text in the element's root, its frames, or the document when detached. */
export const textWaitFn = wanted => `function() {
    const texts = [];
    const read = root => {
      texts.push(root.nodeType === 9 ? root.body?.innerText || '' : root.textContent || '');
      for (const frame of root.querySelectorAll('iframe,frame')) { try { if (frame.contentDocument) read(frame.contentDocument); } catch {} }
    };
    read(this?.isConnected === false ? this.ownerDocument || document : this?.getRootNode?.() || document);
    return ${JSON.stringify(wanted)}.find(w => texts.some(t => t.includes(w))) || null;
  }`;
