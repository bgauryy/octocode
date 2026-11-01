// Representative workloads: every grammar, the large repositories, minified and
// raw bundles (compact view and matchString), GitHub, and research workflows
// chained through the identities each response returns.
import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { REPOS, TESTING } from '../paths.mjs';

// Grammar → checkout and source extensions; the declaration kind feeds an AST rule.
export const GRAMMARS = {
  typescript: { repo: 'typescript', ext: ['ts'], kind: 'function_declaration' },
  tsx: { repo: 'tsx', ext: ['tsx'], kind: 'function_declaration' },
  javascript: { repo: 'javascript', ext: ['js'], kind: 'function_declaration' },
  python: { repo: 'python', ext: ['py'], kind: 'function_definition' },
  go: { repo: 'go', ext: ['go'], kind: 'function_declaration' },
  rust: { repo: 'rust', ext: ['rs'], kind: 'function_item' },
  java: { repo: 'java', ext: ['java'], kind: 'method_declaration' },
  c: { repo: 'c', ext: ['c'], kind: 'function_definition' },
  cpp: { repo: 'cpp', ext: ['cpp', 'hpp', 'cc', 'h'], kind: 'function_definition' },
  cuda: { repo: 'cuda', ext: ['cu', 'cuh'], kind: 'function_definition' },
  csharp: { repo: 'csharp', ext: ['cs'], kind: 'method_declaration' },
  scala: { repo: 'scala', ext: ['scala'], kind: 'function_definition' },
  asm: { repo: 'asm', ext: ['asm', 's', 'S'], kind: 'label' },
};
const HUGE = ['huge-ts', 'huge-go', 'huge-java', 'huge-rust', 'huge-cpp', 'huge-c'];
const LARGEST_FILE_BYTES = 1 << 20;

const tracked = dir => execFileSync('git', ['-C', dir, 'ls-files', '-z'], { encoding: 'utf8', maxBuffer: 1 << 28 }).split('\0').filter(Boolean);

/** The largest tracked source file of these extensions up to the size cap: the stress read. */
export function largestSource(dir, ext) {
  let best = null;
  for (const rel of tracked(dir)) {
    if (!ext.includes(path.extname(rel).slice(1))) continue;
    let size;
    try { size = fs.statSync(path.join(dir, rel)).size; } catch { continue; }
    if (size <= LARGEST_FILE_BYTES && (!best || size > best.size)) best = { rel, size };
  }
  return best;
}

/** The most frequent identifier of at least eight characters: a realistic search term found in the file. */
export function frequentIdentifier(text) {
  const counts = new Map();
  for (const word of text.match(/[A-Za-z_][A-Za-z0-9_]{7,}/g) ?? []) counts.set(word, (counts.get(word) ?? 0) + 1);
  return [...counts].toSorted((a, b) => b[1] - a[1] || (a[0] < b[0] ? -1 : 1))[0]?.[0];
}

const q = query => ({ queries: [query] });

function grammarCases() {
  const cases = [];
  for (const [grammar, g] of Object.entries(GRAMMARS)) {
    const root = path.join(REPOS, g.repo);
    if (!fs.existsSync(root)) { cases.push({ group: 'grammar', name: grammar, missing: root }); continue; }
    const target = largestSource(root, g.ext);
    if (!target) { cases.push({ group: 'grammar', name: grammar, missing: `no .${g.ext.join('/.')} file` }); continue; }
    const file = path.join(root, target.rel);
    const lines = fs.readFileSync(file, 'utf8').split('\n').length;
    const term = frequentIdentifier(fs.readFileSync(file, 'utf8'));
    const dir = path.dirname(file);
    const meta = { grammar, file: `${g.repo}/${target.rel}`, fileBytes: target.size, lines, term };
    const add = (name, tool, args) => cases.push({ group: 'grammar', name: `${grammar} ${name}`, tool, args, cwd: root, meta });
    add('structure paths', 'structureSearch', q({ path: root, view: 'paths' }));
    add('search countMatches', 'localSearch', q({ path: root, matchString: [term], view: 'countMatches' }));
    add('search content', 'localSearch', q({ path: dir, matchString: [term] }));
    add('fetch content', 'localFetch', q({ path: file }));
    add('fetch symbols', 'localFetch', q({ path: file, view: 'symbols' }));
    add('fetch compact (minify)', 'localFetch', q({ path: file, view: 'compact' }));
    add('fetch matchString', 'localFetch', q({ path: file, matchString: [term], contextLines: { before: 2, after: 2 } }));
    add('fetch matchString block', 'localFetch', q({ path: file, matchString: [term], block: true }));
    add('ast symbols (dir)', 'astSearch', q({ path: dir, operation: 'symbols' }));
    add('ast match kind (repo)', 'astSearch', q({ path: root, operation: 'match', rule: { kind: g.kind }, view: 'countMatches' }));
    add('lsp documentSymbols', 'lspSearch', q({ path: file, operation: 'documentSymbols' }));
  }
  return cases;
}

function hugeCases() {
  const cases = [];
  for (const name of HUGE) {
    const root = path.join(REPOS, name);
    if (!fs.existsSync(root)) { cases.push({ group: 'huge', name, missing: root }); continue; }
    const files = tracked(root);
    // The README is a stable, language-neutral source of a search term.
    const readme = files.find(f => /^readme(\.\w+)?$/i.test(f));
    const term = frequentIdentifier(readme ? fs.readFileSync(path.join(root, readme), 'utf8') : files.join(' '));
    const meta = { repo: name, trackedFiles: files.length, term };
    const add = (label, tool, args) => cases.push({ group: 'huge', name: `${name} ${label}`, tool, args, cwd: root, meta });
    add('structure paths', 'structureSearch', q({ path: root, view: 'paths' }));
    add('search countMatches', 'localSearch', q({ path: root, matchString: [term], view: 'countMatches' }));
    add('search files', 'localSearch', q({ path: root, matchString: [term], view: 'files' }));
    add('search content', 'localSearch', q({ path: root, matchString: [term] }));
  }
  return cases;
}

function bundleCases() {
  const cases = [];
  for (const rel of ['vendor/typescript.raw.js', 'vendor/typescript.min.js', 'src/dup.min.js', 'src/cli.min.js']) {
    const file = path.join(TESTING, 'bigfiles', rel);
    if (!fs.existsSync(file)) continue;
    const text = fs.readFileSync(file, 'utf8');
    const term = frequentIdentifier(text);
    const meta = { file: `bigfiles/${rel}`, fileBytes: Buffer.byteLength(text), lines: text.split('\n').length, term };
    const add = (label, args) => cases.push({ group: 'minify+matchString', name: `${rel} ${label}`, tool: 'localFetch', args: q({ path: file, ...args }), meta });
    add('content', {});
    add('compact (minify)', { view: 'compact' });
    add('compact ranges', { view: 'compact', ranges: [{ startLine: 1, endLine: Math.max(1, Math.min(meta.lines, 2000)) }] });
    add('symbols', { view: 'symbols' });
    add('matchString', { matchString: [term] });
    add('matchString regex+context', { matchString: [`${term}\\w*\\(`], regexEngine: 'rust', contextLines: { before: 3, after: 3 } });
  }
  return cases;
}

// A tag: the default branch no longer carries the TypeScript compiler sources.
const GH_FILE = { owner: 'microsoft', repo: 'TypeScript', ref: 'v5.9.3', path: 'src/compiler/checker.ts' };
const GH_SMALL = { owner: 'facebook', repo: 'react', path: 'packages/react/src/ReactHooks.js' };

// Remote cases start in an empty directory, so no workspace language server prewarms beside them.
let empty;
export const emptyDir = () => (empty ??= fs.mkdtempSync(path.join(os.tmpdir(), 'octocode-workloads-')));

const add = (name, tool, query) => ({ group: 'github', name, tool, args: q(query), cwd: emptyDir() });

function githubCases() {
  return [
    add('ghSearchRepo search', 'ghSearchRepo', { operation: 'search', keywords: ['language server'], filters: { language: 'Rust' } }),
    add('ghSearchRepo get', 'ghSearchRepo', { operation: 'get', owner: 'facebook', repo: 'react' }),
    add('ghSearchCode', 'ghSearchCode', { owner: 'facebook', repo: 'react', keywords: ['useEffect'] }),
    add('ghStructure tree', 'ghStructure', { operation: 'tree', owner: 'facebook', repo: 'react', path: 'packages/react', maxDepth: 2 }),
    add('ghStructure refs', 'ghStructure', { operation: 'refs', owner: 'facebook', repo: 'react' }),
    add('ghGetFileContent content (small)', 'ghGetFileContent', GH_SMALL),
    add('ghGetFileContent symbols (small)', 'ghGetFileContent', { ...GH_SMALL, view: 'symbols' }),
    add('ghGetFileContent compact (small)', 'ghGetFileContent', { ...GH_SMALL, view: 'compact' }),
    add('ghGetFileContent matchString (small)', 'ghGetFileContent', { ...GH_SMALL, matchString: ['useEffect'], contextLines: { before: 2, after: 2 } }),
    add('ghGetFileContent content (checker.ts)', 'ghGetFileContent', GH_FILE),
    add('ghGetFileContent compact (checker.ts)', 'ghGetFileContent', { ...GH_FILE, view: 'compact' }),
    add('ghGetFileContent matchString (checker.ts)', 'ghGetFileContent', { ...GH_FILE, matchString: ['getTypeOfSymbol'], wholeWord: true }),
    add('ghGetFileContent symbols (checker.ts)', 'ghGetFileContent', { ...GH_FILE, view: 'symbols' }),
    add('ghSearchHistory pullRequest', 'ghSearchHistory', { operation: 'pullRequest', owner: 'facebook', repo: 'react', keywords: ['useEffect'] }),
    add('ghSearchHistory commit', 'ghSearchHistory', { operation: 'commit', owner: 'facebook', repo: 'react', keywords: ['hooks'] }),
    add('artifactSearch resolve npm', 'artifactSearch', { operation: 'resolve', ecosystem: 'npm', packageName: 'react' }),
    add('artifactSearch resolve crates', 'artifactSearch', { operation: 'resolve', ecosystem: 'crates', packageName: 'tokio' }),
    add('artifactSearch search npm', 'artifactSearch', { operation: 'search', ecosystem: 'npm', keywords: ['http client'] }),
  ];
}

/** A server that only starts in each workspace: what default LSP prewarm costs before any call. */
function idleCases() {
  return Object.entries(GRAMMARS).filter(([, g]) => fs.existsSync(path.join(REPOS, g.repo))).map(([grammar, g]) => {
    let first = true;
    return {
      group: 'idle', name: `${grammar} workspace, no calls (10 s)`, cwd: path.join(REPOS, g.repo),
      chain: async () => {
        const wait = first ? 10_000 : 0;
        first = false;
        await new Promise(r => setTimeout(r, wait));
        return [{ tool: 'idle', ms: wait, bytes: 0 }];
      },
    };
  });
}

export function allCases() {
  return [...idleCases(), ...grammarCases(), ...hugeCases(), ...bundleCases(), ...githubCases()];
}
