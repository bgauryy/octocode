export function bytes(s) { return Buffer.byteLength(s || ''); }

export function chunkTextByBytes(text, maxBytes) {
  if (!Number.isSafeInteger(maxBytes) || maxBytes < 4) throw new Error('chunk byte size must be an integer >= 4');
  const data = Buffer.from(text || ''), chunks = []; let at = 0;
  while (at < data.length) {
    let end = Math.min(data.length, at + maxBytes);
    while (end < data.length && (data[end] & 0xc0) === 0x80) end--;
    const part = data.subarray(at, end).toString('utf8');
    const boundary = Math.max(part.lastIndexOf('\n\n'), part.lastIndexOf('\n'), part.lastIndexOf(' '));
    if (end < data.length && boundary > Math.floor(part.length * 0.6)) end = at + Buffer.byteLength(part.slice(0, boundary + 1));
    chunks.push(data.subarray(at, end).toString('utf8')); at = end;
  }
  return chunks.length ? chunks : [''];
}

export function decodeEntities(s) {
  return (s || '').replace(/&nbsp;/g, ' ').replace(/&amp;/g, '&').replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&quot;/g, '"').replace(/&#39;/g, "'");
}

export function stripTags(html) {
  return decodeEntities(html || '')
    .replace(/<script[\s\S]*?<\/script>/gi, ' ')
    .replace(/<style[\s\S]*?<\/style>/gi, ' ')
    .replace(/<[^>]+>/g, ' ')
    .replace(/\s+/g, ' ')
    .trim();
}

/** HTML → line-structured text: block tags become line breaks, headings become `#` lines. */
export function htmlToText(html) {
  const text = (html || '')
    .replace(/<(script|style|noscript|svg|template)\b[\s\S]*?<\/\1>/gi, ' ')
    .replace(/<h([1-6])\b[^>]*>/gi, (_, n) => `\n\n${'#'.repeat(Number(n))} `)
    .replace(/<li\b[^>]*>/gi, '\n- ')
    .replace(/<br\s*\/?>/gi, '\n')
    .replace(/<\/?(p|div|section|article|main|header|footer|nav|aside|pre|blockquote|table|thead|tbody|tr|ul|ol|dl|dt|dd|h[1-6]|figure|figcaption|details|summary)\b[^>]*>/gi, '\n')
    .replace(/<[^>]+>/g, ' ');
  return decodeEntities(text)
    .split('\n')
    .map((line) => line.replace(/[ \t\f\v\u00a0]+/g, ' ').trim())
    .join('\n')
    .replace(/\n{3,}/g, '\n\n')
    .trim();
}

export function titleFromHtml(text) {
  const m = (text || '').match(/<title[^>]*>([\s\S]*?)<\/title>/i);
  return m ? stripTags(m[1]).trim().slice(0, 300) : '';
}

export function cleanForAgent(text) {
  const original = text || '';
  let lines = original.replace(/\r\n/g, '\n').split('\n').map((line) => line.replace(/[ \t]+$/g, ''));
  const h1AfterOnThisPage = lines.findIndex((line, i) => i > 0 && /^#\s+\S/.test(line) && lines.slice(Math.max(0, i - 8), i).some((prev) => /^On this page\s*$/.test(prev)));
  const firstH1 = lines.findIndex((line) => /^#\s+\S/.test(line));
  let start = h1AfterOnThisPage >= 0 ? h1AfterOnThisPage : firstH1 >= 0 ? firstH1 : 0;
  // Drop only navigation before the heading; a prose line there is content.
  if (start > 0 && lines.slice(0, start).some((line) => line.trim().length >= 100)) start = 0;
  lines = lines.slice(start);
  // Cut a footer only in the last fifth of the page, so a mid-document "© 2024" or "[Previous" line keeps what follows.
  const total = lines.reduce((n, line) => n + line.length + 1, 0);
  let offset = 0;
  let end = -1;
  for (let i = 0; i < lines.length; i++) {
    if (i > 0 && offset >= total * 0.8 && (lines[i].startsWith('[Previous') || /^©\s+\d{4}\b/.test(lines[i]) || /^####\s+Product\s*$/.test(lines[i]))) { end = i; break; }
    offset += lines[i].length + 1;
  }
  if (end > 0) lines = lines.slice(0, end);
  lines = lines.filter((line) => !/^(Skip to main content|Search`Ctrl``K`|Version:\s*v\d+|On this page)\s*$/.test(line));
  return lines.join('\n').replace(/\n{3,}/g, '\n\n').trim() || original.trim();
}

const payloadText = (text, json) => String(text || json?.markdown || json?.text || json?.detail || '');

export function detectTargetError({ status, providerStatus, text, json }) {
  const full = payloadText(text, json);
  const lines = full.slice(0, 5000).split(/\r?\n/).map((line) => line.trim()).filter(Boolean);
  const firstMeaningful = lines[0] || '';
  if (status >= 400) return `provider HTTP ${status}${json?.detail ? `: ${String(json.detail).slice(0, 160)}` : ''}`;
  if (Number.isFinite(providerStatus) && providerStatus >= 400) return `target HTTP ${providerStatus}`;
  if (/^#?\s*(404|403|401|410|429|500|502|503|504)\b/i.test(firstMeaningful)) return `target likely returned ${firstMeaningful.slice(0, 80)}`;
  // Error phrases count only near the top of a short page; a docs page that explains errors is not one.
  const shortPage = full.replace(/\s+/g, ' ').trim().length < 1500;
  if (shortPage && /\b(404\s+not\s+found|access\s+denied|forbidden|rate\s+limit|temporarily\s+unavailable)\b/i.test(lines.slice(0, 3).join('\n'))) return 'target likely returned an error page';
  return null;
}

export function detectBrowserNeed({ status, contentType, body, cleanText, targetLikelyError }) {
  if (status === 403 || status === 423) return `direct HTTP returned ${status}; one live-browser diagnostic may distinguish rendering from a bot wall`;
  if (targetLikelyError || status < 200 || status >= 300 || !/html/i.test(contentType || '')) return null;
  const visible = String(cleanText || '').replace(/\s+/g, ' ').trim();
  if (visible.length < 300 && appShell(String(body || ''))) return `direct HTML contains only ${visible.length} visible characters plus an application shell; render once in Chrome`;
  return null;
}

// One analytics <script src> on a static page is not an app shell; require a framework mount, a module
// bundle, a "needs JavaScript" notice, or several external scripts.
function appShell(html) {
  const frameworkMount = /\b(?:__NEXT_DATA__|__NUXT__|data-reactroot|ng-version|data-server-rendered)\b|id=["'](?:root|app|__next|__nuxt)["']/i.test(html);
  const moduleBundle = /<script\b[^>]*type=["']module["']/i.test(html);
  const needsJs = /<noscript\b[^>]*>[\s\S]{0,400}?\b(?:enable|requires?|turn on)\b[\s\S]{0,40}?javascript/i.test(html);
  const externalScripts = (html.match(/<script\b[^>]*\bsrc=/gi) || []).length;
  return frameworkMount || moduleBundle || needsJs || externalScripts >= 3;
}

const markdownText = (json, body) => json?.markdown ?? json?.text ?? (json ? JSON.stringify(json, null, 2) : body);
const extendedText = (json) => (json.text ?? htmlToText(json.html ?? json.content ?? '')) || JSON.stringify(json, null, 2);

export function parsePayload(mode, contentType, body) {
  let json = null;
  try { json = JSON.parse(body || ''); } catch { /* not JSON: the body is the text */ }
  if (mode === 'markdown') return { json, text: markdownText(json, body) };
  if (mode === 'extended' && json) return { json, text: extendedText(json) };
  if (mode === 'extract' && json) return { json, text: JSON.stringify(json, null, 2) };
  if (/html/i.test(contentType)) return { json, text: /<[a-z!]/i.test(body || '') ? htmlToText(body) : (body || '') };
  return { json, text: json ? JSON.stringify(json, null, 2) : body };
}
