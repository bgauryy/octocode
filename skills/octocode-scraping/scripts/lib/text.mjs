export function bytes(s) { return Buffer.byteLength(s || ''); }

export function truncate(text, maxBytes) {
  const buf = Buffer.from(text || '');
  if (buf.length <= maxBytes) return { text: text || '', truncated: false, bytes: buf.length };
  return { text: buf.subarray(0, maxBytes).toString('utf8'), truncated: true, bytes: buf.length };
}

export function chunkTextByBytes(text, maxBytes) {
  const chunks = [];
  let rest = text || '';
  while (Buffer.byteLength(rest) > maxBytes) {
    let slice = Buffer.from(rest).subarray(0, maxBytes).toString('utf8');
    const boundary = Math.max(slice.lastIndexOf('\n\n'), slice.lastIndexOf('\n'), slice.lastIndexOf(' '));
    if (boundary > Math.floor(maxBytes * 0.6)) slice = slice.slice(0, boundary);
    chunks.push(slice);
    rest = rest.slice(slice.length).replace(/^\s+/, '');
  }
  chunks.push(rest);
  return chunks;
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

export function parsePayload(mode, contentType, body) {
  let json = null;
  try { json = JSON.parse(body || ''); } catch {}
  if (mode === 'markdown') return { json, text: json?.markdown ?? json?.text ?? (json ? JSON.stringify(json, null, 2) : body) };
  if (mode === 'extended' && json) return { json, text: (json.text ?? stripTags(json.html ?? json.content ?? '')) || JSON.stringify(json, null, 2) };
  if (mode === 'extract' && json) return { json, text: JSON.stringify(json, null, 2) };
  if (/html/i.test(contentType)) return { json, text: stripTags(body) };
  return { json, text: json ? JSON.stringify(json, null, 2) : body };
}
