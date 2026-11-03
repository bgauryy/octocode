import path from 'node:path';
import fs from 'node:fs';
import { sourceLines, structureFiles } from '../responses.mjs';
import { leadsIn } from './calls.mjs';
import { statusClass } from './driver.mjs';
import { equal, orderedEqual, sourcePairs } from './advanced-local/helpers.mjs';

const rows = (report, tool) => report.responses.filter(response => response.tool === tool)
  .flatMap(response => response.result.json?.results ?? []).map(row => row.data);
const calls = (ctx, report, tool) => report.responses.flatMap(response => leadsIn(response.result.json, ctx.schemaOf(response.tool)))
  .filter(call => call.tool === tool);

function walker(ctx, surface, checks, workflow) {
  return async (tool, input, step) => {
    const report = await ctx.explore(tool, input, surface);
    const errors = report.responses.filter(response => !['ok', 'empty', 'outOfRange'].includes(statusClass(response.result.json)));
    checks.push([`workflow.github.${workflow}.${step}.accepted.${surface}`, report.failures.length === 0 && errors.length === 0,
      JSON.stringify({ failures: report.failures, errors })]);
    return report;
  };
}

async function repositoryResearch(ctx) {
  const checks = [];
  const evidence = {};
  for (const surface of ctx.surfaces) {
    const walk = walker(ctx, surface, checks, 'repository');
    const found = await walk('ghSearchRepo', { queries: [{ operation: 'search', owner: 'octo', keywords: ['demo'] }], responseChars: 300 }, 'discover');
    const repository = rows(found, 'ghSearchRepo').flatMap(data => data.repositories ?? []).find(repo => repo.repo === 'demo');
    checks.push([`workflow.github.repository.identity.${surface}`, repository?.owner === 'octo', JSON.stringify(repository)]);
    if (!repository) continue;
    const scope = { owner: repository.owner, repo: repository.repo };
    const tree = await walk('ghStructure', { queries: [{ ...scope, operation: 'tree', include: ['src/greeter.ts'] }], responseChars: 300 }, 'structure');
    const files = rows(tree, 'ghStructure').flatMap(data => structureFiles(data.files));
    equal(checks, `workflow.github.repository.paths.${surface}`, files.map(file => file.path), ['src/greeter.ts']);
    const ref = rows(tree, 'ghStructure').find(data => data.commitSha)?.commitSha;
    checks.push([`workflow.github.repository.pinned.${surface}`, ref === ctx.mock.model.head.oid, String(ref)]);
    if (!ref || files.length !== 1) continue;
    const selected = files[0].path;
    const search = await walk('ghSearchCode', { queries: [{ ...scope, ref, keywords: ['greeter-1'], pathPrefix: path.posix.dirname(selected) }], responseChars: 300 }, 'search');
    equal(checks, `workflow.github.repository.hits.${surface}`, rows(search, 'ghSearchCode').flatMap(data => data.files ?? []).map(file => file.path), [selected]);
    const read = calls(ctx, search, 'ghGetFileContent').find(call => call.input.queries?.some(query => query.path === selected));
    checks.push([`workflow.github.repository.emittedRead.${surface}`, !!read, JSON.stringify(read)]);
    if (!read) continue;
    const fetched = await walk(read.tool, read.input, 'emitted-read');
    const source = sourcePairs(ctx.mock.model.files[selected]);
    const lines = rows(fetched, read.tool).flatMap(sourceLines);
    checks.push([`workflow.github.repository.sourceWitness.${surface}`, lines.length > 0 && lines.every(([line, text]) => source.some(pair => pair[0] === line && pair[1] === text))
      && lines.some(([, text]) => text.includes('greeter-1')), JSON.stringify(lines)]);
    const full = await walk('ghGetFileContent', { queries: [{ ...scope, ref, path: selected }], responseChars: 300 }, 'whole-source');
    const whole = rows(full, 'ghGetFileContent').flatMap(sourceLines);
    orderedEqual(checks, `workflow.github.repository.exactSource.${surface}`, whole, source);
    evidence[surface] = { ref, selected, whole };
  }
  orderedEqual(checks, 'workflow.github.repository.parity', evidence.cli, evidence.mcp);
  return checks;
}

async function historyResearch(ctx) {
  const checks = [];
  const evidence = {};
  const cases = [
    { operation: 'issue', keywords: ['prefix'], collection: 'issues', expected: ctx.mock.model.issues.find(item => item.number === 2) },
    { operation: 'pullRequest', keywords: ['Bulk greeter'], collection: 'pullRequests', expected: ctx.mock.model.pulls.find(item => item.number === 4) },
    { operation: 'commit', ref: ctx.mock.model.head.oid, collection: 'commits', expected: ctx.mock.model.head },
  ];
  for (const surface of ctx.surfaces) {
    const walk = walker(ctx, surface, checks, 'history');
    evidence[surface] = [];
    for (const { collection, expected, ...query } of cases) {
      const found = await walk('ghSearchHistory', { queries: [{ owner: 'octo', repo: 'demo', ...query }], responseChars: 400 }, `${query.operation}.search`);
      const identity = item => query.operation === 'commit' ? item.commitSha : item.number;
      const expectedId = expected.oid ?? expected.number;
      checks.push([`workflow.github.history.${query.operation}.discovered.${surface}`, rows(found, 'ghSearchHistory')
        .flatMap(data => data[collection] ?? []).some(item => identity(item) === expectedId), String(expectedId)]);
      const read = calls(ctx, found, 'ghGetHistoryItem').find(call => call.input.queries?.some(input => input.operation === query.operation
        && (query.operation === 'commit' ? input.ref === expectedId : input.number === expectedId)));
      checks.push([`workflow.github.history.${query.operation}.emittedRead.${surface}`, !!read, JSON.stringify(read)]);
      if (!read) continue;
      const detail = await walk(read.tool, read.input, `${query.operation}.detail`);
      const items = query.operation === 'commit' ? rows(detail, read.tool) : rows(detail, read.tool).flatMap(data => data[collection] ?? []);
      checks.push([`workflow.github.history.${query.operation}.identity.${surface}`, items.some(item => identity(item) === expectedId), JSON.stringify(items)]);
      if (query.operation !== 'commit') checks.push([`workflow.github.history.${query.operation}.body.${surface}`, items.some(item => item.body === expected.body), expected.body]);
      evidence[surface].push({ operation: query.operation, id: expectedId, body: items.find(item => item.body)?.body });
    }
  }
  orderedEqual(checks, 'workflow.github.history.parity', evidence.cli, evidence.mcp);
  return checks;
}

async function cloneResearch(ctx) {
  const checks = [];
  const evidence = {};
  const selected = 'src/greeter.ts';
  for (const surface of ctx.surfaces) {
    const walk = walker(ctx, surface, checks, 'clone');
    const cloned = await walk('ghCloneRepo', { queries: [{ owner: 'octo', repo: 'demo', ref: ctx.mock.model.head.oid, paths: [selected], historyDepth: 1 }] }, 'checkout');
    const location = rows(cloned, 'ghCloneRepo').find(data => data.location)?.location;
    checks.push([`workflow.github.clone.pinned.${surface}`, location?.commitSha === ctx.mock.model.head.oid && typeof location.localPath === 'string', JSON.stringify(location)]);
    if (!location?.localPath) continue;
    const target = path.join(location.localPath, selected);
    const searched = await walk('localSearch', { queries: [{ path: target, matchString: ['healthMarker'], view: 'files' }], responseChars: 300 }, 'local-search');
    equal(checks, `workflow.github.clone.localHit.${surface}`, rows(searched, 'localSearch').flatMap(data => data.files ?? []).map(file => fs.realpathSync(path.resolve(ctx.cwd, file.path))), [fs.realpathSync(target)]);
    const fetched = await walk('localFetch', { queries: [{ path: target }], responseChars: 300 }, 'local-read');
    const source = rows(fetched, 'localFetch').flatMap(sourceLines);
    orderedEqual(checks, `workflow.github.clone.exactSource.${surface}`, source, sourcePairs(ctx.mock.model.files[selected]));
    evidence[surface] = { ref: location.commitSha, source };
  }
  orderedEqual(checks, 'workflow.github.clone.parity', evidence.cli, evidence.mcp);
  return checks;
}

export const ADVANCED_GITHUB_WORKFLOWS = { ghSearchRepo: repositoryResearch, ghSearchHistory: historyResearch, ghCloneRepo: cloneResearch };
