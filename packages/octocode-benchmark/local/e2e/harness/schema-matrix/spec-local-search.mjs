// localSearch: every query field judged against the grep oracle on known files.
import fs from 'node:fs';
import path from 'node:path';
import { classificationBase, longContextProblem } from './classification-rows.mjs';
import { searchTruth } from './oracle-grep.mjs';

const PATTERNS = {
  literal: { lower: 'a.pha', mixed: 'Alpha' },
  rust: { lower: 'a.pha', mixed: 'A.pha' },
  pcre2: { lower: 'a.pha(?!bet)', mixed: 'A.pha(?!bet)' },
};
const SPANNING = { on: 'gamma\\sdelta', dotall: 'gamma.delta' };

function pattern({ regexEngine = 'literal', multiline = 'off', casing = 'lower' }) {
  if (multiline !== 'off' && regexEngine !== 'literal') return SPANNING[multiline];
  return PATTERNS[regexEngine][casing];
}

const rel = (ctx, root, value) => path.relative(fs.realpathSync(ctx.world[root]), fs.realpathSync(path.resolve(ctx.cwd, value))).split(path.sep).join('/');
const statOf = (ctx, root, file) => fs.statSync(path.join(ctx.world[root], file));

function observed(ctx, testCase, pages) {
  const { query, assignment } = testCase;
  const view = query.view ?? 'content';
  const files = pages.flatMap(page => page.json?.results ?? []).flatMap(row => row.data?.files ?? []);
  const root = assignment.root;
  const at = file => rel(ctx, root, file.path);
  if (view === 'content') return files.flatMap(file => (file.matches ?? []).map(match => [at(file), match.line, match.value]));
  if (view === 'matchOnly') return files.flatMap(file => (file.matches ?? []).map(match => (query.unique === 'count' ? [at(file), match.value, match.count] : [at(file), match.value])));
  if (view === 'files' || view === 'filesWithout') return files.map(file => [at(file)]);
  return files.map(file => [at(file), file[view === 'countLines' ? 'matchedLineCount' : 'matchCount']]);
}

/** Sort order: the file order of the walk follows the key, read from the tree independently. */
function orderProblem(ctx, testCase, pages) {
  const sort = testCase.query.sort;
  if (!sort || !['path', 'modifiedAt', 'accessedAt', 'createdAt'].includes(sort.by)) return null;
  const root = testCase.assignment.root;
  const files = pages.flatMap(page => page.json?.results ?? []).flatMap(row => row.data?.files ?? []).map(file => rel(ctx, root, file.path));
  const distinct = files.filter((file, index) => files.indexOf(file) === index);
  const value = file => {
    if (sort.by === 'path') return file;
    const stat = statOf(ctx, root, file);
    return { modifiedAt: stat.mtimeMs, accessedAt: stat.atimeMs, createdAt: stat.birthtimeMs }[sort.by];
  };
  const ascending = sort.order === 'asc';
  for (let index = 1; index < distinct.length; index += 1) {
    const [a, b] = [value(distinct[index - 1]), value(distinct[index])];
    if (ascending ? a > b : a < b) return `${sort.by} ${sort.order}: ${distinct[index - 1]} before ${distinct[index]}`;
  }
  return null;
}

export const localSearch = {
  tool: 'localSearch',
  pseudo: [
    { key: 'root', values: ['plain', 'git'] },
    { key: 'page', values: ['whole', 'tiny'] },
    { key: 'casing', values: ['lower', 'mixed'] },
  ],
  base: (row, assignment) => ({
    path: `matrix-world/${assignment.root ?? 'plain'}`,
    matchString: [pattern({ ...row, casing: assignment.casing })],
    ...(assignment.page === 'tiny' && { limit: 1 }),
    ...classificationBase(row, assignment),
  }),
  normalize: testCase => {
    testCase.assignment.root ??= 'plain';
    if (testCase.assignment.page === 'tiny') testCase.responseChars = 300;
  },
  truth: (ctx, testCase) => searchTruth(testCase.assignment.root, testCase.query),
  observed,
  check: (ctx, testCase, pages) => longContextProblem(ctx, testCase.query, searchTruth(testCase.assignment.root, testCase.query).length > 0) ?? orderProblem(ctx, testCase, pages),
  indistinct: {
    sort: 'order, judged against file metadata; the selected set must not change',
    'classification.longContext': 'changes the provider request, judged by check; evidence is kept whole',
  },
};
