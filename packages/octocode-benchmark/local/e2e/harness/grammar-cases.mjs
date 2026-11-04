import path from 'node:path';
import { declarations, astMatchRows, lspLocations, sourceLines, structureFiles, collect } from './responses.mjs';
import { grammarAnchors } from './grammar-lsp.mjs';

const data = pages => pages.flatMap(page => page.json?.results ?? []).map(row => row.data);
const sorted = rows => rows.map(row => JSON.stringify(row)).toSorted();
const same = (a, b) => JSON.stringify(sorted(a)) === JSON.stringify(sorted(b));
export const captureMatchesFixture = (fixture, sourceLine, capture) => !!capture && capture.text === fixture.argument
  && capture.column === sourceLine.indexOf(fixture.argument) + 1
  && sourceLine.slice(0, capture.column - 1).includes('😀');
const symbols = pages => {
  const context = new Map();
  return pages.flatMap(page => declarations(page, 0, context)).map(row => [row.name, row.kind, row.line, row.endLine, row.parent]);
};

export async function runGrammarCase(entry, surface, walk, check) {
  const { fixture: f, file, language, ext, capability } = entry;
  const label = `${language}.${ext}.${surface}`;
  const query = async (tool, fields, tag, small = true) => walk(tool,
    { queries: [{ ...fields, ...(small && fields.operation !== 'syntaxTree' && { limit: 1 }) }], ...(small && { responseChars: 300 }) }, surface, `${label}.${tag}`);
  const expectedLines = f.src.trimEnd().split('\n').map((line, index) => [index + 1, line]);
  const outcomes = {};
  const fullSymbols = await query('astSearch', { path: file, operation: 'symbols' }, 'symbols.full', false);
  const pagedSymbols = await query('astSearch', { path: file, operation: 'symbols' }, 'symbols.paged');
  const declarationNames = symbols(pagedSymbols).map(row => row[0]);
  check(`${label}.symbols.expected`, f.names.every(name => declarationNames.includes(name)), { expected: f.names, observed: declarationNames });
  check(`${label}.symbols.exact-pages`, same(symbols(fullSymbols), symbols(pagedSymbols)), { full: symbols(fullSymbols), paged: symbols(pagedSymbols) });
  outcomes.symbols = sorted(symbols(pagedSymbols));

  const parser = language === 'TSX' ? 'tsx' : capability.languageId;
  const kind = await query('astSearch', { path: file, operation: 'match', parser, rule: { kind: f.kind }, view: 'countMatches' }, 'kind');
  check(`${label}.kind`, data(kind).flatMap(row => row.files ?? []).reduce((sum, row) => sum + (row.matchCount ?? 0), 0) > 0, data(kind));
  if (f.call) {
    const match = await query('astSearch', { path: file, operation: 'match', parser, rule: { pattern: `${f.call}($A)` }, includeCaptures: true }, 'match');
    const matches = match.flatMap(page => astMatchRows(page));
    const anchors = grammarAnchors(language, f);
    const capture = matches.find(row => row.line === anchors.position.line + 1)?.metavarRanges?.A?.[0];
    const sourceLine = expectedLines[anchors.position.line][1];
    const expectedColumn = sourceLine.indexOf(f.argument) + 1;
    check(`${label}.match.UTF16-after-emoji`, captureMatchesFixture(f, sourceLine, capture), { capture, expectedText: f.argument, expectedColumn });
    outcomes.matches = matches.map(({ path: _path, ...row }) => row);
  }
  const tree = await query('astSearch', { path: file, operation: 'syntaxTree', namedOnly: true, includeProvenance: true }, 'syntaxTree');
  check(`${label}.syntaxTree.source-extent`, collect(data(tree), row => row.sourceRange?.startByte === 0 && row.sourceRange?.endByte === Buffer.byteLength(f.src)).length > 0, data(tree));

  const raw = await query('localFetch', { path: file, view: 'content' }, 'fetch.content');
  const lines = data(raw).flatMap(sourceLines);
  check(`${label}.fetch.exact-source`, same(lines, expectedLines) && lines.length === expectedLines.length, { expected: expectedLines, observed: lines });
  outcomes.lines = lines;
  for (const view of ['symbols', 'compact']) {
    const full = await query('localFetch', { path: file, view }, `fetch.${view}.full`, false);
    const paged = await query('localFetch', { path: file, view }, `fetch.${view}.paged`);
    const text = pages => data(pages).map(row => row.content ?? '').join('');
    check(`${label}.fetch.${view}.exact-pages`, text(full) === text(paged) && text(paged).includes(f.names.at(-1)), { full: text(full), paged: text(paged) });
    outcomes[view] = text(paged);
  }
  await checkSearchAndStructure({ entry, label, query, check, outcomes, expectedLines });

  const anchors = grammarAnchors(language, f);
  for (const operation of ['definition', 'references']) {
    const pages = await query('lspSearch', { path: file, workspaceRoot: path.dirname(file), operation,
      symbolName: anchors.symbol, line: anchors.position.line + 1, ...(operation === 'references' && { includeDeclaration: true }) }, `lsp.${operation}`);
    const locations = pages.flatMap(page => lspLocations(page));
    const positions = locations.map(row => [row.displayRange.startLine, row.displayRange.startCharacter]);
    const expected = (operation === 'definition' ? [anchors.target] : anchors.locations).map(row => [row.start.line + 1, row.start.character + 1]);
    check(`${label}.lsp.${operation}.protocol-evidence`, same(positions, expected)
      && locations.every(row => path.resolve(path.dirname(path.dirname(file)), row.path) === file), { expected, observed: positions, locations });
    outcomes[operation] = sorted(positions);
  }
  return outcomes;
}

async function checkSearchAndStructure({ entry, label, query, check, outcomes, expectedLines }) {
  const { fixture: f, file, ext, capability } = entry;
  const search = await query('localSearch', { path: file, matchString: [f.names.at(-1)], view: 'content' }, 'search.content');
  const hits = data(search).flatMap(row => row.files ?? []).flatMap(row => row.matches ?? []).map(row => [row.line, row.value]);
  const expectedHits = expectedLines.filter(([, line]) => line.includes(f.names.at(-1)));
  check(`${label}.search.exact-source`, same(hits, expectedHits), { expected: expectedHits, observed: hits });
  outcomes.search = sorted(hits);
  const extensionSearch = await query('localSearch', { path: path.dirname(file), matchString: [f.names.at(-1)], include: [`*.${ext}`], view: 'files' }, 'search.extension');
  const extensionFiles = data(extensionSearch).flatMap(row => row.files ?? []).map(row => path.basename(row.path));
  check(`${label}.search.extension`, same(extensionFiles, [path.basename(file)]), { expected: [path.basename(file)], observed: extensionFiles });
  const fileSearch = await query('localSearch', { path: path.dirname(file), matchString: [f.names.at(-1)], fileType: f.rg, view: 'files' }, 'search.fileType');
  // fileType is explicitly a ripgrep type, not the native parser's extension set.
  // Verify the known primary type; retain the complete observed set for comparison.
  const typeFiles = data(fileSearch).flatMap(row => row.files ?? []).map(row => path.basename(row.path));
  check(`${label}.search.fileType`, typeFiles.includes(`main.${f.ext}`) && new Set(typeFiles).size === typeFiles.length
    && typeFiles.every(name => capability.extensions.some(extension => name === `main.${extension}`)), { type: f.rg, observed: typeFiles });
  outcomes.fileTypeExtensions = typeFiles.map(name => path.extname(name).slice(1)).toSorted((a, b) => a.localeCompare(b));
  for (const operation of ['files', 'tree']) {
    const structure = await query('structureSearch', { path: path.dirname(file), operation, include: [`*.${ext}`] }, `structure.${operation}`);
    const found = data(structure).flatMap(row => structureFiles(row.files ?? row.entries ?? [])).filter(row => row.type !== 'directory').map(row => path.basename(row.path));
    check(`${label}.structure.${operation}.exact-file`, same(found, [path.basename(file)]), { found, expected: path.basename(file) });
  }

}
