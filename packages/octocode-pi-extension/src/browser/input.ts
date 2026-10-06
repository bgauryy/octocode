import { errorMessage, withTimeout } from '../shared/util.js';
import { abortError, type BrowserSession } from './cdp.js';

type Page = BrowserSession['page'];

/** The element numbered `ref` by the last snapshot, while it is still in the page. */
export const elementExpr = (ref: number) => `(() => { const el = (window.__octoRefs || [])[${ref - 1}]; return el && el.isConnected ? el : null; })()`;

interface Point {
  x: number;
  y: number;
  /** The element on top at that point when it is not the target (a banner or overlay the mouse would hit instead). */
  covered: string;
}

/**
 * Scrolls `[ref]` into view and returns its centre in main-frame viewport coordinates (frame offsets added), or
 * `hidden` for an element with no box (after clicking it directly when `clickHidden`). A link that would open a new tab
 * is retargeted to this one first, so the session keeps driving it.
 */
async function pointOf(page: Page, ref: number, signal: AbortSignal | undefined, options: { clickHidden?: boolean; block?: 'center' | 'nearest' } = {}): Promise<Point | 'hidden'> {
  const point = await page.evaluate<Point | null | 'hidden'>(
    `(() => {
      const el = ${elementExpr(ref)};
      if (!el) return null;
      const link = el.closest && el.closest('a[target]');
      if (link && link.target !== '_self') link.target = '_self';
      el.scrollIntoView({ block: '${options.block ?? 'center'}', inline: 'nearest' });
      const r = el.getBoundingClientRect();
      if (r.width <= 0 || r.height <= 0) { ${options.clickHidden ? 'el.click(); ' : ''}return 'hidden'; }
      const cx = r.left + r.width / 2, cy = r.top + r.height / 2;
      const hit = el.getRootNode().elementFromPoint ? el.getRootNode().elementFromPoint(cx, cy) : null;
      const covered = hit && hit !== el && !el.contains(hit) && !hit.contains(el) ? hit.tagName.toLowerCase() + (hit.id ? '#' + hit.id : '') + ' "' + String(hit.innerText || '').trim().slice(0, 40) + '"' : '';
      let x = cx, y = cy;
      for (let w = el.ownerDocument.defaultView; w && w.frameElement; w = w.parent) { const f = w.frameElement.getBoundingClientRect(); x += f.left + w.frameElement.clientLeft; y += f.top + w.frameElement.clientTop; }
      return { x, y, covered };
    })()`,
    signal,
  );
  if (point === null) throw new Error(`No element [${ref}] — take a new snapshot.`);
  return point;
}

const mouse = (page: Page, type: string, at: { x: number; y: number }, extra: Record<string, unknown>, signal?: AbortSignal) => page.send('Input.dispatchMouseEvent', { type, ...at, ...extra }, signal);

/**
 * Moves the mouse onto `[ref]` and returns where it ended up. Moving can shift the layout (a hover menu elsewhere
 * closes, a sticky header appears), so the element is measured again after the move and the mouse follows it.
 */
async function approach(page: Page, ref: number, signal: AbortSignal | undefined, options: { clickHidden?: boolean; block?: 'center' | 'nearest' } = {}): Promise<Point | 'hidden'> {
  const first = await pointOf(page, ref, signal, options);
  if (first === 'hidden') return first;
  await mouse(page, 'mouseMoved', first, { button: 'none', buttons: 0 }, signal);
  const settled = await pointOf(page, ref, signal, { block: 'nearest' });
  if (settled === 'hidden') return first;
  if (Math.abs(settled.x - first.x) > 1 || Math.abs(settled.y - first.y) > 1) await mouse(page, 'mouseMoved', settled, { button: 'none', buttons: 0 }, signal);
  return settled;
}

/**
 * Clicks `[ref]` the way a user does: a real mouse press and release at its centre (trusted events, so
 * mousedown/pointerdown handlers run). Returns a note when another element covers the target (the click lands on it,
 * as it would for a user) or when the element had no box and was clicked directly.
 */
export async function click(page: Page, ref: number, signal?: AbortSignal): Promise<string> {
  const point = await approach(page, ref, signal, { clickHidden: true });
  if (point === 'hidden') return `[${ref}] has no visible box; it was clicked directly.`;
  await mouse(page, 'mousePressed', point, { button: 'left', buttons: 1, clickCount: 1 }, signal);
  await mouse(page, 'mouseReleased', point, { button: 'left', buttons: 0, clickCount: 1 }, signal);
  return point.covered ? `Note: ${point.covered} covers [${ref}], so the click landed on it.` : '';
}

/** Moves the mouse onto `[ref]` with no button held, so hover menus and tooltips open; the next mouse action ends it. */
export async function hover(page: Page, ref: number, signal?: AbortSignal): Promise<string> {
  const point = await approach(page, ref, signal);
  if (point === 'hidden') throw new Error(`[${ref}] has no visible box to hover.`);
  return point.covered ? `Note: ${point.covered} covers [${ref}], so the mouse is over it instead.` : '';
}

/** Mouse moves between press and release, so pointer-based drag libraries see a real path. */
const DRAG_STEPS = 6;
const DRAG_STEP_MS = 16;
/** Longest wait for Chrome to report an HTML5 drag after the moves. */
const DRAG_INTERCEPT_WAIT_MS = 500;

/**
 * Numbers, as the last element of `window.__octoRefs`, the smallest visible element whose text contains `text` (a drop
 * zone the snapshot does not list because it has no role or handler attribute); throws when there is none.
 */
export async function refForText(page: Page, text: string, signal?: AbortSignal): Promise<number> {
  const ref = await page.evaluate<number | null>(
    `(() => {
      const want = ${JSON.stringify(text.trim().toLowerCase())};
      let best = null;
      for (const el of document.body ? document.body.querySelectorAll('*') : []) {
        const r = el.getBoundingClientRect();
        if (r.width <= 0 || r.height <= 0 || !String(el.innerText || '').toLowerCase().includes(want)) continue;
        if (!best || r.width * r.height < best.area) best = { el, area: r.width * r.height };
      }
      if (!best) return null;
      const refs = window.__octoRefs || (window.__octoRefs = []);
      if (best.el.__octoRef && refs[best.el.__octoRef - 1] === best.el) return best.el.__octoRef;
      refs.push(best.el);
      best.el.__octoRef = refs.length;
      return refs.length;
    })()`,
    signal,
  );
  if (ref === null) throw new Error(`No visible element contains ${JSON.stringify(text)}.`);
  return ref;
}

/**
 * Drags `[from]` onto `[to]`. The mouse is pressed on the source and moved in steps; with drag interception on, an
 * HTML5 drag (draggable elements, dataTransfer) is reported by Chrome and finished with dragEnter/dragOver/drop at the
 * target, while a pointer-based drag (mousedown/mousemove/mouseup libraries) just gets the moves and the release.
 */
export async function drag(page: Page, from: number, to: number, signal?: AbortSignal): Promise<string> {
  const start = await approach(page, from, signal);
  if (start === 'hidden') throw new Error(`[${from}] has no visible box to drag.`);
  const end = await pointOf(page, to, signal, { block: 'nearest' });
  if (end === 'hidden') throw new Error(`[${to}] has no visible box to drop on.`);
  let data: unknown;
  let intercepted: (() => void) | undefined;
  const off = page.on((method, params) => {
    if (method !== 'Input.dragIntercepted') return;
    data = params['data'];
    intercepted?.();
  });
  try {
    await page.send('Input.setInterceptDrags', { enabled: true }, signal);
    await mouse(page, 'mousePressed', start, { button: 'left', buttons: 1, clickCount: 1 }, signal);
    for (let step = 1; step <= DRAG_STEPS; step++) {
      const at = { x: start.x + ((end.x - start.x) * step) / DRAG_STEPS, y: start.y + ((end.y - start.y) * step) / DRAG_STEPS };
      await mouse(page, 'mouseMoved', at, { button: 'left', buttons: 1 }, signal);
      // Paced like a hand: Chrome starts an HTML5 drag (and reports it) only across separate frames.
      await new Promise((resolve) => setTimeout(resolve, DRAG_STEP_MS));
    }
    // An HTML5 drag is reported shortly after the moves; a pointer drag never is.
    if (data === undefined) await new Promise<void>((resolve) => ((intercepted = resolve), setTimeout(resolve, DRAG_INTERCEPT_WAIT_MS)));
  } finally {
    off();
    await page.send('Input.setInterceptDrags', { enabled: false }).catch(() => undefined);
  }
  if (data !== undefined) {
    for (const type of ['dragEnter', 'dragOver', 'drop']) await page.send('Input.dispatchDragEvent', { type, ...end, data, modifiers: 0 }, signal);
  }
  await mouse(page, 'mouseReleased', end, { button: 'left', buttons: 0, clickCount: 1 }, signal);
  const kind = data !== undefined ? 'an HTML5 drag' : 'a mouse drag';
  return `Dragged [${from}] onto [${to}] (${kind}).${end.covered ? ` Note: ${end.covered} covers [${to}].` : ''}`;
}

const MODIFIERS: Record<string, number> = { alt: 1, control: 2, ctrl: 2, meta: 4, cmd: 4, command: 4, shift: 8 };
const KEY_CODES: Record<string, number> = { Enter: 13, Tab: 9, Escape: 27, Backspace: 8, Delete: 46, ' ': 32, Space: 32, ArrowLeft: 37, ArrowUp: 38, ArrowRight: 39, ArrowDown: 40, Home: 36, End: 35, PageUp: 33, PageDown: 34 };
/** Editing shortcuts Chrome only performs when asked for by command name (a synthetic Ctrl/Cmd+A selects nothing otherwise). */
const EDIT_COMMANDS: Record<string, string> = { a: 'selectAll', c: 'copy', x: 'cut', v: 'paste', z: 'undo' };

/**
 * Presses `combo`: one key (`Enter`, `a`, `ArrowDown`) or modifiers joined with `+` (`Control+A`, `Shift+Tab`,
 * `Meta+Enter`). Printable keys type their character unless Control or Meta is held.
 */
export async function pressKey(page: Page, combo: string, signal?: AbortSignal): Promise<void> {
  const parts = combo.split('+').map((part) => part.trim());
  const named = parts.length > 1 && parts.at(-1) === '' ? '+' : parts.at(-1) || combo;
  let modifiers = 0;
  for (const part of parts.slice(0, -1)) {
    const bit = MODIFIERS[part.toLowerCase()];
    if (bit === undefined) throw new Error(`Unknown modifier "${part}" in "${combo}"; use Alt, Control, Meta or Shift.`);
    modifiers |= bit;
  }
  const key = named === 'Space' ? ' ' : named;
  const printable = key.length === 1;
  const code = !printable ? key : /\d/.test(key) ? `Digit${key}` : /[a-z]/i.test(key) ? `Key${key.toUpperCase()}` : key === ' ' ? 'Space' : key;
  const shortcut = (modifiers & (2 | 4)) !== 0;
  const text = key === 'Enter' ? '\r' : printable && !shortcut ? (modifiers & 8 ? key.toUpperCase() : key) : undefined;
  const keyCode = KEY_CODES[key] ?? (printable ? key.toUpperCase().charCodeAt(0) : undefined);
  const command = shortcut && printable ? EDIT_COMMANDS[key.toLowerCase()] : undefined;
  const base = { key, code, modifiers, ...(keyCode !== undefined ? { windowsVirtualKeyCode: keyCode } : {}) };
  await page.send('Input.dispatchKeyEvent', { type: text ? 'keyDown' : 'rawKeyDown', ...base, ...(text ? { text } : {}), ...(command ? { commands: [command] } : {}) }, signal);
  await page.send('Input.dispatchKeyEvent', { type: 'keyUp', ...base }, signal);
}

/** How long an action waits to see whether it started a navigation. */
const NAVIGATION_PROBE_MS = 150;
/** How long a navigation the action started may take to load. */
const LOAD_MS = 10_000;
/** DOM quiet time that counts as settled, and the longest wait for it. */
const DOM_QUIET_MS = 150;
const DOM_MAX_MS = 3_000;

/**
 * After an action: wait briefly to see whether it started a navigation (a link, a form post) and, if so, for the load
 * to finish; then until the DOM has been quiet for a moment (an SPA re-rendering), capped. Never fails on a timeout:
 * the snapshot that follows shows whatever is there.
 */
export async function settle(page: Pick<Page, 'on' | 'evaluate'>, signal?: AbortSignal, timing = { probeMs: NAVIGATION_PROBE_MS, loadMs: LOAD_MS, quietMs: DOM_QUIET_MS, domMaxMs: DOM_MAX_MS }): Promise<void> {
  const loading = new Set<string>();
  let wake: (() => void) | undefined;
  const off = page.on((method, params) => {
    if (method === 'Page.frameStartedLoading') loading.add(String(params['frameId']));
    else if (method === 'Page.frameStoppedLoading') loading.delete(String(params['frameId']));
    else return;
    wake?.();
  });
  const pause = (ms: number, done: () => boolean) =>
    new Promise<void>((resolve, reject) => {
      if (signal?.aborted) return reject(abortError(signal));
      const finish = () => {
        clearTimeout(timer);
        signal?.removeEventListener('abort', onAbort);
        wake = undefined;
        resolve();
      };
      const onAbort = () => {
        clearTimeout(timer);
        wake = undefined;
        reject(abortError(signal!));
      };
      const timer = setTimeout(finish, ms);
      wake = () => done() && finish();
      signal?.addEventListener('abort', onAbort, { once: true });
    });
  try {
    await pause(timing.probeMs, () => loading.size > 0);
    if (loading.size > 0) await pause(timing.loadMs, () => loading.size === 0);
  } finally {
    off();
  }
  // A page torn down mid-wait (another navigation) or an open dialog ends the quiet wait early; either is fine here.
  await page
    .evaluate(
      `new Promise((resolve) => { let timer; const done = () => { observer.disconnect(); clearTimeout(cap); resolve(true); }; const observer = new MutationObserver(() => { clearTimeout(timer); timer = setTimeout(done, ${timing.quietMs}); }); observer.observe(document, { subtree: true, childList: true, attributes: true, characterData: true }); timer = setTimeout(done, ${timing.quietMs}); const cap = setTimeout(done, ${timing.domMaxMs}); })`,
      signal,
    )
    .catch(() => undefined);
}

/** After a navigation's load event: how long content and requests must stay quiet, and the longest wait. */
const LOAD_QUIET_MS = 500;
const LOAD_MAX_MS = 6_000;

/**
 * After a navigation has loaded: waits until the page's content (nodes and text, not attribute-only animation) and its
 * network requests have both been quiet for `quietMs`, capped at `maxMs`, so values a script fills in after load
 * (prices, a localized currency, search results) are in the snapshot. Resolves how long it took and whether the page
 * went quiet; never fails.
 */
export async function settleAfterLoad(page: Pick<Page, 'evaluate'>, signal?: AbortSignal, quietMs = LOAD_QUIET_MS, maxMs = LOAD_MAX_MS): Promise<{ settled: boolean; ms: number }> {
  const started = Date.now();
  const settled = await page
    .evaluate<boolean>(
      `new Promise((resolve) => { let timer; let perf; const finish = (quiet) => { observer.disconnect(); if (perf) perf.disconnect(); clearTimeout(timer); clearTimeout(cap); resolve(quiet); }; const poke = () => { clearTimeout(timer); timer = setTimeout(() => finish(true), ${quietMs}); }; const observer = new MutationObserver(poke); observer.observe(document, { subtree: true, childList: true, characterData: true }); try { perf = new PerformanceObserver(poke); perf.observe({ type: 'resource' }); } catch {} poke(); const cap = setTimeout(() => finish(false), ${maxMs}); })`,
      signal,
    )
    .catch(() => true);
  return { settled: settled !== false, ms: Date.now() - started };
}

/** The page text `waitForText` searches: the document, open shadow roots and same-origin frames. */
const PAGE_TEXT = `(() => { const parts = []; const walk = (root) => { for (const el of root.querySelectorAll('*')) { if (el.shadowRoot) { parts.push([...el.shadowRoot.children].map((c) => c.innerText || '').join(' ')); walk(el.shadowRoot); } if (el.tagName === 'IFRAME') { try { if (el.contentDocument?.body) { parts.push(el.contentDocument.body.innerText); walk(el.contentDocument); } } catch {} } } }; if (document.body) { parts.push(document.body.innerText); walk(document); } return parts.join('\\n'); })()`;

/**
 * Polls until `text` appears on the page (or, with `gone`, disappears), up to `timeoutMs`; resolves with how long it
 * took, or undefined when a JavaScript dialog opened (it blocks the page, so the caller reports it). Throws on timeout,
 * naming what was expected. A probe never runs past the time left, so a stalled page cannot stretch the wait.
 */
export async function waitForText(page: Pick<Page, 'evaluate' | 'untilDialog'>, text: string, gone: boolean, timeoutMs: number, signal?: AbortSignal, intervalMs = 250): Promise<number | undefined> {
  const started = Date.now();
  for (;;) {
    const left = Math.max(timeoutMs - (Date.now() - started), intervalMs);
    const probe = await page.untilDialog(withTimeout(page.evaluate<string>(PAGE_TEXT, signal), left, 'wait').then((body) => ({ body: String(body) }), () => ({ body: undefined })));
    if (!probe) return undefined;
    // A probe that failed (a navigation tore the page down) or ran out of time proves nothing either way.
    if (probe.body !== undefined && probe.body.includes(text) !== gone) return Date.now() - started;
    if (Date.now() - started >= timeoutMs) throw new Error(`Timed out after ${Math.round(timeoutMs / 1000)}s waiting for ${JSON.stringify(text)} to ${gone ? 'disappear' : 'appear'}.`);
    if (signal?.aborted) throw abortError(signal);
    await new Promise((resolve) => setTimeout(resolve, intervalMs));
  }
}

/**
 * Prepares `[ref]` for typed text and says how: `select` (the option was picked here), `field`/`editable` (existing text
 * selected, so the typed text replaces it), `no-focus` (focus did not land on a text field, so typing would go elsewhere),
 * `no-option:<labels>`, or null when the element is gone.
 */
function focusForTyping(ref: number, text: string): string {
  return `(() => {
    const el = ${elementExpr(ref)};
    if (!el) return null;
    if (el.tagName === 'SELECT') {
      const want = ${JSON.stringify(text)}.trim().toLowerCase();
      const options = [...el.options];
      if (!want) return 'no-option:' + options.map((o) => o.label.trim()).join(' | ');
      const option = options.find((o) => o.label.trim().toLowerCase() === want || o.value.toLowerCase() === want) || options.find((o) => o.label.toLowerCase().includes(want));
      if (!option) return 'no-option:' + options.map((o) => o.label.trim()).join(' | ');
      el.value = option.value;
      el.dispatchEvent(new Event('input', { bubbles: true }));
      el.dispatchEvent(new Event('change', { bubbles: true }));
      return 'select';
    }
    el.scrollIntoView({ block: 'center' });
    el.focus();
    let active = el.ownerDocument.activeElement;
    while (active && active.shadowRoot && active.shadowRoot.activeElement) active = active.shadowRoot.activeElement;
    if (active !== el && !(active && el.contains(active))) return 'no-focus';
    const field = active || el;
    if (field.isContentEditable) { field.ownerDocument.getSelection().selectAllChildren(field); return 'editable'; }
    if (field.tagName !== 'TEXTAREA' && !(field.tagName === 'INPUT' && !/^(button|submit|reset|checkbox|radio|file|image|range|color)$/i.test(field.type))) return 'no-focus';
    try { field.select(); } catch { field.value = ''; }
    return 'field';
  })()`;
}

/**
 * Types `text` into `[ref]`, replacing what is there (a `<select>` gets the option with that label or value), then
 * presses Enter when `submit`. Refused, with the reason, when the element is gone, has no such option, or focus does
 * not land on a text field (the text would go elsewhere).
 */
export async function typeInto(page: Page, ref: number, text: string, submit: boolean, signal?: AbortSignal): Promise<void> {
  const target = await page.evaluate<string | null>(focusForTyping(ref, text), signal);
  if (target === null) throw new Error(`No element [${ref}] — take a new snapshot.`);
  if (target.startsWith('no-option:')) throw new Error(`[${ref}] has no option "${text}". Options: ${target.slice('no-option:'.length)}`);
  if (target === 'no-focus') throw new Error(`[${ref}] is not a text field, so typing was refused (it would land elsewhere); click it, or type into the input it opens.`);
  if (target !== 'select') await page.send('Input.insertText', { text }, signal);
  if (submit) await pressKey(page, 'Enter', signal);
}

/** Checkbox-like state of `[ref]`: `checked`/`unchecked`, `text` for anything else, or null when it is gone. */
const toggleState = (ref: number) => `(() => {
  const el = ${elementExpr(ref)};
  if (!el) return null;
  const type = (el.getAttribute('type') || '').toLowerCase();
  if (el.tagName === 'INPUT' && (type === 'checkbox' || type === 'radio')) return el.checked ? 'checked' : 'unchecked';
  const role = el.getAttribute('role');
  if (role === 'checkbox' || role === 'switch' || role === 'radio') return el.getAttribute('aria-checked') === 'true' ? 'checked' : 'unchecked';
  return 'text';
})()`;

/**
 * Fills several fields in one call: text fields and `<select>`s through `typeInto`; checkboxes, radios and switches are
 * clicked only when their state differs from the value (`true`/`false`). Every field is tried; returns one line per
 * field that failed (empty when all succeeded).
 */
export async function fillFields(page: Page, fields: ReadonlyArray<{ ref: number; value: string }>, signal?: AbortSignal): Promise<string[]> {
  const failures: string[] = [];
  for (const { ref, value } of fields) {
    try {
      const state = await page.evaluate<string | null>(toggleState(ref), signal);
      if (state === null) throw new Error(`No element [${ref}] — take a new snapshot.`);
      if (state === 'text') await typeInto(page, ref, value, false, signal);
      else if ((state === 'checked') !== /^(true|yes|on|1|checked)$/i.test(value.trim())) await click(page, ref, signal);
    } catch (error) {
      failures.push(`[${ref}]: ${errorMessage(error)}`);
    }
  }
  return failures;
}
