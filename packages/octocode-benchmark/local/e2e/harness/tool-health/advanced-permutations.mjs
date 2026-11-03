import { contract } from '../contract.mjs';
import { finiteInputDomains } from './input-coverage.mjs';
import { equal, files, fixturePath, matches, relative, scenario, sourcePairs, sourceText } from './advanced-local/helpers.mjs';

const domains = finiteInputDomains(contract.tools.find(tool => tool.name === 'localSearch').querySchema);
const values = name => domains.find(domain => domain.path.join('.') === name)?.values ?? [];

/** Full Cartesian product of these compatible axes; every value comes from core. */
export function searchPermutations() {
  let combinations = [{}];
  for (const axis of ['regexEngine', 'caseMode', 'wholeWord', 'view']) {
    combinations = combinations.flatMap(query => values(axis).map(value => ({ ...query, [axis]: value })));
  }
  return combinations.flatMap(query => query.view === 'matchOnly'
    ? values('unique').map(unique => ({ ...query, unique })) : [query]);
}

/** Source oracle: case/word selection and projection are independent of tool rows. */
export function searchExpectation(sources, query) {
  const expression = new RegExp(query.wholeWord ? '\\balpha\\b' : 'alpha', query.caseMode === 'sensitive' ? 'g' : 'gi');
  return sources.flatMap(([file, text]) => {
    const lines = sourcePairs(text).map(([line, value]) => ({ line, value, hits: [...value.matchAll(expression)].map(hit => hit[0]) }));
    const selected = lines.filter(line => line.hits.length);
    const hits = selected.flatMap(line => line.hits);
    switch (query.view) {
      case 'content': return selected.map(line => [file, line.line, line.value]);
      case 'files': return hits.length ? [[file]] : [];
      case 'filesWithout': return hits.length ? [] : [[file]];
      case 'countLines': return hits.length ? [[file, selected.length]] : [];
      case 'countMatches': return hits.length ? [[file, hits.length]] : [];
      case 'matchOnly': return (query.unique === 'off' ? hits : [...new Set(hits)]).map(value =>
        query.unique === 'count' ? [file, value, hits.filter(hit => hit === value).length] : [file, value]);
      default: throw new Error(`Missing source oracle for localSearch view ${query.view}`);
    }
  });
}

function observed(ctx, data, query) {
  if (query.view === 'content') return matches(data).map(match => [relative(ctx, match.path), match.line, match.value]);
  if (query.view === 'matchOnly') return matches(data).map(match => query.unique === 'count'
    ? [relative(ctx, match.path), match.value, match.count] : [relative(ctx, match.path), match.value]);
  return files(data).map(file => query.view === 'files' || query.view === 'filesWithout' ? [relative(ctx, file.path)]
    : [relative(ctx, file.path), file[query.view === 'countLines' ? 'matchedLineCount' : 'matchCount']]);
}

async function searchMatrix(ctx) {
  const checks = [];
  const sources = ['a.txt', 'b.txt', 'c.txt'].map(name => [`search/${name}`, sourceText(ctx, `search/${name}`)]);
  for (const surface of ctx.surfaces) for (const combination of searchPermutations()) {
    const label = `matrix.search.${Object.values(combination).join('.')}`;
    const query = { path: fixturePath('search'), include: sources.map(([file]) => file.split('/').at(-1)),
      matchString: ['alpha'], sort: { by: 'path', order: 'asc' }, limit: 1, ...combination };
    const result = await scenario(ctx, 'localSearch', surface, { label, query, checks, responseChars: 300 });
    equal(checks, `${label}.evidence.${surface}`, observed(ctx, result.data, query), searchExpectation(sources, query));
  }
  return checks;
}

export const ADVANCED_PERMUTATIONS = { localSearch: searchMatrix };
