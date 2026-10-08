export function bytes(s) { return Buffer.byteLength(s || ''); }

export function truncate(text, maxBytes) {
  const buf = Buffer.from(text || '');
  if (buf.length <= maxBytes) return { text: text || '', truncated: false, bytes: buf.length };
  return { text: buf.subarray(0, maxBytes).toString('utf8'), truncated: true, bytes: buf.length };
}

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
  const start = h1AfterOnThisPage >= 0 ? h1AfterOnThisPage : firstH1 >= 0 ? firstH1 : 0;
  lines = lines.slice(start);
  const end = lines.findIndex((line) => /^\[Previous/.test(line) || /^©\s+\d{4}\b/.test(line) || /^####\s+Product\s*$/.test(line));
  if (end > 0) lines = lines.slice(0, end);
  lines = lines.filter((line) => !/^(Skip to main content|Search`Ctrl``K`|Version:\s*v\d+|On this page)\s*$/.test(line));
  return lines.join('\n').replace(/\n{3,}/g, '\n\n').trim() || original.trim();
}

export function detectTargetError({ status, providerStatus, text, json }) {
  const value = (text || json?.markdown || json?.text || json?.detail || '').slice(0, 5000);
  const firstMeaningful = value.split(/\r?\n/).map((line) => line.trim()).find(Boolean) || '';
  const detail = json?.detail ? `: ${String(json.detail).slice(0, 160)}` : '';
  if (status >= 400) return `provider HTTP ${status}${detail}`;
  if (Number.isFinite(providerStatus) && providerStatus >= 400) return `target HTTP ${providerStatus}`;
  if (/^#?\s*(404|403|401|410|429|500|502|503|504)\b/i.test(firstMeaningful)) return `target likely returned ${firstMeaningful.slice(0, 80)}`;
  if (/\b(404\s+not\s+found|access\s+denied|forbidden|rate\s+limit|temporarily\s+unavailable)\b/i.test(value)) return 'target likely returned an error page';
  return null;
}

export function detectBrowserNeed({ status, contentType, body, cleanText, targetLikelyError }) {
  if (status === 403 || status === 423) return `direct HTTP returned ${status}; one live-browser diagnostic may distinguish rendering from a bot wall`;
  if (targetLikelyError || status < 200 || status >= 300 || !/html/i.test(contentType || '')) return null;
  const visible = String(cleanText || '').replace(/\s+/g, ' ').trim();
  const html = String(body || '');
  const appShell = /<script\b[^>]*(?:src=|type=["']module)|\b(?:__NEXT_DATA__|__NUXT__|data-reactroot|id=["'](?:root|app)["'])/i.test(html);
  if (visible.length < 300 && appShell) return `direct HTML contains only ${visible.length} visible characters plus an application shell; render once in Chrome`;
  return null;
}

export function parsePayload(mode, contentType, body) {
  let json = null;
  try { json = JSON.parse(body || ''); } catch {}
  if (mode === 'markdown') return { json, text: json?.markdown ?? json?.text ?? (json ? JSON.stringify(json, null, 2) : body) };
  if (mode === 'extended' && json) return { json, text: (json.text ?? htmlToText(json.html ?? json.content ?? '')) || JSON.stringify(json, null, 2) };
  if (mode === 'extract' && json) return { json, text: JSON.stringify(json, null, 2) };
  if (/html/i.test(contentType)) return { json, text: /<[a-z!]/i.test(body || '') ? htmlToText(body) : (body || '') };
  return { json, text: json ? JSON.stringify(json, null, 2) : body };
}
