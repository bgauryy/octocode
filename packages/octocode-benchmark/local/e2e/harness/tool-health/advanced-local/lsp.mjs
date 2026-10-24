import { declarations, lspLocations } from '../../responses.mjs';
import { equal, files, fixturePath, relative, scenario } from './helpers.mjs';
import { cyclicCalls } from './lsp-cycles.mjs';
import { rustContexts } from './lsp-rust.mjs';

const workspaceRoot = fixturePath('code');
const model = `${workspaceRoot}/model.ts`;
const use = `${workspaceRoot}/use.ts`;
const anchor = { workspaceRoot, path: model, line: 14, symbolName: 'add' };
const locations = (ctx, data) => data.flatMap(row => lspLocations({ sc: { results: [{ data: row }] } })).map(item => [relative(ctx, item.path), item.displayRange.startLine]);

async function locationsAndReferences(ctx, surface, checks) {
  for (const view of ['content', 'files']) {
    const definition = await scenario(ctx, 'lspSearch', surface, { label: `lsp.definition.${view}`, query: { workspaceRoot, path: use, line: 4, symbolName: 'add', operation: 'definition', view }, checks, responseChars: 600 });
    const found = view === 'content' ? locations(ctx, definition.data) : files(definition.data).flatMap(file => file.lines.map(line => [relative(ctx, file.path), line]));
    equal(checks, `lsp.definition.${view}.${surface}`, found, [['code/model.ts', 14]]);
    for (const includeDeclaration of [true, false]) {
      const refs = await scenario(ctx, 'lspSearch', surface, { label: `lsp.references.${view}.${includeDeclaration}`, query: { ...anchor, operation: 'references', view, includeDeclaration }, checks, responseChars: 600 });
      const expected = [['code/model.ts', 19], ['code/model.ts', 20], ['code/use.ts', 4]];
      if (includeDeclaration) expected.push(['code/model.ts', 14], ['code/use.ts', 1]);
      const observed = view === 'content' ? locations(ctx, refs.data) : files(refs.data).flatMap(file => file.lines.map(line => [relative(ctx, file.path), line]));
      // Providers differ on whether an import binding is a declaration. Every
      // executable use remains required; the function declaration is excluded.
      const required = includeDeclaration ? observed : observed.filter(([file, line]) => file !== 'code/use.ts' || line !== 1);
      equal(checks, `lsp.reference-set.${view}.${includeDeclaration}.${surface}`, required, expected);
      checks.push([`lsp.import-binding-once.${view}.${includeDeclaration}.${surface}`, observed.filter(([file, line]) => file === 'code/use.ts' && line === 1).length <= 1, JSON.stringify(observed)]);
    }
  }
  const implementation = await scenario(ctx, 'lspSearch', surface, { label: 'lsp.implementation', query: { workspaceRoot, path: model, line: 1, symbolName: 'Shape', operation: 'implementation' }, checks });
  equal(checks, `lsp.implementations.${surface}`, locations(ctx, implementation.data), [['code/model.ts', 5], ['code/model.ts', 12]]);
  const types = await scenario(ctx, 'lspSearch', surface, { label: 'lsp.typeDefinition', query: { workspaceRoot, path: use, line: 3, symbolName: 'square', operation: 'typeDefinition' }, checks });
  checks.push([`lsp.inferred-type.${surface}`, locations(ctx, types.data).some(([file, line]) => file === 'code/model.ts' && line === 5), JSON.stringify(types.data)]);
  for (const occurrence of [0, 1]) {
    const row = await scenario(ctx, 'lspSearch', surface, { label: `lsp.occurrence.${occurrence}`, query: { workspaceRoot, path: model, line: 25, symbolName: 'run', occurrence, operation: 'definition' }, checks });
    equal(checks, `lsp.occurrence.${occurrence}.${surface}`, locations(ctx, row.data), [['code/model.ts', 23 + occurrence]]);
  }
}

async function graph(ctx, surface, checks) {
  const callers = await scenario(ctx, 'lspSearch', surface, { label: 'lsp.callers', query: { ...anchor, operation: 'callers', graphDepth: 1 }, checks, responseChars: 600 });
  const edges = files(callers.data).flatMap(file => file.matches.map(match => [relative(ctx, file.path), match.symbolName ?? '<module>', match.sites.map(site => site.line)]));
  equal(checks, `lsp.call-sites.${surface}`, edges, [['code/model.ts', 'calculate', [19, 20]], ['code/use.ts', '<module>', [4]]]);
  const callees = await scenario(ctx, 'lspSearch', surface, { label: 'lsp.callees', query: { ...anchor, line: 18, symbolName: 'calculate', operation: 'callees' }, checks });
  equal(checks, `lsp.callees.${surface}`, files(callees.data).flatMap(file => file.matches.map(match => [match.symbolName, match.line, match.sites.map(site => site.line)])), [['add', 14, [19, 20]]]);
  const supertypes = await scenario(ctx, 'lspSearch', surface, { label: 'lsp.supertypes', query: { workspaceRoot, path: model, line: 12, symbolName: 'LargeSquare', operation: 'supertypes', graphDepth: 2 }, checks });
  equal(checks, `lsp.hierarchy-depth.${surface}`, supertypes.data.flatMap(row => row.matches ?? []).map(match => [match.symbolName, match.level]), [['Square', 1], ['Shape', 2]]);
  const subtypes = await scenario(ctx, 'lspSearch', surface, { label: 'lsp.subtypes', query: { workspaceRoot, path: model, line: 5, symbolName: 'Square', operation: 'subtypes' }, checks });
  equal(checks, `lsp.subtypes.${surface}`, subtypes.data.flatMap(row => row.matches ?? []).map(match => match.symbolName), ['LargeSquare']);
}

async function information(ctx, surface, checks) {
  const hover = await scenario(ctx, 'lspSearch', surface, { label: 'lsp.hover', query: { ...anchor, operation: 'hover' }, checks });
  checks.push([`lsp.hover-signature.${surface}`, hover.data.some(data => data.hover?.contents?.value.includes('add(a: number, b: number): number')), JSON.stringify(hover.data)]);
  const symbols = await scenario(ctx, 'lspSearch', surface, { label: 'lsp.documentSymbols', query: { workspaceRoot, path: model, operation: 'documentSymbols' }, checks, responseChars: 600 });
  equal(checks, `lsp.document-functions.${surface}`, symbols.data.flatMap(data => declarations({ sc: { results: [{ data }] } })).filter(d => d.kind === 'function').map(d => [d.name, d.line]), [['add', 14], ['calculate', 18]]);
  for (const byPath of [false, true]) {
    const query = { operation: 'workspaceSymbol', symbolQuery: 'calculate', ...(byPath ? { path: model } : { workspaceRoot }) };
    const symbolsAtRoot = await scenario(ctx, 'lspSearch', surface, { label: `lsp.workspaceSymbol.${byPath}`, query: query, checks });
    equal(checks, `lsp.workspace-target.${byPath}.${surface}`, symbolsAtRoot.data.flatMap(row => row.matches ?? []).map(match => [match.symbolName, relative(ctx, match.path), match.displayRange.startLine]), [['calculate', 'code/model.ts', 18]]);
  }
  const diagnostics = await scenario(ctx, 'lspSearch', surface, { label: 'lsp.diagnostic', query: { workspaceRoot, path: `${workspaceRoot}/broken.ts`, operation: 'diagnostic' }, checks });
  checks.push([`lsp.type-error.${surface}`, diagnostics.data.some(data => data.matches?.some(item => item.code === 2322 && item.severity === 'error' && item.displayRange.startLine === 1)), JSON.stringify(diagnostics.data)]);
}

export async function advancedLsp(ctx) {
  const checks = [];
  for (const surface of ctx.surfaces) {
    await locationsAndReferences(ctx, surface, checks);
    await graph(ctx, surface, checks);
    await information(ctx, surface, checks);
    await cyclicCalls(ctx, surface, checks);
    await rustContexts(ctx, surface, checks);
  }
  return checks;
}
