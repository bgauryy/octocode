import { copyFileSync, rmSync } from 'node:fs';
import { dirname, isAbsolute, resolve } from 'node:path';

// Canonicalize only the explicitly declared answer type, never free prose.
export function canonical(value, expected) {
  if (typeof value !== 'string') return null;
  const text = value.trim();
  if (expected.valueType === 'identifiers') {
    const parts = text.split(',').map(part => part.trim());
    return parts.every(part => /^[A-Za-z_][A-Za-z_0-9]*$/.test(part)) ? parts.join(',') : null;
  }
  if (expected.valueType === 'variant') {
    const parts = text.split('::');
    if (!parts.every(part => /^[A-Za-z_][A-Za-z_0-9]*$/.test(part))) return null;
    if (parts.length > 1 && parts.at(-2) !== expected.enumType) return null;
    return parts.at(-1);
  }
  if (expected.valueType === 'line') return /^[1-9][0-9]*$/.test(text) ? text : null;
  return text;
}

const samePath = (path, expected, base) => typeof path === 'string' &&
  (isAbsolute(path) || typeof base === 'string') && resolve(base ?? '/', path) === expected;
const sameLine = (text, expected) => typeof text === 'string' && text.trim() === expected.sourceLine.trim();

function structuredEvidence(envelope, call, expected) {
  const queries = call.args?.queries ?? [call.args];
  return envelope?.results?.some(row => {
    if (row.error || row.status === 'error') return false;
    const query = queries[row.index ?? 0];
    const data = row.data ?? {};
    if (call.name === 'localFetch') {
      if (!samePath(query?.path, expected.path) ||
          (data.path && !samePath(data.path, expected.path, envelope.base ?? dirname(query.path)))) return false;
      // A range alone is insufficient: tie the actual statement to its exact line.
      const ranges = data.sourceLineRanges ?? [];
      const lines = data.content?.split('\n').filter(line => !/^\.\.\. \[lines \d+-\d+ omitted\] \.\.\.$/.test(line));
      if (!lines || !ranges.length) return false;
      const count = ranges.reduce((sum, range) => sum + range.end - range.start + 1, 0);
      if (lines.length === count + 1 && lines.at(-1) === '') lines.pop();
      if (lines.length !== count) return false;
      let offset = 0;
      for (const range of ranges) {
        if (range.start <= expected.line && range.end >= expected.line && sameLine(lines[offset + expected.line - range.start], expected)) return true;
        offset += range.end - range.start + 1;
      }
    }
    if (call.name === 'localSearch') return data.files?.some(file =>
      samePath(file.path, expected.path, envelope.base ?? query?.path) && file.matches?.some(match =>
        match.line === expected.line && sameLine(match.value, expected)));
    return false;
  }) ?? false;
}

function numberedEvidence(text, call, expected) {
  if (call.name !== 'localFetch') return false;
  const queries = call.args?.queries ?? [call.args];
  const pagination = call.result?.structuredContent?.responsePagination;
  // A later text page can begin inside the numbered body. A sole query binds
  // that continuation; multi-row pages still require an observed row header.
  const continuation = queries.length === 1 && pagination?.scope === 'content.text' && pagination.currentPage > 1;
  let index = continuation ? 0 : null, path = continuation ? queries[0]?.path : undefined, inSource = continuation;
  let base = call.result?.structuredContent?.base;
  for (const line of text.split('\n')) {
    const baseMatch = /^base: (.+)$/.exec(line);
    if (baseMatch) base = baseMatch[1];
    const row = /^result: (\d+)(?:\s.*)?$/.exec(line);
    if (row) { index = Number(row[1]); path = undefined; inSource = false; continue; }
    const file = /^  path: (.+)$/.exec(line);
    if (file && !inSource) path = file[1];
    if (line === 'content (source lines):') { inSource = true; continue; }
    if (!inSource || index === null) continue;
    const numbered = /^(\d+): (.*)$/.exec(line);
    const query = queries[index];
    if (numbered && Number(numbered[1]) === expected.line && sameLine(numbered[2], expected) &&
        samePath(query?.path, expected.path) && samePath(path, expected.path, base ?? dirname(query.path))) return true;
  }
  return false;
}

export function grade(expected, answer, calls) {
  if (!expected.sourceLine || !Array.isArray(answer?.items) || answer.items.length !== 1) return false;
  const item = answer.items[0];
  const value = canonical(item.value, expected);
  if (item.id !== expected.id || value === null || value !== canonical(expected.value, expected)) return false;
  if (!item.evidence?.some(cite => isAbsolute(cite.path ?? '') && samePath(cite.path, expected.path) && cite.line === expected.line)) return false;
  return calls.some(call => {
    if (call.event !== 'call' || !call.admitted || call.result?.isError || !['localFetch', 'localSearch'].includes(call.name)) return false;
    if (structuredEvidence(call.result?.structuredContent, call, expected)) return true;
    return call.result?.content?.some(block => {
      if (block.type !== 'text') return false;
      try { if (structuredEvidence(JSON.parse(block.text), call, expected)) return true; } catch { /* Numbered runtime renderer below. */ }
      return numberedEvidence(block.text, call, expected);
    }) ?? false;
  });
}

export async function withCredential(source, destination, action) {
  try {
    copyFileSync(source, destination);
    return await action();
  } finally {
    rmSync(destination, { force: true });
  }
}
