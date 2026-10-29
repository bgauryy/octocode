import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import { scenario } from './advanced-local/helpers.mjs';
import { checkGraphLookup, prepareGraphLookup } from './advanced-graph-lookup.mjs';

const data = result => result.json?.results?.[0]?.data;
const input = query => ({ queries: [query] });
const argumentSource = [
  "import { imported_target } from './graph-argument-import';",
  'function argumentTarget() {}',
  'function argumentSink(_value: unknown) {}',
  'function argumentFactory() { return 1; }',
  'export function argumentSource() { argumentSink(argumentTarget); argumentSink(argumentTarget); }',
  'export function shadowedSource(argumentTarget: () => void) { argumentSink(argumentTarget); }',
  'export function fragmentedSource(argumentTarget: () => void) { argumentSink(argumentTarget); argumentSink(argumentTarget); argumentSink(argumentTarget); }',
  'export function computedSource(registry: Record<string, (value: unknown) => void>, key: string) { registry[key](argumentTarget); }',
  'export function unsupportedSource() { argumentSink(...[argumentTarget]); }',
  'export function valueSource() { argumentSink(() => argumentTarget()); argumentSink(argumentFactory()); argumentSink(3); }',
  'class Unrelated { imported_target() {} }',
  'export function importedSource() { argumentSink(imported_target); }',
  '',
].join('\n');
const argumentBytes = Buffer.from(argumentSource);
const argumentDigest = crypto.createHash('sha256').update(argumentBytes).digest('hex');
const rustArgumentSource = [
  'fn argument_target() {}',
  'fn argument_sink<T>(_value: T) {}',
  'pub fn argument_loop() { for argument_target in [1] { argument_sink(argument_target); } }',
  '',
].join('\n');
const compactArgumentSource = `function compactTarget(){} function compactSink(_value:unknown){} export function compactSource(){ compactSink(compactTarget);${' '.repeat(5000)}}`;
const argumentSlice = range => range && Number.isInteger(range.startByte) && Number.isInteger(range.endByte)
  && range.startByte >= 0 && range.endByte >= range.startByte && range.endByte <= argumentBytes.length
  ? argumentBytes.subarray(range.startByte, range.endByte).toString('utf8') : undefined;

async function argumentQuery(ctx, graph, checks, { operation, owner, label, ...extra }) {
  const result = await scenario(ctx, 'graphQuery', 'cli', {
    label: `graph.arguments.${label}`, checks, responseChars: 600,
    query: { operation, graph, target: `graph-arguments.ts#${owner}`, edges: ['passes'], limit: 1, ...extra },
  });
  return result.data;
}

async function checkRepeatedArguments(ctx, graph, checks) {
  const passed = await argumentQuery(ctx, graph, checks, { operation: 'witnesses', owner: 'argumentSource', label: 'repeated' });
  const sites = passed.flatMap(row => row.results ?? []);
  checks.push(['graph.arguments.distinct-slots', sites.length === 2 && new Set(sites.map(row => row.id)).size === 2
    && new Set(sites.map(row => row.recipient?.occurrenceId)).size === 2
    && sites.every(row => row.to === 'graph-arguments.ts#argumentTarget' && row.sourceDigest === argumentDigest
      && argumentSlice(row.sourceRange) === 'argumentTarget' && argumentSlice(row.selectionByteRange) === 'argumentTarget'
      && argumentSlice(row.recipient?.sourceRange) === 'argumentSink(argumentTarget)'
      && row.argument?.groupIndex === 0 && row.argument?.index === 0 && row.argument?.form === 'identifier'), JSON.stringify(sites)]);
  const coverages = passed.map(row => row.argumentCoverage);
  checks.push(['graph.arguments.separate-coverage', coverages.length > 0 && coverages.every(coverage => coverage?.scope === 'snapshot'
    && coverage.population === 'syntax-argument-slots' && Object.values(coverage.byLanguage ?? {}).some(language => language.extraction === 'supported'
      && language.referenceForms.includes('identifier') && language.counts?.slots > 0
      && language.counts.slots === language.counts.directReferences + language.counts.nonReferenceValues + language.counts.unsupportedSlots)), JSON.stringify(coverages)]);
}

async function checkArgumentBoundaries(ctx, graph, checks) {
  const shadowed = (await argumentQuery(ctx, graph, checks, { operation: 'unresolved', owner: 'shadowedSource', label: 'shadowing' })).flatMap(row => row.results ?? []);
  checks.push(['graph.arguments.shadowing-blocks-global', shadowed.length === 1 && shadowed[0].reason === 'binding-unrepresented'
    && argumentSlice(shadowed[0].selectionByteRange) === 'argumentTarget'
    && argumentSlice(shadowed[0].bindingSourceRange)?.includes('argumentTarget') && !shadowed[0].candidates, JSON.stringify(shadowed)]);
  const shadowedWitnesses = (await argumentQuery(ctx, graph, checks, { operation: 'witnesses', owner: 'shadowedSource', label: 'shadowing-no-witness' })).flatMap(row => row.results ?? []);
  checks.push(['graph.arguments.shadowing-no-global-witness', shadowedWitnesses.length === 0, JSON.stringify(shadowedWitnesses)]);
  const computed = (await argumentQuery(ctx, graph, checks, { operation: 'witnesses', owner: 'computedSource', label: 'computed-recipient' })).flatMap(row => row.results ?? []);
  checks.push(['graph.arguments.unnamed-recipient', computed.length === 1 && computed[0].to === 'graph-arguments.ts#argumentTarget'
    && argumentSlice(computed[0].recipient?.sourceRange) === 'registry[key](argumentTarget)', JSON.stringify(computed)]);
  const unsupported = (await argumentQuery(ctx, graph, checks, { operation: 'unresolved', owner: 'unsupportedSource', label: 'unsupported-slot' })).flatMap(row => row.results ?? []);
  checks.push(['graph.arguments.unsupported-retains-slot', unsupported.length === 1 && unsupported[0].reason === 'unsupported-argument-form'
    && unsupported[0].argument?.form === 'unsupported' && argumentSlice(unsupported[0].sourceRange) === '...[argumentTarget]'
    && !unsupported[0].targetName && !unsupported[0].selectionByteRange, JSON.stringify(unsupported)]);
  const values = (await argumentQuery(ctx, graph, checks, { operation: 'witnesses', owner: 'valueSource', label: 'values' })).flatMap(row => row.results ?? []);
  checks.push(['graph.arguments.values-not-references', values.length === 0, JSON.stringify(values)]);
  const unknownValues = (await argumentQuery(ctx, graph, checks, { operation: 'unresolved', owner: 'valueSource', label: 'values-unresolved' })).flatMap(row => row.results ?? []);
  checks.push(['graph.arguments.values-not-unresolved-references', unknownValues.length === 0, JSON.stringify(unknownValues)]);
}

async function checkArgumentFragments(ctx, graph, checks) {
  for (const [operation, owner, expected] of [['witnesses', 'argumentSource', 2], ['unresolved', 'fragmentedSource', 3]]) {
    const result = await scenario(ctx, 'graphQuery', 'cli', {
      label: `graph.arguments.fragments.${operation}`, checks, responseChars: 600,
      query: { operation, graph, target: `graph-arguments.ts#${owner}`, edges: ['passes'], limit: 2 },
    });
    const rows = result.data.flatMap(row => row.results ?? []);
    checks.push([`graph.arguments.fragments.${operation}.exactly-once`, rows.length === expected
      && new Set(rows.map(row => row.id)).size === expected, JSON.stringify(rows)]);
    const fragments = result.responses.flatMap(entry => entry.result.json?.results ?? []);
    checks.push([`graph.arguments.fragments.${operation}.response-split`, fragments.some(row => row.rowPart?.of > 1), JSON.stringify(fragments.map(row => row.rowPart))]);
    const coverages = result.data.map(row => row.argumentCoverage);
    const first = JSON.stringify(coverages[0]);
    checks.push([`graph.arguments.fragments.${operation}.metadata`, coverages.length > 1
      && coverages.every(coverage => JSON.stringify(coverage) === first && Object.keys(coverage?.byLanguage ?? {}).length >= 2
        && Object.values(coverage.byLanguage).every(language => language.extraction !== 'supported' || language.referenceForms?.length > 0)), JSON.stringify(coverages)]);
  }
}

async function checkArguments(ctx, graph, checks) {
  await checkRepeatedArguments(ctx, graph, checks);
  await checkArgumentBoundaries(ctx, graph, checks);
  await checkArgumentFragments(ctx, graph, checks);
  const calls = (await argumentQuery(ctx, graph, checks, { operation: 'callees', owner: 'argumentSource', label: 'not-invocation', edges: ['calls'] })).flatMap(row => row.results ?? []);
  checks.push(['graph.arguments.passing-not-invocation', calls.some(row => row.id === 'graph-arguments.ts#argumentSink')
    && calls.every(row => row.id !== 'graph-arguments.ts#argumentTarget'), JSON.stringify(calls)]);
  const loop = await scenario(ctx, 'graphQuery', 'cli', {
    label: 'graph.arguments.rust-loop-scope', checks, responseChars: 600,
    query: { operation: 'unresolved', graph, target: 'graph-arguments.rs#argument_loop', edges: ['passes'], limit: 1 },
  });
  const loopRows = loop.data.flatMap(row => row.results ?? []);
  checks.push(['graph.arguments.rust-loop-blocks-global', loopRows.length === 1 && loopRows[0].reason === 'binding-unrepresented'
    && loopRows[0].targetName === 'argument_target' && !loopRows[0].candidates
    && Buffer.from(rustArgumentSource).subarray(loopRows[0].bindingSourceRange?.startByte, loopRows[0].bindingSourceRange?.endByte).toString('utf8') === 'argument_target', JSON.stringify(loopRows)]);
  const loopWitnesses = await scenario(ctx, 'graphQuery', 'cli', {
    label: 'graph.arguments.rust-loop-no-witness', checks, responseChars: 600,
    query: { operation: 'witnesses', graph, target: 'graph-arguments.rs#argument_loop', edges: ['passes'], limit: 1 },
  });
  const loopWitnessRows = loopWitnesses.data.flatMap(row => row.results ?? []);
  checks.push(['graph.arguments.rust-loop-no-global-witness', loopWitnessRows.length === 0, JSON.stringify(loopWitnessRows)]);
  const imported = (await argumentQuery(ctx, graph, checks, { operation: 'witnesses', owner: 'importedSource', label: 'import-binding' })).flatMap(row => row.results ?? []);
  checks.push(['graph.arguments.import-not-unrelated-member', imported.length === 1 && imported[0].to === 'graph-argument-import.ts#imported_target', JSON.stringify(imported)]);
}

async function checkWhitespaceEvidence(ctx, graph, checks) {
  for (const file of ['graph-compact.ts', 'graph-formatted.ts']) {
    const result = await scenario(ctx, 'graphQuery', 'cli', {
      label: `graph.arguments.whitespace.${file}`, checks, responseChars: 600,
      query: { operation: 'witnesses', graph, target: `${file}#compactSource`, edges: ['calls', 'passes'], limit: 1 },
    });
    const rows = result.data.flatMap(row => row.results ?? []);
    checks.push([`graph.arguments.whitespace-preserves-evidence.${file}`, rows.length === 2
      && rows.some(row => row.edge === 'calls' && row.to === `${file}#compactSink`)
      && rows.some(row => row.edge === 'passes' && row.to === `${file}#compactTarget`), JSON.stringify(rows)]);
  }
}

export const ADVANCED_GRAPH = {
  graphBuild: async ctx => {
    const checks = [];
    for (const surface of ctx.surfaces) {
      const scope = `build-${surface}`;
      const cwd = path.join(ctx.cwd, scope);
      fs.cpSync(ctx.fixture, cwd, { recursive: true });
      const call = ctx.callOn(surface);
      const initial = data(await call('graphBuild', input({ path: scope, keep: 10 })));
      const reused = data(await call('graphBuild', input({ path: scope, keep: 10 })));
      checks.push([`graph.fresh.${surface}`, !initial.reused && initial.extraction?.parsedFiles > 0, JSON.stringify(initial)]);
      checks.push([`graph.reuse.identity.${surface}`, initial.id === reused.id && reused.reused === true, JSON.stringify(reused)]);
      checks.push([`graph.storage.workspace.${surface}`, fs.realpathSync(path.resolve(ctx.cwd, initial.dir)).startsWith(path.join(fs.realpathSync(ctx.cwd), '.octocode/graph') + path.sep), initial.dir]);
      fs.writeFileSync(path.join(cwd, 'graph-health.ts'), 'export const graphHealthAdded = 1;\n');
      const stale = data(await call('graphQuery', input({ path: scope, operation: 'stats', graph: initial.id })));
      checks.push([`graph.stale.after-add.${surface}`, stale.freshness?.fresh === false, JSON.stringify(stale.freshness)]);
      const rebuilt = data(await call('graphBuild', input({ path: scope, keep: 10 })));
      checks.push([`graph.rebuild.changed.${surface}`, rebuilt.id !== initial.id && !rebuilt.reused, rebuilt.id]);
      checks.push([`graph.rebuild.reuses-unchanged-extraction.${surface}`, rebuilt.extraction?.parsedFiles >= 1 && rebuilt.extraction?.reusedFiles > 0, JSON.stringify(rebuilt.extraction)]);
      const drift = data(await call('graphQuery', input({ path: scope, operation: 'drift', graph: rebuilt.id, baseline: initial.id })));
      checks.push([`graph.drift.added.${surface}`, JSON.stringify(drift).includes('graph-health.ts'), JSON.stringify(drift)]);
    }
    return checks;
  },
  graphQuery: async ctx => {
    fs.writeFileSync(path.join(ctx.cwd, 'graph-evidence.ts'), 'function evidenceTarget() {}\nexport function evidenceRun() { evidenceTarget(); evidenceTarget();\n evidenceTarget(); evidenceMissing(); }\n');
    fs.writeFileSync(path.join(ctx.cwd, 'graph-arguments.ts'), argumentSource);
    fs.writeFileSync(path.join(ctx.cwd, 'graph-arguments.rs'), rustArgumentSource);
    fs.writeFileSync(path.join(ctx.cwd, 'graph-argument-import.ts'), 'export function imported_target() {}\n');
    fs.writeFileSync(path.join(ctx.cwd, 'graph-compact.ts'), compactArgumentSource);
    fs.writeFileSync(path.join(ctx.cwd, 'graph-formatted.ts'), compactArgumentSource.replaceAll(' ', '\n'));
    prepareGraphLookup(ctx.cwd);
    const snapshot = data(await ctx.cliCall('graphBuild', input({ keep: 10 })));
    const files = data(await ctx.cliCall('graphQuery', input({ operation: 'find', target: '.ts', kind: 'file' })));
    const symbols = data(await ctx.cliCall('graphQuery', input({ operation: 'find', target: 'greet', kind: 'symbol' })));
    const file = files.results[0].id;
    const symbol = symbols.results[0].id;
    const checks = [];
    for (const operation of ['stats', 'find', 'candidates', 'node', 'symbols', 'deps', 'dependents', 'callers', 'callees', 'path', 'walk', 'cycles', 'hubs', 'diagnostics', 'stale', 'impact', 'drift', 'witnesses', 'unresolved']) {
      const query = { operation, graph: snapshot.id };
      if (['candidates', 'node', 'callers', 'callees'].includes(operation)) query.target = symbol;
      if (['symbols', 'deps', 'dependents', 'walk', 'impact', 'path'].includes(operation)) query.target = file;
      if (operation === 'find') query.target = 'greet';
      if (operation === 'path') query.to = file;
      if (operation === 'drift') query.baseline = snapshot.id;
      const result = await ctx.cliCall('graphQuery', input(query));
      checks.push([`graph.operation.${operation}`, !result.isError && data(result)?.graph?.id === snapshot.id, result.text]);
    }
    const witnesses = await scenario(ctx, 'graphQuery', 'cli', {
      label: 'graph.witnesses.paged', checks, responseChars: 600,
      query: { operation: 'witnesses', graph: snapshot.id, target: 'graph-evidence.ts#evidenceRun', to: 'graph-evidence.ts#evidenceTarget', edges: ['calls'], limit: 1 },
    });
    const sites = witnesses.data.flatMap(row => row.results ?? []);
    checks.push(['graph.witnesses.preserve-distinct-sites', sites.length === 3 && new Set(sites.map(row => row.id)).size === 3 && sites.every(row => row.sourceDigest?.length === 64 && row.evidence === 'syntax'), JSON.stringify(sites)]);
    const unresolved = await scenario(ctx, 'graphQuery', 'cli', {
      label: 'graph.unresolved.paged', checks, responseChars: 600,
      query: { operation: 'unresolved', graph: snapshot.id, target: 'graph-evidence.ts#evidenceRun', limit: 1 },
    });
    const unknown = unresolved.data.flatMap(row => row.results ?? []);
    checks.push(['graph.unresolved.retains-source', unknown.some(row => row.targetName === 'evidenceMissing' && row.edge === 'calls' && row.sourceDigest?.length === 64), JSON.stringify(unknown)]);
    await checkArguments(ctx, snapshot.id, checks);
    await checkWhitespaceEvidence(ctx, snapshot.id, checks);
    await checkGraphLookup(ctx, snapshot.id, checks);
    return checks;
  },
};
