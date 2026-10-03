// Edge cases: each scenario runs an expert shell command and the octocode equivalent, stores both outputs,
// and applies an automatic check where one is decidable. Judgments are summarised in REPORT.md.
import path from 'node:path';
import fs from 'node:fs';
import { HERE, REPOS, sh, oc, ocAll, ocRaw, saveRaw } from './lib.mjs';

const P = path.join(HERE, 'fixtures/edge/proj');
const S = (f) => path.join(P, f);
const CHECKER = path.join(REPOS, 'typescript/tsc/testdata/fixtures/compiler/checker.ts');
const out = [];
const cut = (s, n = 1200) => s.length > n ? s.slice(0, n) + `…[+${s.length - n} chars]` : s;
function scen(id, title, shellCmd, ocTool, ocQuery, check, { shellCwd = P, ocCwd = P, follow = false, envelope } = {}) {
  const s = sh(shellCmd, { cwd: shellCwd, reps: 1 });
  const o = follow ? ocAll(ocTool, [ocQuery], { cwd: ocCwd, reps: 1 }) : oc(ocTool, ocQuery, { cwd: ocCwd, reps: 1, envelope });
  const ostdout = follow ? o.steps.map(x => x.stdout).join('\n') : o.stdout;
  const res = check ? check(s, ostdout, o) : {};
  const rec = { id, title, shell: { cmd: shellCmd, chars: s.chars, ms: s.ms, code: s.code, out: cut(s.stdout + s.stderr, 3000) },
    octocode: { tool: ocTool, query: ocQuery, calls: o.calls || 1, chars: o.chars, ms: o.ms, code: o.code, out: cut(ostdout + (o.stderr || ''), 4000) }, check: res };
  out.push(rec);
  console.log(`\n### ${id}: ${title}\n  shell: ${s.chars}c ${s.ms}ms exit=${s.code} :: ${cut((s.stdout + s.stderr).replace(/\n/g, '⏎'), 260)}\n  oc:    ${rec.octocode.calls}x ${o.chars}c ${o.ms}ms exit=${o.code} :: ${cut(ostdout.replace(/\n/g, '⏎'), 700)}\n  check: ${JSON.stringify(res)}`);
  return rec;
}
const has = (s, x) => s.includes(x);

// ---------- huge file (3.2 MB CRLF checker.ts)
scen('huge-search-one', 'Find one declaration in 3.2MB checker.ts', `rg -n 'function checkSourceElementWorker' ${CHECKER}`, 'localSearch',
  { path: CHECKER, searchText: 'function checkSourceElementWorker', regex: 'literal' }, (s, o) => ({ bothLine49064: has(s.stdout, '49064:') && has(o, '"line":49064') }), { shellCwd: REPOS, ocCwd: REPOS });
scen('huge-search-many', 'Frequent identifier getTypeOfSymbol in checker.ts (default output)', `rg -n -w getTypeOfSymbol ${CHECKER}`, 'localSearch',
  { path: CHECKER, searchText: 'getTypeOfSymbol', wholeWord: true }, (s, o) => ({ rgLines: s.stdout.split('\n').filter(Boolean).length, ocReportsTotal: (o.match(/"totalMatches":(\d+)/) || [])[1] }), { shellCwd: REPOS, ocCwd: REPOS });
scen('huge-fetch-full', 'Read whole 3.2MB checker.ts (cat vs fullContent)', `cat ${CHECKER}`, 'localFetch',
  { path: CHECKER, fullContent: true }, (s, o) => ({ catChars: s.stdout.length, ocMentionsClip: /clip|truncat|too large|large|chunk|responseChar|hasMore/i.test(o) }), { shellCwd: REPOS, ocCwd: REPOS });
scen('huge-symbols', 'Declarations in checker.ts (astSearch symbols vs rg decl regex)', `rg -c '^\\s*function \\w+' ${CHECKER}`, 'astSearch',
  { operation: 'symbols', path: CHECKER, kinds: ['function'], pageSize: 100 }, (s, o) => ({ rgFunctionLines: +s.stdout.trim(), ocTotal: (o.match(/"totalDeclarations":(\d+)/) || [])[1] }), { shellCwd: REPOS, ocCwd: REPOS });
scen('huge-structure', 'Find files >1MB under typescript/tsc/testdata (find -size vs structureSearch size)', `find typescript/tsc/testdata -type f -size +1024k`, 'structureSearch',
  { operation: 'files', path: path.join(REPOS, 'typescript/tsc/testdata'), size: { greater: '1m' }, entryType: 'f', detail: 'full', sort: 'size', pageSize: 50 },
  (s, o) => ({ findCount: s.stdout.split('\n').filter(Boolean).length, ocFiles: (o.match(/"path":/g) || []).length - 1 }), { shellCwd: REPOS, ocCwd: REPOS });

// ---------- minified single-line 2 MB JS
scen('minified-rg-naive', 'Search in 2MB single-line minified JS (naive rg prints the whole line)', `rg -n 'function f12345\\b' src/vendor.min.js`, 'localSearch',
  { path: S('src/vendor.min.js'), searchText: 'function f12345\\b' }, (s, o) => ({ rgChars: s.chars, ocChars: o.length, ocHasHit: has(o, 'f12345') }));
scen('minified-rg-expert', 'Same, expert rg -o with a ±60 char window', `rg -n -o '.{0,60}function f12345\\b.{0,60}' src/vendor.min.js`, 'localSearch',
  { path: S('src/vendor.min.js'), searchText: 'function f12345\\b', resultView: 'matchOnly', matchWindow: 60 }, (s, o) => ({ rgChars: s.chars, ocChars: o.length }));
scen('minified-fetch', 'Read around a match in minified JS (grep -o window vs localFetch contextBytes)', `grep -o '.\\{0,80\\}function f12345(.\\{0,80\\}' src/vendor.min.js`, 'localFetch',
  { path: S('src/vendor.min.js'), matchString: 'function f12345(', matchStringCaseSensitive: true, contextBytes: 80 }, (s, o) => ({ ocChars: o.length, ocHasHit: has(o, 'f12345') }));

// ---------- binary
scen('binary-search', 'Text search hitting a binary file', `rg -n NEEDLE_BIN src`, 'localSearch', { path: S('src'), searchText: 'NEEDLE_BIN' },
  (s, o) => ({ rgSays: s.stdout.trim().slice(0, 120), ocMentionsBinary: /binary|NUL/i.test(o) }));
scen('binary-fetch', 'Read a binary file', `cat -v src/blob.bin`, 'localFetch', { path: S('src/blob.bin'), fullContent: true },
  (s, o) => ({ ocRefusesOrFlags: /binary|NUL|not text|error/i.test(o) }));

// ---------- non-UTF8 (latin-1)
scen('latin1-search', 'Search a latin-1 (non-UTF8) file', `rg -n NEEDLE_LATIN1 src/latin1.txt`, 'localSearch', { path: S('src/latin1.txt'), searchText: 'NEEDLE_LATIN1' },
  (s, o) => ({ ocHit: has(o, '"line":1'), ocFlagsEncoding: /utf|encod|invalid|lossy|�/i.test(o) }));
scen('latin1-search-accent', 'Search "café" (UTF-8 query) in a latin-1 file', `rg -n 'café' src/latin1.txt; rg -n -E latin1 'café' src/latin1.txt`, 'localSearch', { path: S('src/latin1.txt'), searchText: 'café' },
  (s, o) => ({ rgEncodingFlagFinds: /1:/.test(s.stdout), ocFinds: has(o, '"line"') }));
scen('latin1-fetch', 'Read a latin-1 file', `cat src/latin1.txt`, 'localFetch', { path: S('src/latin1.txt'), fullContent: true },
  (s, o) => ({ ocHasReplacementChar: o.includes('�') || o.includes('\\ufffd'), ocFlagsEncoding: /utf|encod|lossy|binary/i.test(o) }));

// ---------- symlinks
scen('symlink-search', 'Search through symlinks pointing outside root (/etc/hosts, ../lib.mjs)', `rg -n 'localhost|ocAll' src; echo '--- with -L:'; rg -L -l 'localhost|ocAll' src`, 'localSearch',
  { path: S('src'), searchText: 'localhost|ocAll' }, (s, o) => ({ ocLeaksHosts: has(o, 'localhost'), ocLeaksOutside: has(o, 'ocAll') }));
scen('symlink-fetch', 'Read a symlink that points outside the root', `cat src/link_hosts | head -3`, 'localFetch', { path: S('src/link_hosts'), fullContent: true },
  (s, o) => ({ ocRefused: /outside|not allowed|symlink|denied|error/i.test(o) && !has(o, 'localhost') }));
scen('symlink-fetch-rel', 'Read a relative symlink escaping the root', `head -3 src/link_outside`, 'localFetch', { path: S('src/link_outside'), startLine: 1, endLine: 3 },
  (s, o) => ({ ocRefused: /outside|not allowed|symlink|denied|error/i.test(o) && !has(o, 'Measurement harness') }));
scen('symlink-tree', 'Tree listing containing symlinks', `find src -maxdepth 1 | sort`, 'structureSearch', { operation: 'tree', path: S('src'), maxDepth: 1 },
  (s, o) => ({ ocListsLinks: has(o, 'link_hosts'), ocDescendsLinkDir: /link_dir\/\w/.test(o) }));
scen('sandbox-abs', 'Absolute path outside the workspace root', `head -2 /etc/hosts`, 'localFetch', { path: '/etc/hosts', startLine: 1, endLine: 2 },
  (s, o) => ({ ocRefused: /outside/i.test(o) }));
scen('sandbox-dotdot', 'Path traversal with ..', `head -2 src/../../../../lib.mjs`, 'localFetch', { path: S('src/../../../../lib.mjs'), startLine: 1, endLine: 2 },
  (s, o) => ({ ocRefused: /outside/i.test(o) }));

// ---------- ignored / dependency / build dirs
scen('ignore-default', 'Default search: gitignored dir, node_modules, target, dist, .env, *.log', `rg -l NEEDLE_ .`, 'localSearch', { path: P, searchText: 'NEEDLE_', resultView: 'files', pageSize: 100 },
  (s, o) => ({ rg: s.stdout.split('\n').filter(Boolean).sort(), oc: [...o.matchAll(/"path":"([^"]+)"/g)].map(m => m[1]).sort() }));
scen('ignore-defaultExcludes-false', 'defaultExcludes:false vs rg (dependency/build dirs)', `rg -l NEEDLE_ .`, 'localSearch', { path: P, searchText: 'NEEDLE_', resultView: 'files', defaultExcludes: false, pageSize: 100 },
  (s, o) => ({ oc: [...o.matchAll(/"path":"([^"]+)"/g)].map(m => m[1]).sort() }));
scen('ignore-noIgnore-hidden', 'noIgnore+hidden+defaultExcludes:false vs rg --no-ignore --hidden', `rg -l --no-ignore --hidden NEEDLE_ .`, 'localSearch', { path: P, searchText: 'NEEDLE_', resultView: 'files', defaultExcludes: false, noIgnore: true, hidden: true, pageSize: 100 },
  (s, o) => ({ rg: s.stdout.split('\n').filter(Boolean).sort(), oc: [...o.matchAll(/"path":"([^"]+)"/g)].map(m => m[1]).sort() }));
scen('ignore-real-target', 'Real repo: rust/target (81MB, gitignored) — files mentioning tokio', `rg -l 'tokio' rust | wc -l; rg -l --no-ignore 'tokio' rust | wc -l`, 'localSearch',
  { path: path.join(REPOS, 'rust'), searchText: 'tokio', caseMode: 'sensitive', resultView: 'files', pageSize: 1 }, (s, o) => ({ rgCounts: s.stdout.trim().split('\n'), ocTotalFiles: (o.match(/"totalFiles":(\d+)/) || [])[1] }), { shellCwd: REPOS, ocCwd: REPOS });
scen('ignore-real-target-off', 'Real repo: same with defaultExcludes:false + noIgnore', `rg -l --no-ignore 'tokio' rust | wc -l`, 'localSearch',
  { path: path.join(REPOS, 'rust'), searchText: 'tokio', caseMode: 'sensitive', resultView: 'files', pageSize: 1, defaultExcludes: false, noIgnore: true }, (s, o) => ({ rg: s.stdout.trim(), ocTotalFiles: (o.match(/"totalFiles":(\d+)/) || [])[1] }), { shellCwd: REPOS, ocCwd: REPOS });

// ---------- zero results / regex error
scen('zero-results', 'No matches', `rg -n ZZZ_DEFINITELY_ABSENT_42 src`, 'localSearch', { path: S('src'), searchText: 'ZZZ_DEFINITELY_ABSENT_42' }, (s, o) => ({ rgExit: s.code, ocHints: /hint/i.test(o) }));
scen('zero-results-ast', 'No structural matches', `ast-grep run -p 'nonexistentFn($A)' -l typescript src`, 'astSearch', { operation: 'match', path: S('src'), langType: 'typescript', pattern: 'nonexistentFn($A)' }, (s, o) => ({ ocMsg: o.slice(0, 300) }));
scen('regex-error', 'Invalid regex', `rg -n 'foo(bar' src`, 'localSearch', { path: S('src'), searchText: 'foo(bar' }, (s, o) => ({ rgExit: s.code, ocError: /regex|parse|unclosed|group/i.test(o) }));
scen('regex-error-ast', 'Invalid ast-grep pattern', `ast-grep run -p 'foo(' -l typescript src`, 'astSearch', { operation: 'match', path: S('src'), langType: 'typescript', pattern: 'foo(' }, (s, o) => ({ ocMsg: o.slice(0, 400) }));

// ---------- unicode columns: truth: NEEDLE_EMOJI on line 1 starts at UTF-16 col (0-based) 26, byte col (1-based) 31, char col 24
{
  const line = fs.readFileSync(S('src/emoji.ts'), 'utf8').split('\n')[0];
  const idx16 = line.indexOf('NEEDLE_EMOJI'); const byte1 = Buffer.byteLength(line.slice(0, idx16)) + 1; const cp0 = [...line.slice(0, idx16)].length;
  const truth = { utf16_0based: idx16, byte_1based: byte1, codepoint_0based: cp0 };
  scen('unicode-col-search', 'Column of a hit after emoji (rg --column is bytes)', `rg -n --column NEEDLE_EMOJI src/emoji.ts`, 'localSearch', { path: S('src/emoji.ts'), searchText: 'NEEDLE_EMOJI', resultView: 'matchOnly' },
    (s, o) => ({ truth, rgColumn: +(s.stdout.split('\n')[0].split(':')[1] || 0), ocColumns: [...o.matchAll(/"column":(\d+)/g)].map(m => +m[1]) }));
  scen('unicode-col-ast', 'astSearch column after emoji (documented UTF-16 0-based)', `ast-grep run -p 'const NEEDLE_EMOJI = $V;' -l typescript src/emoji.ts --json=stream | jq -c '.range.start'`, 'astSearch',
    { operation: 'match', path: S('src/emoji.ts'), langType: 'typescript', pattern: 'const NEEDLE_EMOJI = $V;' },
    (s, o) => ({ truthConstKeyword_utf16: line.indexOf('const NEEDLE_EMOJI'), sgStart: s.stdout.trim(), ocColumns: [...o.matchAll(/"column":(\d+)/g)].map(m => +m[1]) }));
  scen('unicode-col-symbols', 'astSearch symbols line for emojiFn after emoji comment', `rg -n 'function emojiFn' src/emoji.ts`, 'astSearch', { operation: 'symbols', path: S('src/emoji.ts') },
    (s, o) => ({ ocHasEmojiFnLine2: /"name":"emojiFn"[^}]*"line":2/.test(o) }));
}

// ---------- secrets
scen('secret-search', 'Search a file holding fake credentials', `rg -n 'AWS_SECRET|GITHUB_TOKEN|DB_URL|PRIVATE' src/config.ts`, 'localSearch', { path: S('src/config.ts'), searchText: 'AWS_SECRET|GITHUB_TOKEN|DB_URL|PRIVATE', contextLines: 2 },
  (s, o) => ({ rgLeaks: ['wJalrXUtnFEMI', 'ghp_aBcDe', 'hunter2secret'].filter(x => has(s.stdout, x)), ocLeaks: ['wJalrXUtnFEMI', 'ghp_aBcDe', 'hunter2secret', 'MIIEowIBAAKC', 'AKIAIOSFODNN7'].filter(x => has(o, x)) }));
scen('secret-fetch', 'Read the credentials file', `cat src/config.ts`, 'localFetch', { path: S('src/config.ts'), fullContent: true },
  (s, o) => ({ ocLeaks: ['wJalrXUtnFEMI', 'ghp_aBcDe', 'hunter2secret', 'MIIEowIBAAKC', 'b2x0aW5nIGEgZmFrZ', 'AKIAIOSFODNN7'].filter(x => has(o, x)) }));
scen('secret-ast', 'Structural match over the credentials file', `ast-grep run -p 'export const $N = $V' -l typescript src/config.ts`, 'astSearch', { operation: 'match', path: S('src/config.ts'), langType: 'typescript', pattern: 'export const $N = $V' },
  (s, o) => ({ ocLeaks: ['wJalrXUtnFEMI', 'ghp_aBcDe', 'hunter2secret', 'MIIEowIBAAKC', 'AKIAIOSFODNN7'].filter(x => has(o, x)) }));
scen('secret-ast-terminated', 'Structural match with terminated pattern over the credentials file', `ast-grep run -p 'export const $N = $V;' -l typescript src/config.ts`, 'astSearch', { operation: 'match', path: S('src/config.ts'), langType: 'typescript', pattern: 'export const $N = $V;' },
  (s, o) => ({ sgLeaks: ['wJalrXUtnFEMI', 'ghp_aBcDe', 'hunter2secret'].filter(x => has(s.stdout, x)), ocLeaks: ['wJalrXUtnFEMI', 'ghp_aBcDe', 'hunter2secret', 'MIIEowIBAAKC', 'AKIAIOSFODNN7'].filter(x => has(o, x)) }));
scen('ast-strictness', 'Unterminated pattern: ast-grep CLI matches, astSearch does not', `ast-grep run -p 'const NEEDLE_EMOJI = $V' -l typescript src/emoji.ts --json=stream | jq -c '.range.start'`, 'astSearch',
  { operation: 'match', path: S('src/emoji.ts'), langType: 'typescript', pattern: 'const NEEDLE_EMOJI = $V' }, (s, o) => ({ sgHits: s.stdout.trim().split('\n').filter(Boolean).length, ocHits: (o.match(/"line":/g) || []).length }));
scen('secret-dotenv', 'Read .env', `cat .env`, 'localFetch', { path: S('.env'), fullContent: true }, (s, o) => ({ ocRefusedOrRedacted: !has(o, 'SECRET=abc') }));

// ---------- CRLF
scen('crlf-fetch', 'CRLF file line fidelity', `sed -n '1,2p' src/crlf.txt | od -c | head -3`, 'localFetch', { path: S('src/crlf.txt'), startLine: 1, endLine: 2 }, (s, o) => ({ ocKeepsCR: has(o, '\\r') }));

// ---------- missing path
scen('missing-path', 'Nonexistent path', `rg -n foo src/nope.ts`, 'localSearch', { path: S('src/nope.ts'), searchText: 'foo' }, (s, o) => ({ ocErr: (o.match(/"errorCode":"([^"]+)"/) || [])[1] }));

// ---------- astRewrite stale-hash guard (file changes between preview and apply)
{
  const f = S('src/rw.ts'); fs.writeFileSync(f, 'console.log("a");\nconsole.log("b");\n');
  const prev = oc('astRewrite', { path: S('src'), langType: 'typescript', ruleKind: 'pattern', pattern: 'console.log($A)', rewrite: 'logger.info($A)', include: ['rw.ts'] }, { cwd: P, reps: 1 });
  const nx = prev.parsed?.results?.[0]?.data?.hints?.apply?.query;
  fs.appendFileSync(f, 'console.log("c");\n');
  const ap = nx ? ocRaw('astRewrite', { queries: [nx] }, { cwd: P, reps: 1 }) : null;
  const after = fs.readFileSync(f, 'utf8');
  const rec = { id: 'rewrite-stale-guard', title: 'astRewrite apply after the file changed since preview', shell: { cmd: 'ast-grep -U has no preview/apply guard (applies to whatever is on disk)', chars: 0, ms: 0 },
    octocode: { calls: 2, chars: prev.chars + (ap?.chars || 0), ms: prev.ms + (ap?.ms || 0), out: cut((prev.stdout || '') + '\n' + (ap?.stdout || ''), 3000) },
    check: { hadApply: !!nx, fileUnchangedByApply: after === 'console.log("a");\nconsole.log("b");\nconsole.log("c");\n', applyRejected: /hash|changed|stale|mismatch|conflict/i.test(ap?.stdout || '') } };
  out.push(rec); console.log(`\n### ${rec.id}\n  ${JSON.stringify(rec.check)}\n  ${cut((ap?.stdout || '').replace(/\n/g, '⏎'), 500)}`);
  fs.rmSync(f);
}
saveRaw('edge', out);
