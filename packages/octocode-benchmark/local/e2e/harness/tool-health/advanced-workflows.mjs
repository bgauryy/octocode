import path from 'node:path';
import { sourceLines, structureFiles } from '../responses.mjs';
import { leadsIn } from './calls.mjs';
import { equal, fixturePath, orderedEqual, relative, sourcePairs, sourceText } from './advanced-local/helpers.mjs';

const dataOf = (report, tool) => report.responses.filter(response => response.tool === tool)
  .flatMap(response => response.result.json?.results ?? []).map(row => row.data);
const related = (ctx, report, tool) => report.responses.flatMap(response => leadsIn(response.result.json, ctx.schemaOf(response.tool)))
  .filter(lead => lead.kind === 'related' && lead.tool === tool);

async function localResearch(ctx) {
  const checks = [];
  const source = sourcePairs(sourceText(ctx, 'code/model.ts'));
  const declaration = source.find(([, text]) => text.startsWith('export function add('))[0];
  const expectedBody = source.filter(([line]) => line >= declaration && line <= declaration + 2);
  const outcomes = {};
  for (const surface of ctx.surfaces) {
    const walk = async (tool, input, step) => {
      const report = await ctx.explore(tool, input, surface);
      checks.push([`workflow.local.${step}.complete.${surface}`, !report.failures.length, JSON.stringify(report.failures)]);
      return report;
    };
    const structure = await walk('structureSearch', { queries: [{ path: fixturePath('code'), operation: 'files', include: ['model.ts'], limit: 1 }], responseChars: 300 }, 'discover');
    const discovered = dataOf(structure, 'structureSearch').flatMap(data => structureFiles(data.files));
    equal(checks, `workflow.local.discovered.${surface}`, discovered.map(file => path.basename(file.path)), ['model.ts']);
    if (discovered.length !== 1) continue;
    const search = await walk('localSearch', { queries: [{ path: discovered[0].path, matchString: ['add'], wholeWord: true, view: 'content', limit: 1 }], responseChars: 300 }, 'search');
    const read = related(ctx, search, 'localFetch').find(lead => lead.input.queries?.some(query => query.ranges?.some(range => range.startLine === declaration)));
    const callers = related(ctx, search, 'lspSearch').find(lead => lead.input.queries?.some(query => query.operation === 'callers' && query.line === declaration && query.symbolName === 'add'));
    checks.push([`workflow.local.emitted-read-and-callers.${surface}`, !!read && !!callers, JSON.stringify({ read, callers })]);
    if (!read || !callers) continue;
    // These requests come from tool output and must reach the consumer unchanged.
    const fetched = await walk(read.tool, read.input, 'read');
    const body = dataOf(fetched, 'localFetch').flatMap(sourceLines);
    orderedEqual(checks, `workflow.local.exact-source.${surface}`, body, expectedBody);
    const semantic = await walk(callers.tool, callers.input, 'callers');
    const sites = dataOf(semantic, 'lspSearch').flatMap(data => data.files ?? []).flatMap(file =>
      (file.matches ?? []).flatMap(match => (match.sites ?? []).map(site => [relative(ctx, file.path), site.line])));
    equal(checks, `workflow.local.semantic-sites.${surface}`, sites,
      [['code/model.ts', declaration + 5], ['code/model.ts', declaration + 6], ['code/use.ts', 4]]);
    outcomes[surface] = { body, sites: sites.toSorted((a, b) => JSON.stringify(a).localeCompare(JSON.stringify(b))) };
  }
  checks.push(['workflow.local.transport-parity', outcomes.cli !== undefined && outcomes.mcp !== undefined
    && JSON.stringify(outcomes.cli) === JSON.stringify(outcomes.mcp), JSON.stringify(outcomes)]);
  return checks;
}

export const ADVANCED_WORKFLOWS = { lspSearch: localResearch };
