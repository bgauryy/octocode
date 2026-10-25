import { isDeepStrictEqual } from 'node:util';
import { nextHints } from '../responses.mjs';

const sourceScope = { owner: 'octo', repo: 'demo', extensions: ['ts'], pathPrefix: 'src', nameContains: 'greeter' };

function relatedSearch(result, variants) {
  return nextHints(result.json).find(call => call.kind === 'related' && call.tool === 'ghSearchCode'
    && call.input.queries?.length === variants.length
    && call.input.queries.every((query, index) => isDeepStrictEqual(query.keywords, variants[index])));
}

async function executeFreshSearch(ctx, surface, checks, scenario) {
  const query = { ...sourceScope, keywords: scenario.keywords, matchIn: scenario.matchIn };
  const result = await ctx.call(ctx.q(query), surface);
  const related = relatedSearch(result, scenario.variants);
  const expected = scenario.variants.map(keywords => ({ ...sourceScope, keywords, matchIn: ['content'] }));
  const admitted = related && !ctx.validateInput(related.tool, related.input);
  checks.push([`code.related.${scenario.label}.authored.${surface}`, result.json?.results?.[0]?.status === 'empty'
    && admitted && isDeepStrictEqual(related.input.queries, expected), JSON.stringify(result.json)]);
  if (!admitted) return;
  const report = await ctx.explore(related.tool, related.input, surface);
  const evidence = report.responses.filter(entry => entry.tool === 'ghSearchCode').flatMap(entry => entry.result.json?.results ?? []);
  const actual = scenario.variants.map((_, index) => evidence.filter(row => row.index === index).flatMap(row => (row.data.files ?? []).map(file => file.path)));
  checks.push([`code.related.${scenario.label}.executes.${surface}`, !report.failures.length
    && evidence.every(row => !row.status && !row.data.errorCode)
    && isDeepStrictEqual(actual, scenario.variants.map(() => ['src/greeter.ts'])), JSON.stringify({ actual, failures: report.failures, calls: report.calls.length })]);
}

async function searchContinuations(ctx) {
  const checks = [];
  for (const surface of ctx.surfaces) {
    await ctx.scenario('pathToContent', surface, () => executeFreshSearch(ctx, surface, checks, { label: 'pathToContent', keywords: ['greet('], matchIn: ['path'], variants: [['greet(']] }));
    await ctx.scenario('phraseAlternatives', surface, () => executeFreshSearch(ctx, surface, checks, { label: 'phraseAlternatives', keywords: ['greet OR Greeter'], matchIn: ['content'], variants: [['greet'], ['Greeter']] }));
  }
  return checks;
}

export const ADVANCED_SEARCH_CONTINUATION = { ghSearchCode: searchContinuations };
