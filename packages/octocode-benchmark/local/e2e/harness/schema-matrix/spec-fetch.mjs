// localFetch and ghGetFileContent: one reading of the view fields against known lines.
// The GitHub tool reads the same files from a loopback repository, so both share an oracle.
import { sourceLines } from '../responses.mjs';
import { classificationBase, longContextProblem } from './classification-rows.mjs';
import { searchRegex } from './oracle-grep.mjs';
import { textOf } from './world-local.mjs';

const FILE = 'src/alpha.ts';
const PATTERNS = { literal: { lower: 'a.pha', mixed: 'Alpha' }, rust: { lower: 'a.pha', mixed: 'A.pha' }, pcre2: { lower: 'a.pha(?!bet)', mixed: 'A.pha(?!bet)' } };
const BLOCK_HEAD = 'export function alphaHelper';
/** Hand-read from the fixture: the declaration around lines 6-8, and every outline row. */
const HELPER_BLOCK = [6, 7, 8];
const OUTLINE = [[2, 2], [3, 5], [6, 8], [9, 9], [10, 10], [11, 11]];
const COMMENT_LINES = new Set([1]);

const source = () => { const lines = textOf(FILE).split('\n'); if (lines.at(-1) === '') lines.pop(); return lines; };
const inRanges = (ranges, line) => !ranges || ranges.some(range => range.startLine <= line && line <= range.endLine);
const squeeze = text => text.replace(/\s+/g, '');

function viewFields(row, assignment) {
  const block = row.block === true;
  const ranges = block || assignment.span === 'part' ? { ranges: [{ startLine: 7, endLine: 7 }] } : {};
  return ranges;
}

function base(tool, row, assignment) {
  if (row.readProgress) return { ...tool.at(FILE), readProgress: { offset: 0, ...row.readProgress } };
  const view = row.view ?? 'content';
  if (view === 'symbols') return tool.at(FILE);
  const searching = row.regexEngine || row.caseMode || row.wholeWord !== undefined;
  if (searching) {
    const engine = row.regexEngine ?? 'literal';
    const pattern = row.block ? BLOCK_HEAD : PATTERNS[engine][assignment.casing ?? 'lower'];
    return { ...tool.at(FILE), matchString: [pattern], ...classificationBase(row, assignment) };
  }
  return { ...tool.at(FILE), ...viewFields(row, assignment), ...classificationBase(row, assignment) };
}

function contentTruth(query) {
  const lines = source().map((text, index) => [index + 1, text]);
  if (query.matchString) {
    if (query.block) return lines.filter(([line]) => HELPER_BLOCK.includes(line));
    return lines.filter(([, text]) => text.match(searchRegex(query)));
  }
  if (query.block) return lines.filter(([line]) => HELPER_BLOCK.includes(line));
  return lines.filter(([line]) => inRanges(query.ranges, line));
}

function compactTruth(query) {
  const wanted = query.block ? HELPER_BLOCK : null;
  const kept = source().map((text, index) => [index + 1, text]).filter(([line]) => !COMMENT_LINES.has(line)
    && (wanted ? wanted.includes(line) : inRanges(query.ranges, line)));
  return [['tokens', kept.map(([, text]) => squeeze(text)).join('')]];
}

function truth(ctx, testCase) {
  const { query } = testCase;
  const view = query.view ?? 'content';
  if (view === 'symbols') return OUTLINE.map(([start, end]) => [start, end]);
  return view === 'compact' ? compactTruth(query) : contentTruth(query);
}

const dataOf = pages => pages.flatMap(page => page.json?.results ?? []).map(row => row.data ?? {});

function observed(ctx, testCase, pages) {
  const view = testCase.query.view ?? 'content';
  const data = dataOf(pages);
  if (view === 'symbols') return data.flatMap(row => (row.content ?? '').split('\n').filter(Boolean)).map(line => /^(\d+)(?:-(\d+))?\t/.exec(line)).filter(Boolean).map(match => [Number(match[1]), Number(match[2] ?? match[1])]);
  const lines = data.flatMap(sourceLines);
  if (view === 'compact') return [['tokens', lines.map(([, text]) => squeeze(text)).join('')]];
  return lines;
}

const check = (ctx, testCase) => longContextProblem(ctx, testCase.query);
const refusal = (ctx, testCase) => (testCase.query.readProgress ? 'invalidInput' : null);

const spec = (name, at) => ({
  tool: name,
  pseudo: [{ key: 'page', values: ['whole', 'tiny'] }, { key: 'casing', values: ['lower', 'mixed'] }, { key: 'span', values: ['whole', 'part'] }],
  base: (row, assignment) => base({ at }, row, assignment),
  normalize: testCase => { if (testCase.assignment.page === 'tiny') testCase.responseChars = 120; },
  truth: (ctx, testCase) => (testCase.query.readProgress ? null : truth(ctx, testCase)),
  observed,
  check,
  refusal,
  indistinct: {
    'classification.longContext': 'changes the provider request, judged by check; evidence is kept whole',
    'readProgress.unit': 'private progress: the schema a caller sees omits it and the tool refuses it',
    view: 'one value per form, and each form has its own oracle',
  },
});

export const localFetch = spec('localFetch', file => ({ path: `matrix-world/plain/${file}` }));
export const ghGetFileContent = spec('ghGetFileContent', file => ({ owner: 'octo', repo: 'matrix', path: file }));
