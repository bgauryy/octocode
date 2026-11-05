// Every error code a public call can reach, the cheapest way a fixture can reach it.
// The ones nothing can reach live in unreachable.mjs, each with its reason.
import { spawnSync } from 'node:child_process';
import FS from 'node:fs';
import PATH from 'node:path';
import { dataOf } from './scenario-helpers.mjs';

const OP_GET = { operation: 'get', owner: 'octo', repo: 'demo' };
const answerRepo = answer => request => (request.p === '/repos/octo/demo' ? answer : null);
const code = (id, tool) => (query, codes, more = {}) => ({ id: `code.${id}`, tool, query, codes: [codes].flat(), ...more });
const github = (status, codes, more = {}) => code(`github.${codes}`, 'ghSearchRepo')(OP_GET, codes, { github: answerRepo({ status, body: { message: `Fixture ${status}` } }), ...more });
const provider = (id, codes, answer, more = {}) => code(`provider.${id}`, 'clasify')({ operation: 'evaluate', content: `state ${id}`, questions: { k: { type: 'noul', instructions: 'x?' } } }, codes, { provider: answer, env: ({ dir }) => ({ OCTOCODE_HOME: `${dir}/.account` }), ...more });

const local = [
  code('binary', 'localFetch')({ path: '$DIR/b.bin' }, 'binaryFileUnsupported', { files: { 'b.bin': Buffer.from([0, 1, 2, 0, 3]) } }),
  code('notAFile', 'localFetch')({ path: '$DIR' }, 'notAFile'),
  code('notFound.path', 'localFetch')({ path: '$DIR/missing.txt' }, 'pathNotFound'),
  code('notADirectory', 'structureSearch')({ path: '$DIR/a.txt', operation: 'tree' }, 'notADirectory', { files: { 'a.txt': 'x\n' } }),
  code('invalidPattern', 'structureSearch')({ path: '$DIR', operation: 'files', nameRegex: '(' }, 'invalidPattern'),
  code('invalidGlob', 'astSearch')({ path: '$DIR', operation: 'symbols', parserGlobs: { cpp: ['['] } }, 'invalidGlob', { files: { 'a.ts': 'export const a = 1;\n' } }),
  code('languageDirectoryRequired', 'astSearch')({ path: '$DIR/a.ts', operation: 'match', rule: { pattern: 'f()' }, parserGlobs: { typescript: ['*.ts'] } }, 'languageDirectoryRequired', { files: { 'a.ts': 'f();\n' } }),
  code('languageMismatch', 'astSearch')({ path: '$DIR/b.py', operation: 'match', parser: 'typescript', rule: { pattern: 'old()' } }, 'languageMismatch', { files: { 'b.py': 'old()\n' } }),
  code('languageRequired', 'astSearch')({ path: '$DIR', operation: 'match', rule: { pattern: 'b()' } }, 'languageRequired', { files: { 'notes.txt': 'b()\n' } }),
  code('languageUnsupported', 'astSearch')({ path: '$DIR/data.json', operation: 'syntaxTree' }, 'languageUnsupported', { files: { 'data.json': '{}\n' } }),
  code('largeSourceWindowOnly', 'localFetch')({ path: '$DIR/big.txt', view: 'symbols' }, 'largeSourceWindowOnly', { files: { 'big.txt': { sparse: 10 * 1024 * 1024 + 1, head: 'text\n' } } }),
  code('symbolsUnsupported', 'astSearch')({ path: '$DIR/a.md', operation: 'symbols' }, 'symbolsUnsupported', { files: { 'a.md': '# title\n' } }),
  code('invalidInput.graphFile', 'graphBuild')({ path: '$DIR/a.ts' }, 'invalidInput', { files: { 'a.ts': 'x\n' } }),
  code('fileAccessFailed', 'localFetch')({ path: '$DIR/a.txt' }, 'fileAccessFailed', { files: { 'a.txt': 'x\n' }, setup: ({ dir }) => chmod(dir, 'a.txt', 0o000) }),
  code('pathValidationFailed', 'localFetch')({ path: '$DIR/loop' }, 'pathValidationFailed', { files: { loop: { symlink: 'loop' } } }),
  code('notFound.noSnapshot', 'graphQuery')({ operation: 'stats', path: '$DIR' }, 'notFound', { files: { 'a.ts': 'x\n' } }),
  code('fileTooLarge.search', 'localSearch')({ path: '$DIR/big.txt', matchString: ['needle'], multiline: 'on', regexEngine: 'rust' }, 'fileTooLarge', { files: { 'big.txt': { sparse: 20 * 1024 * 1024 + 1, head: 'needle\n' } } }),
  { id: 'code.staleSnapshot', tool: 'localFetch', codes: ['staleSnapshot'], files: { 'a.txt': Array.from({ length: 4000 }, (_, i) => `line ${i}\n`).join('') }, run: async ({ call, dir }) => {
    const first = await call('localFetch', { queries: [{ path: '$DIR/a.txt' }], responseChars: 500 });
    const next = first.json?.responsePagination?.next ?? dataOf(first).next?.nextPage;
    if (!next) return `the read did not page: ${JSON.stringify(first.json).slice(0, 200)}`;
    write(dir, 'a.txt', 'changed text\n');
    await call(next.tool, next.input);
    return null;
  } },
  { id: 'code.responseSnapshotExpired', tool: 'localFetch', codes: ['responseSnapshotExpired'], query: { cursor: 'not-issued-here' }, run: async ({ call }) => { await call('localFetch', { cursor: 'matrix-cursor-not-issued' }); return null; } },
  { id: 'code.checkoutDirty', tool: 'ghCloneRepo', codes: ['checkoutDirty'], run: async ({ call }) => {
    const row = { owner: 'octo', repo: 'demo', paths: ['docs'], historyDepth: 1 };
    const location = dataOf(await call('ghCloneRepo', { queries: [row] })).location;
    if (!location?.localPath) return 'no checkout';
    write(location.localPath, 'docs/notes.md', 'local edit\n');
    await call('ghCloneRepo', { queries: [row] });
    return null;
  } },
  code('pathNotFound.clone', 'ghCloneRepo')({ owner: 'octo', repo: 'demo', paths: ['nope'] }, 'pathNotFound'),
  { ...code('persistentStorageDisabled', 'ghCloneRepo')({ owner: 'octo', repo: 'demo' }, 'persistentStorageDisabled'), env: () => ({ OCTOCODE_STORAGE_MODE: 'memory' }) },
  { ...code('gitUnavailable', 'ghCloneRepo')({ owner: 'octo', repo: 'demo' }, 'gitUnavailable'), env: ({ dir }) => ({ PATH: dir }) },
  { ...code('commitMismatch', 'ghCloneRepo')({ owner: 'octo', repo: 'demo' }, 'commitMismatch'), setup: ({ dir }) => fakeGit(dir, `[ "$1 $2" = "rev-parse --verify" ] && { echo ${'f'.repeat(40)}; exit 0; }`), env: ({ dir }) => ({ PATH: `${dir}/bin:${process.env.PATH}` }) },
  { ...code('gitFailed', 'ghCloneRepo')({ owner: 'octo', repo: 'demo' }, 'gitFailed'), setup: ({ dir }) => fakeGit(dir, '[ "$1 $2" = "rev-parse --verify" ] && { echo not-a-sha; exit 0; }'), env: ({ dir }) => ({ PATH: `${dir}/bin:${process.env.PATH}` }) },
];

const providerSide = [
  github(401, 'authentication'),
  github(405, 'httpStatus'),
  github(404, 'notFound'),
  github(403, 'permissionDenied'),
  github(451, 'unavailable'),
  github(429, 'rateLimited', { github: answerRepo({ status: 429, body: { message: 'slow' }, headers: { 'retry-after': '60' } }), channels: ['retry'] }),
  github(500, 'server'),
  code('github.decode', 'ghSearchRepo')({ operation: 'search', keywords: ['x'] }, 'decode', { github: request => (request.p === '/search/repositories' ? { status: 200, body: 'not json at all' } : null) }),
  code('github.redirectDenied', 'ghSearchRepo')({ operation: 'list', owner: 'octo', sort: { by: 'pushedAt', order: 'desc' }, filters: { archive: 'include' } }, 'redirectDenied', { github: request => (request.p === '/users/octo/repos' ? { status: 302, body: '', headers: { location: 'http://example.invalid/elsewhere' } } : null) }),
  { ...code('github.config', 'ghSearchRepo')(OP_GET, 'config'), env: () => ({ GITHUB_API_URL: 'http://example.com/api/v3' }) },
  { ...code('github.transport', 'ghSearchRepo')(OP_GET, 'transport'), env: () => ({ GITHUB_API_URL: 'http://127.0.0.1:9/api/v3' }) },
  code('registry.versionNotFound', 'artifactSearch')({ operation: 'resolve', ecosystem: 'npm', packageName: 'tiny-health', version: '9.9.9', registryUrl: '$MOCK' }, 'versionNotFound'),
  provider('classificationProviderError', 'classificationProviderError', () => ({ status: 400, body: 'plain refusal' })),
  provider('classificationProviderUnavailable', 'classificationProviderUnavailable', () => ({ status: 500, body: { detail: 'down' } })),
  provider('classificationQuotaExhausted', 'classificationQuotaExhausted', () => ({ status: 402, body: { detail: 'quota' } })),
  provider('classificationRateLimited', 'classificationRateLimited', () => ({ status: 429, body: { detail: 'slow' } })),
  provider('classificationStateTooLarge', 'classificationStateTooLarge', () => ({ status: 400, body: { detail: { error_type: 'max_tokens_exceeded' } } })),
  provider('invalidClassificationResponse', 'invalidClassificationResponse', () => ({ status: 200, body: {} })),
  { ...code('invalidClassificationConfiguration', 'clasify')({ operation: 'evaluate', content: 'x', questions: { k: { type: 'noul', instructions: 'x?' } } }, 'invalidClassificationConfiguration'), env: () => ({ OCTOCODE_CLASSIFICATION_API_HOST: 'http://example.com' }) },
  code('classificationContextEmpty', 'clasify')({ operation: 'evaluate', content: '   ', questions: { k: { type: 'noul', instructions: 'x?' } } }, 'classificationContextEmpty'),
  code('classificationContextFailed', 'clasify')({ operation: 'sections', resources: { a: { tool: 'localFetch', query: { path: '$DIR/missing' } }, b: { tool: 'localFetch', query: { path: '/etc/hosts' } } }, questions: { k: { type: 'noul', instructions: 'x?' } } }, 'classificationContextFailed'),
  code('invalidClassificationContext', 'clasify')({ operation: 'evaluate', content: { image: 'k' }, assets: { k: { source: { tool: 'localFetch', query: { path: '$DIR/not-image.png' } } } }, questions: { k: { type: 'noul', instructions: 'x?' } } }, 'invalidClassificationContext', { files: { 'not-image.png': 'plain text, no image\n' } }),
  code('securityValidationFailed', 'clasify')({ operation: 'evaluate', source: { url: 'https://127.0.0.1:1/x' }, questions: { k: { type: 'noul', instructions: 'x?' } } }, 'securityValidationFailed'),
];

const write = (dir, file, text) => FS.writeFileSync(PATH.join(dir, file), text);
const chmod = (dir, file, mode) => FS.chmodSync(PATH.join(dir, file), mode);
function fakeGit(dir, rule) {
  FS.mkdirSync(PATH.join(dir, 'bin'), { recursive: true });
  const real = spawnSync('which', ['git'], { encoding: 'utf8' }).stdout.trim();
  FS.writeFileSync(PATH.join(dir, 'bin/git'), `#!/bin/sh\n${rule}\nexec ${real} "$@"\n`);
  FS.chmodSync(PATH.join(dir, 'bin/git'), 0o755);
}

export const CODE_SCENARIOS = [...local, ...providerSide];
