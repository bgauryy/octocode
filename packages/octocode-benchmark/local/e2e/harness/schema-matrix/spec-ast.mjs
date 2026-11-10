// astSearch: known call sites, declarations and positions in the fixture. Which
// scene a row runs on follows from its fields; fields that cannot share one scene
// are not a row, so a value is judged where its effect can be seen.
import fs from 'node:fs';
import path from 'node:path';
import { LANG, SYMBOLS, textOf, testModeKeeps, visibleFiles } from './world-local.mjs';
import { captureText } from '../responses.mjs';

const firstValue = (value, fallback) => value ?? fallback;
const byText = (a, b) => (a < b ? -1 : a > b ? 1 : 0);
const sceneOf = row => (row.scope ? 'scope' : row.rule?.nthChild ? 'nth' : row.rule?.stopBy ? 'stopBy' : null);
const rootOf = assignment => assignment.root ?? 'plain';

const SCENES = {
  scope: { path: 'matrix-world/shapes/scope.ts', scope: { start: { line: 1, column: 1 }, end: { line: 2, column: 4 } }, rule: { kind: 'call_expression' } },
  nth: { path: 'matrix-world/shapes/nth.ts', rule: { kind: 'identifier', nthChild: { position: 1 } } },
  stopBy: { path: 'matrix-world/shapes/nest.ts', rule: { kind: 'return_statement', inside: { kind: 'function_declaration' } } },
};
const isLanguageRow = row => !!row.parser && row.parser !== 'typescript';

function matchBase(row, assignment) {
  const scene = sceneOf(row);
  const language = isLanguageRow(row);
  if (scene && (language || row.view || row.sort || row.testCode)) return null;
  if ([row.scope, row.rule?.nthChild, row.rule?.stopBy].filter(Boolean).length > 1) return null;
  if (scene) return SCENES[scene];
  if (language) return { path: `matrix-world/lang/${LANG[row.parser][0]}`, rule: { pattern: row.parser === 'asm' ? 'greet:' : 'greet($A)' } };
  return { path: `matrix-world/${rootOf(assignment)}`, parser: 'typescript', rule: { pattern: 'probe($A)' }, ...(assignment.page === 'tiny' && { limit: 1 }) };
}

function symbolsBase(row, assignment) {
  const language = row.parser && row.parser !== 'typescript';
  if (language) return { path: `matrix-world/lang/${LANG[row.parser][0]}` };
  const filter = row.symbolFilter && { symbolFilter: { terms: [assignment.terms === 'cased' ? 'alphaCall' : 'alphacall'] } };
  return { path: row.parser ? `matrix-world/${rootOf(assignment)}/src/alpha.ts` : `matrix-world/${rootOf(assignment)}`, ...filter, ...(assignment.page === 'tiny' && { limit: 1 }) };
}

function base(row, assignment) {
  if (row.operation === 'match') return matchBase(row, assignment);
  if (row.operation === 'symbols') return symbolsBase(row, assignment);
  const parser = row.parser ?? 'typescript';
  return { path: `matrix-world/lang/${LANG[parser][0]}` };
}

const callSites = (query, root) => visibleFiles(root, query).filter(file => file.endsWith('.ts') && testModeKeeps(query.testCode ?? 'include', file))
  .flatMap(file => textOf(file).split('\n').flatMap((text, index) => [...text.matchAll(/probe\(\d+\)/g)].map(match => [file, index + 1, match[0]])));

function languageSites(query) {
  if (query.testCode === 'only') return [];
  return LANG[query.parser][1].flatMap((text, index) => (text.includes('greet') ? [['', index + 1, query.parser === 'asm' ? 'greet:' : 'greet(1)']] : []));
}

function matchTruth(testCase) {
  const { query, assignment } = testCase;
  if (query.scope) return query.scope.mode === 'overlap' ? [['', 1, 'probe(\n  1\n)']] : [];
  if (query.rule.nthChild) return query.rule.nthChild.reverse ? [['', 1, 'c3']] : [['', 1, 'list'], ['', 1, 'a1']];
  if (query.rule.inside) return query.rule.inside.stopBy === 'end' ? [['', 3, 'return 1;']] : [];
  const rows = isLanguageRow(query) ? languageSites(query) : callSites(query, rootOf(assignment));
  const view = query.view ?? 'content';
  if (view === 'content') return rows;
  const counts = new Map();
  for (const [file] of rows) counts.set(file, (counts.get(file) ?? 0) + 1);
  return [...counts].map(([file, count]) => (view === 'files' ? [file] : [file, count]));
}

function symbolTruth(testCase) {
  const { query, assignment } = testCase;
  if (query.parser && query.parser !== 'typescript') return [];
  const mode = query.testCode ?? 'include';
  const files = (query.parser ? ['src/alpha.ts'] : visibleFiles(rootOf(assignment), query).filter(file => Object.hasOwn(SYMBOLS, file))).filter(file => testModeKeeps(mode, file));
  const testItems = ['tests', 'alpha_case'];
  const keepsItem = (file, name) => file !== 'src/lib.rs' || mode === 'include' || testItems.includes(name) === (mode === 'only');
  const filter = query.symbolFilter;
  const matches = name => {
    if (!filter) return true;
    const term = filter.terms[0];
    const sensitive = filter.caseMode === 'sensitive' || (firstValue(filter.caseMode, 'smart') === 'smart' && /[A-Z]/.test(term));
    const [a, b] = sensitive ? [name, term] : [name.toLowerCase(), term.toLowerCase()];
    return (filter.mode ?? 'exactFirst') === 'substring' ? a.includes(b) : a === b;
  };
  return files.flatMap(file => SYMBOLS[file].filter(name => matches(name) && keepsItem(file, name)).map(name => [file, name]));
}

function truth(ctx, testCase) {
  const { operation } = testCase.query;
  if (operation === 'match') return matchTruth(testCase);
  if (operation === 'symbols') return symbolTruth(testCase);
  return [['hasGreet', true], ['anonymous', testCase.query.namedOnly === false]];
}

const rowsOf = pages => pages.flatMap(page => page.json?.results ?? []).map(row => row.data ?? {});
const rel = (ctx, testCase, value) => {
  const single = !testCase.query.path.match(/matrix-world\/(plain|git)$/);
  if (single) return '';
  return path.relative(fs.realpathSync(ctx.world[rootOf(testCase.assignment)]), path.resolve(fs.realpathSync(ctx.cwd), value)).split(path.sep).join('/');
};

function nodeTexts(data, source) {
  const lines = source.split('\n');
  return (data.nodes ?? []).map(({ node }) => {
    const match = / (\d+):(\d+)-(\d+):(\d+)/.exec(node);
    if (!match) return { node, text: null };
    const [sl, sc, el, ec] = match.slice(1).map(Number);
    return { node, text: sl === el ? lines[sl - 1]?.slice(sc - 1, ec - 1) : null };
  });
}

function observed(ctx, testCase, pages) {
  const { query } = testCase;
  const view = query.view ?? 'content';
  if (query.operation === 'match') {
    const files = rowsOf(pages).flatMap(data => data.files ?? []);
    if (view === 'content') return files.flatMap(file => (file.matches ?? []).map(match => [rel(ctx, testCase, file.path), match.line, match.value]));
    return files.map(file => (view === 'files' ? [rel(ctx, testCase, file.path)] : [rel(ctx, testCase, file.path), file.matchCount]));
  }
  if (query.operation === 'symbols') {
    const single = rowsOf(pages).flatMap(data => (data.symbols ?? []).map(symbol => ({ file: '', symbol })));
    const many = rowsOf(pages).flatMap(data => (data.files ?? []).flatMap(file => (file.symbols ?? []).map(symbol => ({ file: rel(ctx, testCase, file.path), symbol }))));
    const named = ({ file, symbol }) => [{ file, name: symbol.symbolName }, ...(symbol.members ?? []).flatMap(member => named({ file, symbol: member }))];
    if (query.parser && query.parser !== 'typescript') return [];
    return [...single, ...many].flatMap(named).map(({ file, name }) => [file || 'src/alpha.ts', name]);
  }
  const source = fs.readFileSync(path.resolve(ctx.cwd, query.path), 'utf8');
  const nodes = rowsOf(pages).flatMap(data => nodeTexts(data, source));
  return [['hasGreet', nodes.some(entry => entry.text === 'greet')], ['anonymous', nodes.some(entry => /^\d+ "/.test(entry.node))]];
}

const matchesOf = pages => rowsOf(pages).flatMap(data => data.files ?? []).flatMap(file => file.matches ?? []);

function provenanceProblem(query, pages) {
  if (!query.includeProvenance) return null;
  const missing = matchesOf(pages).find(match => !match.sourceRange || !match.sourceDigest);
  return missing ? `includeProvenance: match at line ${missing.line} has no sourceRange or sourceDigest` : null;
}

function captureProblem(query, pages) {
  if (!query.rule.pattern?.includes('$A')) return null;
  for (const match of matchesOf(pages)) {
    const captured = captureText(match, match.metavarRanges?.A?.[0]);
    if (query.includeCaptures === true && !captured) return `includeCaptures: no capture at line ${match.line}`;
    if (query.includeCaptures !== true && match.metavarRanges) return 'captures returned without includeCaptures';
    if (captured && !match.value.includes(captured)) return `capture ${captured} is not inside ${match.value}`;
  }
  return null;
}

function sortProblem(query, pages) {
  if (query.sort?.by !== 'path') return null;
  const listed = rowsOf(pages).flatMap(data => data.files ?? []).map(file => file.path);
  const distinct = listed.filter((file, index) => listed.indexOf(file) === index);
  const expected = distinct.toSorted(byText);
  if (query.sort.order === 'desc') expected.reverse();
  return JSON.stringify(expected) === JSON.stringify(distinct) ? null : `path ${query.sort.order}: ${distinct.join(', ')}`;
}

function check(ctx, testCase, pages) {
  const { query } = testCase;
  if (query.operation !== 'match') return null;
  const content = (query.view ?? 'content') === 'content';
  return (content ? provenanceProblem(query, pages) ?? captureProblem(query, pages) : null) ?? sortProblem(query, pages);
}

export const astSearch = {
  tool: 'astSearch',
  pseudo: [{ key: 'root', values: ['plain', 'git'] }, { key: 'page', values: ['whole', 'tiny'] }, { key: 'terms', values: ['lower', 'cased'] }],
  base,
  // The schema reuses one rule shape everywhere; stopBy only means something on a relation.
  shape: query => {
    if (!query.rule?.stopBy) return query;
    const { stopBy, ...rule } = query.rule;
    return { ...query, rule: { ...rule, inside: { ...rule.inside, stopBy } } };
  },
  normalize: testCase => {
    testCase.assignment.root ??= 'plain';
    if (testCase.assignment.page === 'tiny') testCase.responseChars = 300;
  },
  truth,
  observed,
  check,
  indistinct: {
    sort: 'order, judged by check; the selected set must not change',
    includeCaptures: 'adds fields, judged by check on every match',
    includeProvenance: 'adds fields, judged by check on every match',
    operation: 'one value per form',
    'symbolFilter.caseMode': 'judged in rows with a cased term',
  },
};
