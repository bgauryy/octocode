// Real-repo sweep: one big repo per Tree-sitter grammar, every local tool,
// full pagination, and cross-tool correlation (text ⊇ syntax ⊇ identity).
import fs from 'node:fs';
import path from 'node:path';
import { REPOS, astMatchRows, checks, collect, declarations, findHint, rowData, sourcePath, sourceView, rootPath, startServer, structureFiles, walk, writeResults, lspLocations } from './mcp-client.mjs';

const { check, summary } = checks('repo-sweep');
const client = await startServer();
const { call, raw } = client;
const ONLY = process.argv[2]?.split(',');

const REPOS_BY_LANG = [
  { lang: 'TypeScript', dir: 'typescript', ext: ['ts'], rg: 'ts', lsp: true, scope: 'tsc/testdata/fixtures/compiler' },
  { lang: 'TSX', dir: 'tsx', ext: ['tsx'], rg: 'ts', lsp: true, scope: 'packages/excalidraw/components' },
  { lang: 'JavaScript', dir: 'javascript', ext: ['js'], rg: 'js', lsp: true, scope: '.' },
  { lang: 'Python', dir: 'python', ext: ['py'], rg: 'py', lsp: false, scope: 'django/db/models' },
  { lang: 'Go', dir: 'go', ext: ['go'], rg: 'go', lsp: false, scope: 'tsdb' },
  { lang: 'Rust', dir: 'rust', ext: ['rs'], rg: 'rust', lsp: true, scope: 'tokio/src/runtime' },
  { lang: 'Java', dir: 'java', ext: ['java'], rg: 'java', lsp: false, scope: 'guava/src/com/google/common/collect' },
  { lang: 'C', dir: 'c', ext: ['c'], rg: 'c', lsp: true, scope: 'src' },
  { lang: 'C++', dir: 'cpp', ext: ['hpp'], rg: 'cpp', lsp: true, scope: 'include/nlohmann/detail', language: 'cpp' },
  { lang: 'C#', dir: 'csharp', ext: ['cs'], rg: 'csharp', lsp: false, scope: 'Src/Newtonsoft.Json' },
  { lang: 'Scala', dir: 'scala', ext: ['scala'], rg: 'scala', lsp: false, scope: 'core/src/main/scala/cats/data' },
  { lang: 'Assembly', dir: 'asm', ext: ['asm'], rg: 'asm', lsp: false, scope: 'simd/x86_64' },
];


const table = [];
for (const r of REPOS_BY_LANG) {
  if (ONLY && !ONLY.includes(r.dir)) continue;
  const root = path.join(REPOS, r.dir);
  const scope = path.join(root, r.scope);
  const row = { lang: r.lang };
  if (!fs.existsSync(scope)) { check(`${r.lang}: scope exists`, false, scope); continue; }

  const tree = await call('structureSearch', { operation: 'tree', path: root, maxDepth: 2 });
  check(`${r.lang}: structure tree`, !tree.isError && !tree.rowErrors, `${tree.ms}ms`);
  const listing = await call('structureSearch', { operation: 'files', path: scope, extensions: r.ext, detail: 'full', sort: 'lines', maxEntries: 5 });
  const biggest = structureFiles(rowData(listing)?.files, rowData(listing)?.path).filter(o => typeof o.path === 'string' && typeof o.lineCount === 'number')[0];
  check(`${r.lang}: biggest source located`, !!biggest, listing.text.slice(0, 100));
  if (!biggest) { table.push(row); continue; }
  const L = rootPath(listing, biggest.path);
  row.file = `${path.relative(root, L)} (${biggest.lineCount} lines)`;

  // Symbols: whole file, all pages, unique ids.
  const sym1 = await call('astSearch', { operation: 'symbols', path: L, pageSize: 100, ...(r.language ? { language: r.language } : {}) });
  const symPages = await walk(client, sym1, 'nextPage', 200, { keepError: true });
  const decls = symPages.flatMap(p => declarations(p));
  // Symbols carry no id: identity is name + line + character + parent.
  const identity = d => `${d.name}|${d.line}|${d.character ?? ''}|${d.parent ?? ''}|${d.parentLine ?? ''}`;
  const dupes = decls.length - new Set(decls.map(identity)).size;
  row.symbols = `${decls.length} in ${symPages.length}p`;
  check(`${r.lang}: symbols paged to the end, unique identities`, decls.length > 0 && dupes === 0 && !symPages.some(p => p.isError), `${decls.length} dupes=${dupes} ${sym1.isError ? sym1.text.slice(0, 120) : ''}`);
  // A declaration is never its own parent (same name and kind at the same
  // line); a typedef holding a same-named struct on its line is two rows.
  const selfParented = decls.filter(d => d.parent === d.name && d.parentLine === d.line && (d.parentKind ?? d.kind) === d.kind);
  check(`${r.lang}: no self-parented symbol rows`, selfParented.length === 0, selfParented.slice(0, 3).map(d => `${d.name}@${d.line}`).join(','));

  // Pick a callable used elsewhere.
  const fns = decls.filter(d => ['function', 'method', 'constructor', 'label'].includes(d.kind) && d.name.length >= 5 && !/^(main|test|init|new|get|set)$/i.test(d.name));
  let S, textFiles = [];
  for (const candidate of fns.slice(0, 40)) {
    const s = await call('localSearch', { path: root, matchString: candidate.name, wholeWord: true, language: r.rg, resultView: 'files', pageSize: 100 });
    const sPages = await walk(client, s, 'nextPage', 50, { keepError: true });
    const files = sPages.flatMap(p => collect(rowData(p), o => typeof o.path === 'string').map(o => rootPath(p, o.path)));
    if (files.length >= 2) { S = candidate; textFiles = files; break; }
  }
  check(`${r.lang}: found a symbol referenced from ≥2 files`, !!S, `callables=${fns.length}`);
  if (!S) { table.push(row); continue; }
  row.symbol = `${S.name}@${S.line}`;
  row.textFiles = textFiles.length;

  // Syntax: calls of S across the repo, all pages; correlation with text hits.
  const callPattern = r.lang === 'Assembly' ? null : `${S.name}($$$ARGS)`;
  let callRows = [];
  if (callPattern) {
    const m1 = await call('astSearch', { operation: 'match', path: root, language: r.language ?? r.lang, pattern: callPattern, matchPageSize: 50, pageSize: 20 });
    const mPages = await walk(client, m1, 'nextPage', 100, { keepError: true });
    callRows = mPages.flatMap(p => astMatchRows(p).map(m => ({ ...m, file: rootPath(p, m.path) })));
    const callFiles = [...new Set(callRows.map(c => c.file))];
    const notText = callFiles.filter(f => !textFiles.includes(f));
    row.calls = `${callRows.length} in ${callFiles.length} files`;
    check(`${r.lang}: every syntactic call file ⊆ text hits`, !m1.isError && notText.length === 0, `calls=${callRows.length} outside=${notText.slice(0, 2).join(',')} ${m1.isError ? m1.text.slice(0, 120) : ''}`);
    let proven = 0;
    for (const c of callRows.slice(0, 5)) {
      const f = await call('localFetch', { path: c.file, ranges: [`${c.line}-${c.endLine ?? c.line}`] });
      if ((rowData(f)?.content ?? '').includes(S.name)) proven += 1;
    }
    check(`${r.lang}: sampled call rows proven by localFetch`, proven === Math.min(5, callRows.length), `${proven}/${Math.min(5, callRows.length)}`);
  }

  // Identity: AST→LSP bridge (definition from a call site lands on the symbols row).
  if (r.lsp && callRows.length) {
    const site = callRows.find(c => c.file !== L) ?? callRows[0];
    const def = await call('lspSearch', { path: site.file, symbolName: S.name, lineHint: site.line, operation: 'definition' });
    const locs = lspLocations(def);
    // The definition must land on a declaration astSearch also reports (the
    // call site may legitimately resolve to a same-named local function).
    let lands = false;
    for (const l of locs) {
      const target = sourcePath(def, l, site.file);
      const decl = await call('astSearch', { operation: 'symbols', path: target, symbolName: S.name, ...(r.language ? { language: r.language } : {}) });
      if (declarations(decl).filter(o => o.name === S.name && Math.abs(o.line - (l.displayRange?.startLine ?? 0)) <= 1).length) lands = true;
      // Nested declarations (inside functions) are not outline rows: prove
      // the landing line itself declares the name.
      if (!lands) {
        const at = l.displayRange?.startLine ?? 1;
        const line = await call('localFetch', { path: target, ranges: [`${at}-${at}`] });
        const text = sourceView(rowData(line)).text;
        if (new RegExp(`\\b(function|def|fn|func|class|var|let|const|struct)\\b[^\\n]*\\b${S.name}\\b|\\b${S.name}\\s*[:=]\\s*(function|\\()`).test(text)) lands = true;
      }
    }
    row.lspDefinition = def.isError || def.rowErrors ? `ERR ${rowData(def)?.errorCode}` : locs.length === 0 ? `none (${JSON.stringify(rowData(def)?.hints ?? rowData(def)?.payload?.reason ?? '')})` : lands ? 'lands on an astSearch declaration' : `→ ${locs.map(l => `${l.path}:${l.displayRange?.startLine}`).join(',')}`;
    // Without a compilation database clangd cannot resolve other headers;
    // the tool must say exactly that rather than return a bare empty row.
    const explainedNoDatabase = locs.length === 0 && /compile_commands\.json/.test(JSON.stringify(rowData(def)?.hints ?? []));
    check(`${r.lang}: lspSearch definition from a call site = an astSearch declaration${explainedNoDatabase ? ' (clangd: no compile database, explained)' : ''}`, lands || explainedNoDatabase, row.lspDefinition);
    const refs = await call('lspSearch', { path: L, symbolName: S.name, lineHint: S.line, operation: 'references', pageSize: 100 });
    const refPages = await walk(client, refs, 'nextPage', 50, { keepError: true });
    const refFiles = [...new Set(refPages.flatMap(p => lspLocations(p).map(o => sourcePath(p, o, L))))];
    const outside = refFiles.filter(f => !textFiles.includes(f));
    row.lspRefs = `${refFiles.length} files`;
    check(`${r.lang}: LSP reference files ⊆ text hits`, !refs.isError && outside.length === 0, `outside=${outside.slice(0, 2).join(',')}`);
  }

  // localFetch on the biggest file: pages continue gap-free; views agree.
  const f1 = await call('localFetch', { path: L, unit: 'lines', length: 400 });
  const fPages = await walk(client, f1, 'continue', 400, { keepError: true });
  const ranges = fPages.map(p => sourceView(rowData(p)).ranges).map(rr => [rr[0]?.start, rr.at(-1)?.end]);
  const byteMode = fPages.map(p => rowData(p)?.pagination?.unit === 'bytes');
  let gapFree = ranges[0]?.[0] === 1;
  // A line longer than a page is served as byte pages: the next page may
  // continue the same line (the final byte page carries no pagination, so a
  // byte-mode predecessor also licenses the same-line continuation).
  for (let i = 1; i < ranges.length; i++) gapFree &&= ranges[i][0] === ranges[i - 1][1] + 1 || ((byteMode[i] || byteMode[i - 1]) && ranges[i][0] === ranges[i - 1][1]);
  row.fetchPages = `${fPages.length} → ${ranges.at(-1)?.[1]}/${biggest.lineCount}`;
  check(`${r.lang}: localFetch pages cover the whole file gap-free`, gapFree && ranges.at(-1)?.[1] === biggest.lineCount, row.fetchPages);
  const ms = await call('localFetch', { path: L, matchString: S.name, contextLines: 1 });
  check(`${r.lang}: localFetch matchString finds ${S.name}`, (rowData(ms)?.matchRanges?.length ?? 0) > 0 || (rowData(ms)?.content ?? '').includes(S.name), rowData(ms)?.errorCode ?? '');
  const outline = await call('localFetch', { path: L, minify: 'symbols' });
  // Walk the outline's pages: the symbol must appear somewhere in the view.
  const outlinePages = await walk(client, outline, 'continue', 40, { keepError: true });
  const named = outlinePages.some(pg => (rowData(pg)?.content ?? '').includes(S.name));
  check(`${r.lang}: symbols view names ${S.name}`, named, `${outlinePages.length} page(s) ${rowData(outline)?.contentView ?? rowData(outline)?.errorCode}`);

  // syntaxTree pages advance.
  const t1 = await call('astSearch', { operation: 'syntaxTree', path: L, namedOnly: true, pageSize: 200, ...(r.language ? { language: r.language } : {}) });
  const tPages = await walk(client, t1, 'nextPage', 3, { keepError: true });
  check(`${r.lang}: syntaxTree pages`, !t1.isError && !t1.rowErrors && (tPages.length > 1 || !findHint(t1.sc, 'nextPage')), `${t1.ms}ms pages=${tPages.length} ${t1.isError || t1.rowErrors ? t1.text.slice(0, 120) : ''}`);
  table.push(row);
}
console.table(table);
const result = summary();
writeResults('repo-sweep', { table, ...result, calls: client.log.map(({ text, sc, ...meta }) => meta) });
client.close();
process.exitCode = result.failed.length ? 1 : 0;
