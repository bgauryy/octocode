// An independent reading of the search fields over known text: a small grep in
// JavaScript. It shares no code with the tool, so a dropped or misread field shows
// as a difference; it needs only what the fixture's patterns use.
import { textOf, testModeKeeps, testModeKeepsLine, visibleFiles } from './world-local.mjs';

const escapeRegex = text => text.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
const isCased = pattern => /[A-Z]/.test(pattern.replace(/\\./g, ''));

export function searchRegex(query) {
  const raw = query.matchString[0];
  const source = (query.regexEngine ?? 'literal') === 'literal' ? escapeRegex(raw) : raw;
  const mode = query.caseMode ?? 'smart';
  const sensitive = mode === 'sensitive' || (mode === 'smart' && isCased(raw));
  const flags = ['g', sensitive ? '' : 'i', 'u', query.multiline === 'dotall' ? 's' : ''].join('');
  return new RegExp(query.wholeWord ? `(?<![\\p{L}\\p{N}_])(?:${source})(?![\\p{L}\\p{N}_])` : source, flags);
}

const lineStarts = text => {
  const starts = [0];
  for (let index = text.indexOf('\n'); index >= 0; index = text.indexOf('\n', index + 1)) starts.push(index + 1);
  return starts;
};
const lineAt = (starts, offset) => {
  let low = 0, high = starts.length - 1;
  while (low < high) { const mid = (low + high + 1) >> 1; if (starts[mid] <= offset) low = mid; else high = mid - 1; }
  return low + 1;
};
const linesOf = text => { const lines = text.split('\n'); if (lines.at(-1) === '') lines.pop(); return lines; };

const wholeLine = value => ({ line: value.line, last: value.line, text: value.text, hits: [value.text] });

/** One file's hits: `{ line, text, hits }` per selected line group, `hits` the matched strings. */
export function fileHits(file, query) {
  const text = textOf(file);
  const regex = searchRegex(query);
  const lines = linesOf(text);
  const multi = (query.multiline ?? 'off') !== 'off';
  if (multi) {
    const starts = lineStarts(text);
    const spans = [...text.matchAll(regex)].filter(match => match[0] !== '').map(match => {
      const first = lineAt(starts, match.index);
      const last = lineAt(starts, match.index + match[0].length - 1);
      return { line: first, last, text: lines.slice(first - 1, last).join('\n'), hits: [match[0]] };
    });
    if (!query.invertMatch) return spans;
    return lines.map((value, index) => ({ line: index + 1, text: value })).filter(row => !spans.some(span => span.line <= row.line && row.line <= span.last)).map(wholeLine);
  }
  const rows = lines.map((value, index) => ({ line: index + 1, last: index + 1, text: value, hits: [...value.matchAll(regex)].map(match => match[0]).filter(Boolean) }));
  // An inverted hit is the whole selected line, in every view.
  return query.invertMatch ? rows.filter(row => !row.hits.length).map(wholeLine) : rows.filter(row => row.hits.length);
}

/** Files and hits a query selects, in the fixture's terms. */
export function selected(root, query) {
  const mode = query.testCode ?? 'include';
  return visibleFiles(root, query).filter(file => testModeKeeps(mode, file)).map(file => ({
    file,
    hits: fileHits(file, query).filter(hit => testModeKeepsLine(mode, file, hit.line)),
  }));
}

/** The evidence a view must show. */
export function searchTruth(root, query) {
  const view = query.view ?? 'content';
  const files = selected(root, query);
  const withHits = files.filter(entry => entry.hits.length);
  switch (view) {
    case 'content': return withHits.flatMap(({ file, hits }) => hits.map(hit => [file, hit.line, hit.text]));
    case 'files': return withHits.map(({ file }) => [file]);
    case 'filesWithout': return files.filter(entry => !entry.hits.length).map(({ file }) => [file]);
    case 'countLines': return withHits.map(({ file, hits }) => [file, hits.reduce((sum, hit) => sum + hit.last - hit.line + 1, 0)]);
    case 'countMatches': return withHits.map(({ file, hits }) => [file, hits.reduce((sum, hit) => sum + Math.max(hit.hits.length, 1), 0)]);
    case 'matchOnly': return withHits.flatMap(({ file, hits }) => {
      const all = hits.flatMap(hit => hit.hits);
      const unique = query.unique ?? 'off';
      if (unique === 'off') return all.map(value => [file, value]);
      return [...new Set(all)].map(value => (unique === 'count' ? [file, value, all.filter(hit => hit === value).length] : [file, value]));
    });
    default: throw new Error(`no oracle for localSearch view ${view}`);
  }
}
