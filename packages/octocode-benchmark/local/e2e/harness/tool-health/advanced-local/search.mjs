import { equal, orderedEqual, files, fixturePath, matches, relative, scenario } from './helpers.mjs';

const scope = fixturePath('search');
const base = { path: scope, matchString: ['alpha', 'beta'], caseMode: 'sensitive', wholeWord: true, exclude: ['*.log'], sort: { by: 'path', order: 'asc' } };
const hitLines = [['search/a.txt', 1, 'alpha alpha'], ['search/a.txt', 3, 'beta'], ['search/b.txt', 2, 'alpha'], ['search/b.txt', 3, 'beta beta']];
const names = ['search/a.txt', 'search/b.txt'];

async function views(ctx, surface, checks) {
  for (const view of ['content', 'files', 'filesWithout', 'countLines', 'countMatches']) {
    const row = await scenario(ctx, 'localSearch', surface, { label: `search.${view}`, query: { ...base, view }, checks, responseChars: 220 });
    const found = files(row.data);
    if (view === 'content') equal(checks, `search.content.${surface}`, matches(row.data).map(m => [relative(ctx, m.path), m.line, m.value]), hitLines);
    else if (view === 'files' || view === 'filesWithout') equal(checks, `search.${view}.${surface}`, found.map(f => relative(ctx, f.path)), view === 'files' ? names : ['search/c.txt']);
    else equal(checks, `search.${view}.${surface}`, found.map(f => [relative(ctx, f.path), f[view === 'countLines' ? 'matchedLineCount' : 'matchCount']]), names.map(name => [name, view === 'countLines' ? 2 : 3]));
  }
  for (const unique of ['off', 'list', 'count']) {
    const row = await scenario(ctx, 'localSearch', surface, { label: `search.matchOnly.${unique}`, query: { ...base, view: 'matchOnly', unique }, checks, responseChars: 220 });
    const expected = unique === 'off'
      ? [['search/a.txt', 'alpha'], ['search/a.txt', 'alpha'], ['search/a.txt', 'beta'], ['search/b.txt', 'alpha'], ['search/b.txt', 'beta'], ['search/b.txt', 'beta']]
      : [['search/a.txt', 'alpha'], ['search/a.txt', 'beta'], ['search/b.txt', 'alpha'], ['search/b.txt', 'beta']];
    equal(checks, `search.matchOnly.${unique}.${surface}`, matches(row.data).map(m => [relative(ctx, m.path), m.value]), expected);
    if (unique === 'count') equal(checks, `search.frequencies.${surface}`, matches(row.data).map(m => [relative(ctx, m.path), m.value, m.count]), [['search/a.txt', 'alpha', 2], ['search/a.txt', 'beta', 1], ['search/b.txt', 'alpha', 1], ['search/b.txt', 'beta', 2]]);
  }
}

async function effects(ctx, surface, checks) {
  const cases = [
    { label: 'pcre2-lookaround', query: { ...base, matchString: ['(?<=alpha )alpha'], regexEngine: 'pcre2' }, expected: [['search/a.txt', 1]] },
    { label: 'rust-word-boundary', query: { ...base, matchString: ['a\\w+'], regexEngine: 'rust' }, expected: [['search/a.txt', 1], ['search/a.txt', 2], ['search/b.txt', 2]] },
    { label: 'insensitive-whole-word', query: { ...base, matchString: ['alpha'], caseMode: 'insensitive' }, expected: [['search/a.txt', 1], ['search/a.txt', 2], ['search/b.txt', 2]] },
    { label: 'smart-uppercase', query: { path: scope, matchString: ['ALPHA'], wholeWord: true }, expected: [['search/a.txt', 2]] },
    { label: 'file-type', query: { path: fixturePath('code'), matchString: ['add('], fileType: 'ts', caseMode: 'sensitive' }, expected: [['code/model.ts', 14], ['code/model.ts', 19], ['code/model.ts', 20], ['code/use.ts', 4]] },
    { label: 'root-depth', query: { path: fixturePath('structure'), matchString: ['a', 'deep'], wholeWord: true, maxDepth: 1 }, expected: [['structure/a.txt', 1]] },
    { label: 'nested-depth', query: { path: fixturePath('structure'), matchString: ['a', 'deep'], wholeWord: true, maxDepth: 2 }, expected: [['structure/a.txt', 1], ['structure/sub/deep.txt', 1]] },
    { label: 'file-has', query: { ...base, fileHas: ['SELECT'] }, expected: [['search/b.txt', 2], ['search/b.txt', 3]] },
    { label: 'file-lacks', query: { ...base, matchString: ['SELECT'], fileLacks: ['OMIT'] }, expected: [['search/b.txt', 1]] },
    { label: 'hidden-and-exclude', query: { ...base, includeHidden: true, exclude: ['a.txt', 'b.txt', '*.log'] }, expected: [['search/.hidden.txt', 1]] },
    { label: 'inverted-line-selection', query: { path: `${scope}/b.txt`, matchString: ['alpha'], invertMatch: true }, expected: [['search/b.txt', 1], ['search/b.txt', 3]] },
  ];
  for (const { label, query, expected } of cases) {
    const row = await scenario(ctx, 'localSearch', surface, { label: `search.${label}`, query: query, checks });
    equal(checks, `search.${label}.${surface}`, matches(row.data).map(m => [relative(ctx, m.path), m.line]), expected);
  }
  const multiline = await scenario(ctx, 'localSearch', surface, { label: 'search.multiline', query: { path: `${scope}/a.txt`, matchString: ['bridge-start.*bridge-end'], regexEngine: 'rust', multiline: 'dotall' }, checks });
  equal(checks, `search.multiline.${surface}`, matches(multiline.data).map(m => [m.line, m.value]), [[4, 'bridge-start\nmiddle\nbridge-end']]);
  const context = await scenario(ctx, 'localSearch', surface, { label: 'search.context', query: { path: `${scope}/a.txt`, matchString: ['beta'], contextLines: { before: 1, after: 2 } }, checks });
  checks.push([`search.asymmetric-context.${surface}`, JSON.stringify(context.data).includes('bridge-start') && JSON.stringify(context.data).includes('middle') && !JSON.stringify(context.data).includes('bridge-end'), JSON.stringify(context.data)]);
  const sorted = await scenario(ctx, 'localSearch', surface, { label: 'search.order', query: { ...base, view: 'files', sort: { by: 'path', order: 'desc' } }, checks });
  orderedEqual(checks, `search.descending.${surface}`, files(sorted.data).map(f => relative(ctx, f.path)), names.toReversed());
}

async function multilineEvidence(ctx, surface, checks) {
  for (const regexEngine of ['rust', 'pcre2']) {
    for (const view of ['content', 'files', 'countLines', 'countMatches', 'matchOnly']) {
      const label = `search.multiline-evidence.${regexEngine}.${view}`;
      const row = await scenario(ctx, 'localSearch', surface, {
        label, query: { path: `${scope}/a.txt`, matchString: ['bridge-start\\nmiddle\\nbridge-end|tail'], regexEngine, multiline: 'on', view }, checks,
      });
      orderedEqual(checks, `${label}.lines.${surface}`, row.root.results[0]?.data?.matchedLineCount, 4);
      if (view === 'matchOnly') orderedEqual(checks, `${label}.coordinates.${surface}`, matches(row.data).map(m => [m.line, m.column, m.value]), [[4, 1, 'bridge-start\nmiddle\nbridge-end'], [7, 1, 'tail']]);
      if (view === 'countLines') equal(checks, `${label}.file-lines.${surface}`, files(row.data).map(f => f.matchedLineCount), [4]);
    }
    const row = await scenario(ctx, 'localSearch', surface, {
      label: `search.matched-newline.${regexEngine}`, query: { path: `${scope}/a.txt`, matchString: ['bridge-end\\n'], regexEngine, multiline: 'on', view: 'matchOnly' }, checks,
    });
    equal(checks, `search.matched-newline.${regexEngine}.${surface}`, matches(row.data).map(m => m.value), ['bridge-end\n']);
  }
}

export async function advancedSearch(ctx) {
  const checks = [];
  for (const surface of ctx.surfaces) {
    await views(ctx, surface, checks);
    await effects(ctx, surface, checks);
    await multilineEvidence(ctx, surface, checks);
  }
  return checks;
}
