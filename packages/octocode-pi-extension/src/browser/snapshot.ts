import { dialogNotice } from './actions.js';
import type { BrowserSession } from './cdp.js';
import { shortPath } from '../shared/format.js';
import { saveFullOutput } from '../shared/spill.js';
import { capChars } from '../shared/util.js';

/** Snapshot budget: the element list comes first, page text fills what is left. */
const SNAPSHOT_MAX_CHARS = 8_000;
/** Most of the budget the element list may take, so some page text always shows. */
const ELEMENTS_MAX_CHARS = 5_000;

/**
 * Lists the visible interactive elements in document order, through open shadow roots and same-origin frames, each
 * with a stable number kept in `window.__octoRefs` (element [n] is index n-1) for the actions to find. Returns page text plus one line
 * per element: tag, label, and a field's value and state.
 */
const SNAPSHOT_SCRIPT = `(() => {
  const selector = 'a[href],button,input:not([type=hidden]),select,textarea,summary,[role=button],[role=link],[role=tab],[role=menuitem],[role=option],[role=checkbox],[role=radio],[role=switch],[role=combobox],[contenteditable=true],[onclick],[draggable=true]';
  // Clickable divs and spans (framework click handlers leave no attribute): the outermost element with a pointer cursor
  // that holds no other control.
  const pointer = (el) => { const view = el.ownerDocument.defaultView; return view.getComputedStyle(el).cursor === 'pointer' && !(el.parentElement && view.getComputedStyle(el.parentElement).cursor === 'pointer') && !el.querySelector(selector); };
  const clean = (value, max) => String(value || '').trim().replace(/\\s+/g, ' ').slice(0, max);
  const visible = (el) => { const r = el.getBoundingClientRect(); if (r.width <= 0 || r.height <= 0) return false; const s = el.ownerDocument.defaultView.getComputedStyle(el); return s.visibility !== 'hidden' && s.display !== 'none'; };
  const labelOf = (el) => {
    const doc = el.ownerDocument;
    const by = el.getAttribute('aria-labelledby');
    const named = el.getAttribute('aria-label') || (by && by.split(/\\s+/).map((id) => doc.getElementById(id)?.innerText || '').join(' ')) || (el.labels && el.labels[0] && el.labels[0].innerText);
    if (named) return clean(named, 80);
    const field = /^(INPUT|TEXTAREA|SELECT)$/.test(el.tagName);
    return clean((!field && el.innerText) || el.getAttribute('placeholder') || el.getAttribute('title') || el.getAttribute('name') || (field && el.id) || (el.querySelector && el.querySelector('img[alt]')?.getAttribute('alt')) || (field ? '' : el.value), 80);
  };
  const stateOf = (el) => {
    const parts = [];
    const type = (el.getAttribute('type') || '').toLowerCase();
    if (el.tagName === 'SELECT') parts.push('options=' + [...el.options].slice(0, 12).map((o) => (o.selected ? '*' : '') + clean(o.label, 30)).join('|') + (el.options.length > 12 ? '|…' + el.options.length : ''));
    else if (type === 'checkbox' || type === 'radio') parts.push(el.checked ? 'checked' : 'unchecked');
    else if (type === 'password') { if (el.value) parts.push('value=•••'); }
    else if (/^(INPUT|TEXTAREA)$/.test(el.tagName) && el.value) parts.push('value=' + JSON.stringify(clean(el.value, 60)));
    else if (el.getAttribute('aria-checked')) parts.push('checked=' + el.getAttribute('aria-checked'));
    if (el.getAttribute('aria-expanded')) parts.push('expanded=' + el.getAttribute('aria-expanded'));
    if (el.disabled || el.getAttribute('aria-disabled') === 'true') parts.push('disabled');
    return parts.length ? ' ' + parts.join(' ') : '';
  };
  // Stable numbers: an element keeps its number while it stays in the page, so numbers from an earlier result still
  // work after other elements appear or vanish; new elements get new numbers. A new document starts again at 1.
  const refs = window.__octoRefs || (window.__octoRefs = []);
  const numberOf = (el) => { if (el.__octoRef && refs[el.__octoRef - 1] === el) return el.__octoRef; refs.push(el); el.__octoRef = refs.length; return refs.length; };
  const items = [];
  const footer = [];
  const extra = [];
  let closedFrames = 0;
  const walk = (root, where) => {
    const walker = (root.ownerDocument || root).createTreeWalker(root, NodeFilter.SHOW_ELEMENT);
    for (let el = walker.nextNode(); el; el = walker.nextNode()) {
      if ((el.matches(selector) || pointer(el)) && visible(el)) {
        const ref = numberOf(el);
        const tag = el.tagName.toLowerCase();
        const type = el.getAttribute('type');
        const href = tag === 'a' ? ' -> ' + clean(el.getAttribute('href'), 100) : '';
        if (el.closest('footer,[role=contentinfo]')) footer.push('[' + ref + '] ' + labelOf(el));
        else items.push('[' + ref + '] ' + tag + (type ? ':' + type : '') + ' "' + labelOf(el) + '"' + stateOf(el) + href + where);
      }
      if (el.shadowRoot) {
        walk(el.shadowRoot, where);
        const text = clean([...el.shadowRoot.children].map((child) => child.innerText || '').join(' '), 2000);
        if (text) extra.push(text);
      }
      if (el.tagName === 'IFRAME' || el.tagName === 'FRAME') {
        let doc = null;
        try { doc = el.contentDocument; } catch {}
        if (doc && doc.body) {
          walk(doc.body, ' (in frame)');
          const text = clean(doc.body.innerText, 4000);
          if (text) extra.push('[frame] ' + text);
        } else closedFrames += 1;
      }
    }
  };
  if (document.body) walk(document.body, '');
  if (closedFrames) extra.push('[' + closedFrames + ' cross-origin frame(s) not shown: navigate to the frame URL to use them]');
  const text = ((document.body ? document.body.innerText : '') + (extra.length ? '\\n\\n' + extra.join('\\n\\n') : '')).replace(/\\n{3,}/g, '\\n\\n').slice(0, 200000);
  // Which locale the page rendered for: sites localize prices by IP and browser language, so say what was shown.
  const counts = {};
  for (const match of text.matchAll(/[$€£₪¥₹₩₽₺₴₫฿]|\\b(?:USD|EUR|GBP|ILS|JPY|INR|AUD|CAD|CHF|CNY|BRL|MXN|NIS)\\b/g)) counts[match[0]] = (counts[match[0]] || 0) + 1;
  const currencies = Object.entries(counts).sort((a, b) => b[1] - a[1]).slice(0, 3).map(([symbol, count]) => symbol + ' (' + count + ')').join(', ');
  let timeZone = '';
  try { timeZone = Intl.DateTimeFormat().resolvedOptions().timeZone; } catch {}
  const locale = ['page lang ' + (document.documentElement.lang || '?'), 'browser ' + navigator.language, timeZone && 'time zone ' + timeZone, currencies && 'currency ' + currencies].filter(Boolean).join(' · ');
  return { title: document.title, url: location.href, text, items, footer, locale };
})()`;

/** The last snapshot per session, so an action on the same page reports what changed instead of the whole page. */
const lastSnapshot = new WeakMap<BrowserSession, SnapshotData>();

/** The page as the model sees it; with `changesOnly`, what changed since the last snapshot when the page is the same. */
export async function snapshot(session: BrowserSession, signal?: AbortSignal, options: { changesOnly?: boolean } = {}): Promise<string> {
  if (session.dialog) return dialogNotice(session.dialog);
  const data = await session.page.untilDialog(session.page.evaluate<SnapshotData | undefined>(SNAPSHOT_SCRIPT, signal));
  if (session.dialog) return dialogNotice(session.dialog);
  if (!data) throw new Error('Could not read the page; wait for it to load and take a snapshot again.');
  const previous = lastSnapshot.get(session);
  lastSnapshot.set(session, data);
  if (options.changesOnly && previous && previous.url === data.url && previous.title === data.title) return formatChanges(previous, data);
  const full = formatSnapshot(data);
  // A cut snapshot is saved whole, so the rest of the elements and text are a file read away.
  if (!full.includes('more characters cut') && !full.includes('more elements;')) return full;
  const file = saveFullOutput(formatSnapshot(data, Number.MAX_SAFE_INTEGER, Number.MAX_SAFE_INTEGER), 'browser-snapshot');
  return file ? `${full}\n[Full snapshot (every element and all text): ${shortPath(file)}; read it by line range or search it.]` : full;
}

/** Most new page text an action result shows; the rest is one snapshot away. */
const CHANGE_TEXT_MAX_CHARS = 3_000;

/** Element lines by their number (`[12] button "Go"` → `12`). */
const byNumber = (items: readonly string[]) => new Map(items.map((item) => [/^\[(\d+)\]/.exec(item)?.[1] ?? item, item]));

/**
 * An action's result on the same page: the elements that are new or changed (numbers are stable, so the rest of the
 * earlier list still holds), the numbers no longer shown, and the text lines that are new. Much smaller than a
 * snapshot, which stays one call away.
 */
export function formatChanges(before: SnapshotData, after: SnapshotData): string {
  const head = `# ${after.title}\n${after.url} (same page)`;
  const earlier = byNumber(before.items);
  const now = byNumber(after.items);
  const changed = [...now].filter(([ref, item]) => earlier.get(ref) !== item).map(([, item]) => item);
  const gone = [...earlier.keys()].filter((ref) => !now.has(ref));
  const elements =
    changed.length === 0 && gone.length === 0
      ? `Unchanged (${after.items.length}).`
      : `${capChars(changed.join('\n'), CHANGE_TEXT_MAX_CHARS, 'take a snapshot to see them all')}${gone.length ? `${changed.length ? '\n' : ''}No longer shown: ${gone.map((ref) => `[${ref}]`).join(' ')}` : ''}`;
  const seen = new Set(before.text.split('\n').map((line) => line.trim()));
  const added = after.text.split('\n').filter((line) => line.trim() && !seen.has(line.trim()));
  const kept = new Set(after.text.split('\n').map((line) => line.trim()));
  const removed = before.text.split('\n').filter((line) => line.trim() && !kept.has(line.trim())).length;
  const text = added.length > 0 ? capChars(added.join('\n'), CHANGE_TEXT_MAX_CHARS, 'take a snapshot to see the whole page') : 'No new text.';
  return `${head}\n\n## Elements${changed.length || gone.length ? ' (new or changed)' : ''}\n${elements}\n\n## New text${removed ? ` (${removed} line${removed === 1 ? '' : 's'} gone)` : ''}\n${text}`;
}

interface SnapshotData {
  title: string;
  url: string;
  text: string;
  items: string[];
  /** Footer links and controls, `[n] label`: listed compactly after the elements. */
  footer?: string[];
  /** The locale the page rendered for: page language, browser language, time zone, currency symbols seen. */
  locale?: string;
}

/** Most characters the compact footer line takes. */
const FOOTER_MAX_CHARS = 800;

/** Title, URL, the numbered elements (first, capped) and the page text in what is left of `max` characters. */
export function formatSnapshot(data: SnapshotData, max = SNAPSHOT_MAX_CHARS, elementsMax = ELEMENTS_MAX_CHARS): string {
  const head = `# ${data.title}\n${data.url}${data.locale ? `\nLocale: ${data.locale}` : ''}\n\n## Elements\n`;
  const elementBudget = Math.min(elementsMax, max - head.length);
  const kept: string[] = [];
  let used = 0;
  for (const item of data.items) {
    if (used + item.length + 1 > elementBudget) break;
    kept.push(item);
    used += item.length + 1;
  }
  const hidden = data.items.length - kept.length;
  const footer = data.footer?.length ? `\n\n## Footer (${data.footer.length})\n${capChars(data.footer.join(' · '), max === Number.MAX_SAFE_INTEGER ? max : FOOTER_MAX_CHARS, 'click by number still works')}` : '';
  const elements = `${kept.join('\n') || '(none)'}${hidden > 0 ? `\n[… ${hidden} more elements; click by number still works]` : ''}${footer}`;
  const top = `${head}${elements}\n\n## Text\n`;
  return `${top}${capChars(data.text, Math.max(0, max - top.length))}`;
}
