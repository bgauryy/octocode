import { bytes, stripTags } from './text.mjs';
import { classifyWorkflow as workflowHint } from './analyzers.mjs';

function attrValue(tag, name) {
  const m = String(tag || '').match(new RegExp(`${name}\\s*=\\s*(["'])(.*?)\\1`, 'i'));
  return m ? m[2] : '';
}

function attrs(tag) {
  const out = {};
  for (const m of String(tag || '').matchAll(/([A-Za-z_:][-A-Za-z0-9_:.]*)\s*=\s*(["'])(.*?)\2/g)) out[m[1].toLowerCase()] = m[3];
  return out;
}

function cssString(value) {
  return String(value || '').replace(/\\/g, '\\\\').replace(/"/g, '\\"');
}

function selectorFor(tag, a = {}, fallback = '') {
  const name = String(tag || '').toLowerCase();
  if (a.id) return `${name}#${cssString(a.id)}`;
  for (const key of ['data-testid', 'data-test', 'data-cy', 'data-qa']) {
    if (a[key]) return `${name}[${key}="${cssString(a[key])}"]`;
  }
  if (a.name) return `${name}[name="${cssString(a.name)}"]`;
  if (a['aria-label']) return `${name}[aria-label="${cssString(a['aria-label'])}"]`;
  if (a.href) return `${name}[href="${cssString(a.href)}"]`;
  if (a.action) return `${name}[action="${cssString(a.action)}"]`;
  if (a.type) return `${name}[type="${cssString(a.type)}"]`;
  return fallback || name;
}

// Structural, not fuzzy: HTML5's own rel=next/prev is the actual standard for pagination links.
// A "pagination" class name (pagination-nav, pager, ...) is the common fallback when rel is absent
// (e.g. Docusaurus). Checked against the isolated rel/class attribute values only — never against
// a JSON-stringified blob of a whole element, which is what made a text-pattern version of this
// false-positive on this codebase's own internal pageId bookkeeping field.
function isPaginationLink(relAttr, classAttr) {
  if (/\b(next|prev|previous)\b/i.test(relAttr || '')) return true;
  return /pagination|\bpager\b/i.test(classAttr || '');
}

export function extractLinksFromHtml(html, baseUrl, pageId) {
  const rows = [];
  const rx = /<a\b([^>]*)>([\s\S]*?)<\/a>/gi;
  let m;
  while ((m = rx.exec(html || ''))) {
    const href = attrValue(m[1], 'href');
    const text = stripTags(m[2]).slice(0, 300);
    const relAttr = attrValue(m[1], 'rel');
    const classAttr = attrValue(m[1], 'class');
    const hint = isPaginationLink(relAttr, classAttr) ? 'pagination' : workflowHint(`${href} ${text}`);
    const linkAttrs = attrs(m[1]);
    const selector = selectorFor('a', linkAttrs, href ? `a[href="${cssString(href)}"]` : 'a');
    try { rows.push({ pageId, href: new URL(href, baseUrl).href, text, selector, source: 'html', workflowHint: hint }); }
    catch { rows.push({ pageId, href, text, selector, source: 'html', workflowHint: hint }); }
    if (rows.length >= 1000) break;
  }
  return rows;
}

export function extractLinksFromMarkdown(markdown, baseUrl, pageId) {
  const rows = [];
  const rx = /\[([^\]]*)\]\(([^)\s]+)(?:\s+"[^"]*")?\)/g;
  let m;
  while ((m = rx.exec(markdown || ''))) {
    try { rows.push({ pageId, href: new URL(m[2], baseUrl).href, text: m[1].trim().slice(0, 300), source: 'markdown', workflowHint: workflowHint(`${m[2]} ${m[1]}`) }); }
    catch { rows.push({ pageId, href: m[2], text: m[1].trim().slice(0, 300), source: 'markdown', workflowHint: workflowHint(`${m[2]} ${m[1]}`) }); }
    if (rows.length >= 1000) break;
  }
  return rows;
}

export function extractHeadings(markdown, pageId) {
  return (markdown || '')
    .split(/\r?\n/)
    .map((line) => line.match(/^(#{1,6})\s+(.+?)\s*#*\s*$/))
    .filter(Boolean)
    .map((m) => ({ pageId, level: m[1].length, text: m[2].trim().slice(0, 500), source: 'markdown', workflowHint: workflowHint(m[2]) }));
}

export function extractMetaFromHtml(html, pageId) {
  const rows = [];
  for (const m of String(html || '').matchAll(/<meta\b[^>]*>/gi)) {
    const a = attrs(m[0]);
    const key = a.name || a.property || a['http-equiv'] || a.charset || null;
    if (key) rows.push({ pageId, key, content: (a.content || a.charset || '').slice(0, 1000), source: 'html', workflowHint: workflowHint(`${key} ${a.content || ''}`) });
  }
  return rows.slice(0, 200);
}

export function extractCanonicalFromHtml(html, baseUrl, pageId) {
  const rows = [];
  for (const m of String(html || '').matchAll(/<link\b[^>]*>/gi)) {
    const a = attrs(m[0]);
    if (String(a.rel || '').toLowerCase().split(/\s+/).includes('canonical') && a.href) {
      try { rows.push({ pageId, href: new URL(a.href, baseUrl).href, source: 'html' }); }
      catch { rows.push({ pageId, href: a.href, source: 'html' }); }
    }
  }
  return rows;
}

export function extractJsonLdFromHtml(html, pageId) {
  const rows = [];
  const rx = /<script\b[^>]*type=["']application\/ld\+json["'][^>]*>([\s\S]*?)<\/script>/gi;
  let m;
  while ((m = rx.exec(html || ''))) {
    let parsed = null;
    try { parsed = JSON.parse(m[1].trim()); } catch {}
    rows.push({ pageId, type: parsed?.['@type'] || null, json: parsed, textBytes: bytes(m[1]), source: 'html', workflowHint: workflowHint(JSON.stringify(parsed || {}).slice(0, 1000)) });
  }
  return rows.slice(0, 100);
}

export function extractFormsFromHtml(html, baseUrl, pageId) {
  const rows = [];
  const rx = /<form\b([^>]*)>([\s\S]*?)<\/form>/gi;
  let m;
  while ((m = rx.exec(html || ''))) {
    const a = attrs(m[1]);
    const body = m[2] || '';
    const inputs = [...body.matchAll(/<(input|select|textarea)\b([^>]*)>/gi)].map((x) => { const inputAttrs = attrs(x[2]); return { tag: x[1].toLowerCase(), name: inputAttrs.name || null, type: inputAttrs.type || x[1].toLowerCase(), placeholder: inputAttrs.placeholder || null, selector: selectorFor(x[1], inputAttrs) }; }).slice(0, 50);
    const labels = [...body.matchAll(/<label\b[^>]*>([\s\S]*?)<\/label>/gi)].map((x) => stripTags(x[1]).slice(0, 200)).filter(Boolean).slice(0, 50);
    let action = a.action || '';
    try { if (action) action = new URL(action, baseUrl).href; } catch {}
    rows.push({ pageId, kind: 'form', action: action || null, method: (a.method || 'get').toLowerCase(), selector: selectorFor('form', a), labels, inputs, inputTypes: inputs.map((i) => i.type).filter(Boolean), workflowHint: workflowHint(`${action} ${labels.join(' ')} ${inputs.map((i) => i.name || '').join(' ')}`), source: 'html' });
  }
  return rows.slice(0, 100);
}

export function extractButtonsFromHtml(html, baseUrl, pageId) {
  const rows = [];
  for (const m of String(html || '').matchAll(/<button\b([^>]*)>([\s\S]*?)<\/button>/gi)) {
    const text = stripTags(m[2]).slice(0, 300);
    const buttonAttrs = attrs(m[1]);
    rows.push({ pageId, kind: 'button', text, selector: selectorFor('button', buttonAttrs), type: buttonAttrs.type || null, href: null, workflowHint: workflowHint(text), source: 'html' });
  }
  for (const m of String(html || '').matchAll(/<a\b([^>]*)>([\s\S]*?)<\/a>/gi)) {
    const text = stripTags(m[2]).slice(0, 300);
    const hrefRaw = attrValue(m[1], 'href');
    const hint = workflowHint(`${hrefRaw} ${text}`);
    if (!hint || !/signup|login|pricing|checkout|contact|support/.test(hint)) continue;
    let href = hrefRaw;
    try { href = new URL(hrefRaw, baseUrl).href; } catch {}
    const linkAttrs = attrs(m[1]);
    rows.push({ pageId, kind: 'cta-link', text, selector: selectorFor('a', linkAttrs, hrefRaw ? `a[href="${cssString(hrefRaw)}"]` : 'a'), type: null, href, workflowHint: hint, source: 'html' });
  }
  return rows.slice(0, 300);
}

export function extractTablesFromHtml(html, pageId) {
  const rows = [];
  const rx = /<table\b[^>]*>([\s\S]*?)<\/table>/gi;
  let m;
  while ((m = rx.exec(html || ''))) {
    const table = m[1];
    const headers = [...table.matchAll(/<th\b[^>]*>([\s\S]*?)<\/th>/gi)].map((x) => stripTags(x[1]).slice(0, 200)).filter(Boolean);
    const rowCount = [...table.matchAll(/<tr\b[^>]*>/gi)].length;
    const preview = stripTags(table).slice(0, 500);
    rows.push({ pageId, kind: 'table', selector: `table:nth-of-type(${rows.length + 1})`, headers, rowCount, preview, workflowHint: workflowHint(`${headers.join(' ')} ${preview}`), source: 'html' });
  }
  return rows.slice(0, 100);
}

// Purely structural attribute extraction — same class of operation as extractFormsFromHtml/
// extractButtonsFromHtml above, not a classifier. No workflowHint: a resource's kind (script,
// stylesheet, image, media, feed) is read directly off the tag, never guessed from text.
export function extractResourcesFromHtml(html, baseUrl, pageId) {
  const rows = [];
  const push = (kind, src) => {
    if (!src) return;
    try { rows.push({ pageId, kind, src: new URL(src, baseUrl).href, source: 'html' }); }
    catch { rows.push({ pageId, kind, src, source: 'html' }); }
  };
  for (const m of String(html || '').matchAll(/<script\b([^>]*)>/gi)) push('script', attrValue(m[1], 'src'));
  for (const m of String(html || '').matchAll(/<link\b([^>]*)>/gi)) {
    const rel = attrValue(m[1], 'rel').toLowerCase();
    const type = attrValue(m[1], 'type').toLowerCase();
    if (rel.split(/\s+/).includes('stylesheet')) push('stylesheet', attrValue(m[1], 'href'));
    else if (/rss\+xml|atom\+xml/.test(type)) push('feed', attrValue(m[1], 'href'));
  }
  for (const m of String(html || '').matchAll(/<img\b([^>]*)>/gi)) push('image', attrValue(m[1], 'src'));
  for (const m of String(html || '').matchAll(/<(?:video|audio|source)\b([^>]*)>/gi)) push('media', attrValue(m[1], 'src'));
  return rows.slice(0, 500);
}

export function extractCodeBlocksFromMarkdown(markdown, pageId) {
  const rows = [];
  const rx = /```([^\n`]*)\n([\s\S]*?)```/g;
  let m;
  while ((m = rx.exec(markdown || ''))) rows.push({ pageId, kind: 'code-block', language: (m[1] || '').trim() || null, textPreview: m[2].trim().slice(0, 1000), textBytes: bytes(m[2]), workflowHint: workflowHint(m[2]), source: 'markdown' });
  return rows.slice(0, 300);
}

export function redactedCookies(value, pageId) {
  if (!value) return [];
  return String(value).split(';').map((p) => p.trim().split('=')[0]).filter(Boolean).map((name) => ({ pageId, name, value: '<redacted>' }));
}

export function extendedExtractFiles(parsedJson, pageId) {
  const files = [];
  if (!parsedJson) return files;
  if (Array.isArray(parsedJson.headers)) files.push([`${pageId}-headers.jsonl`, parsedJson.headers.map((r) => ({ pageId, ...r }))]);
  if (Array.isArray(parsedJson.xhrs)) files.push([`${pageId}-xhrs.jsonl`, parsedJson.xhrs.map((r) => ({ pageId, ...r }))]);
  if (Array.isArray(parsedJson.iframes)) files.push([`${pageId}-iframes.jsonl`, parsedJson.iframes.map((r) => ({ pageId, src: r.src, htmlBytes: bytes(r.html) }))]);
  files.push([`${pageId}-cookies.redacted.jsonl`, redactedCookies(parsedJson.cookies, pageId)]);
  return files;
}

export function jsonl(rows) {
  return rows.map((r) => JSON.stringify(r)).join('\n') + (rows.length ? '\n' : '');
}
