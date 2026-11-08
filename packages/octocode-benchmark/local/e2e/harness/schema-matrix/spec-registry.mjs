// artifactSearch, securityCheck, clasify and ghCloneRepo on the loopback providers.
// Each ecosystem's known package comes from the registry proxy's fixture; the answers
// a question type owes come from the mock classifier's stated behavior.
import fs from 'node:fs';
import path from 'node:path';
import { uniquePredicate } from './classification-rows.mjs';

const dataOf = pages => pages.flatMap(page => page.json?.results ?? []).map(row => row.data ?? {});
const PLACEHOLDER_URL = 'http://127.0.0.1:1';
const COORDINATE = { maven: 'org.proof:proof', go: 'example.test/proof', packagist: 'proof/proof' };
const KNOWN_VERSION = { go: 'v1.2.3' };

export const artifactSearch = {
  tool: 'artifactSearch',
  base: row => {
    const ecosystem = row.ecosystem ?? 'npm';
    if (row.operation === 'search') return ecosystem === 'npm' ? { keywords: ['health'], registryUrl: PLACEHOLDER_URL } : { keywords: ['proof'] };
    return ecosystem === 'npm' ? { packageName: 'tiny-health', registryUrl: PLACEHOLDER_URL } : { packageName: COORDINATE[ecosystem] ?? 'proof' };
  },
  resolve: (ctx, input) => JSON.parse(JSON.stringify(input).replaceAll(PLACEHOLDER_URL, ctx.mock.url)),
  truth: (ctx, { query }) => {
    const ecosystem = query.ecosystem ?? 'npm';
    if (query.operation === 'resolve') return [[ecosystem === 'npm' ? 'tiny-health' : COORDINATE[ecosystem] ?? 'proof', ecosystem === 'npm' ? '1.2.3' : KNOWN_VERSION[ecosystem] ?? '1.2.3']];
    return null;
  },
  observed: (ctx, testCase, pages) => dataOf(pages).flatMap(row => row.artifacts ?? []).map(item => [item.packageName, item.version]),
  judge: (worker, { testCase, pages }) => {
    if (testCase.query.operation !== 'search') return null;
    const names = dataOf(pages).flatMap(row => row.artifacts ?? []).map(item => item.packageName);
    const owed = testCase.query.ecosystem === 'npm' ? 'tiny-health' : COORDINATE[testCase.query.ecosystem]?.replace(/proof$/, 'proof-00') ?? 'proof-00';
    return names.includes(owed) && new Set(names).size === names.length ? null : `search misses ${owed} or repeats a row: ${names.slice(0, 5).join(', ')}`;
  },
  indistinct: { operation: 'one value per form', ecosystem: 'each ecosystem has its own coordinate and oracle' },
};

const OSV_ROW = { package: ecosystem => ({ ecosystem, packageName: COORDINATE[ecosystem] ?? 'proof', version: KNOWN_VERSION[ecosystem] ?? '1.2.3' }) };

export const securityCheck = {
  tool: 'securityCheck',
  base: row => {
    if (row.operation === 'advisory') return { id: 'GHSA-happy' };
    if (row.operation === 'commit') return { commit: '6879efc2c1596d11a6a6ad296f80063b558d5e0f' };
    if (row.operation === 'url') return { url: 'pkg:pypi/proof@1.0.0' };
    if (row.operation === 'repository') return { repo: 'octo/demo' };
    return OSV_ROW.package(row.ecosystem ?? 'npm');
  },
  judge: (worker, { testCase, pages }) => {
    const row = dataOf(pages)[0] ?? {};
    if (testCase.query.operation === 'advisory') return row.advisories?.[0]?.id === 'GHSA-happy' ? null : `advisory ${JSON.stringify(row).slice(0, 160)}`;
    return 'answer' in row ? null : `no answer sentence: ${JSON.stringify(row).slice(0, 160)}`;
  },
  indistinct: { operation: 'one value per form', ecosystem: 'each ecosystem has its own coordinate' },
};

const local = { tool: 'localFetch', query: { path: 'matrix-world/plain/src/alpha.ts' } };
const remote = { tool: 'ghGetFileContent', query: { owner: 'octo', repo: 'matrix', path: 'src/alpha.ts' } };
const questionOf = type => ({
  noul: { type, instructions: 'Is this code?' },
  choice: { type, criteria: { code: 'source code', other: 'something else' }, instructions: 'Which kind?' },
  score: { type, criteria: ['low', 'high'], instructions: 'How much?' },
  box: { type, instructions: 'Where?' },
})[type];

function sectionsBase(row, questions) {
  if (row.assessment !== undefined) return {};
  return { questions, resources: { k: (row.resources?.k?.tool ?? 'localFetch') === 'localFetch' ? local : remote } };
}

function imageBase(row, questions, assignment) {
  const asset = row.assets?.k?.source?.tool === 'ghGetFileContent' ? { ...remote, query: { ...remote.query, path: 'pixel.png', ref: 'a'.repeat(40) } } : { tool: 'localFetch', query: { path: 'matrix-world/pixel.png' } };
  return { questions, model: 'jev-1.14.0', assets: { k: { source: asset } }, content: { image: 'k', note: uniquePredicate(assignment) } };
}

function clasifyBase(row, assignment) {
  if (row.operation === 'models') return {};
  const type = row.questions?.k?.type ?? 'noul';
  const questions = { k: questionOf(type) };
  if (row.operation === 'sections') return sectionsBase(row, questions);
  const needsImage = type === 'box' || row.assets;
  if (needsImage) return row.source ? null : imageBase(row, questions, assignment);
  if (!row.source?.tool) return { questions, content: `state ${uniquePredicate(assignment)}` };
  return { questions, source: row.source.tool === 'localFetch' ? local : remote };
}

function clasifyJudge(worker, { testCase, pages }) {
  const { query } = testCase;
  const row = dataOf(pages)[0] ?? {};
  if (query.operation === 'models') return row.models?.some(model => model.name === 'jev-latest') ? null : `models ${JSON.stringify(row).slice(0, 160)}`;
  if (query.operation === 'sections') return row.resources?.k?.status === 'ready' || row.sections ? null : `sections ${JSON.stringify(row).slice(0, 200)}`;
  const answer = row.answers?.k;
  return answer?.type === query.questions.k.type ? null : `answer ${JSON.stringify(row).slice(0, 200)}`;
}

export const clasify = {
  tool: 'clasify',
  base: clasifyBase,
  shape: query => {
    if (query.questions?.k) query.questions.k = { ...questionOf(query.questions.k.type) };
    return query;
  },
  judge: clasifyJudge,
  indistinct: { operation: 'one value per form', longContext: 'changes the provider request', 'questions.*.type': 'each type has its own answer shape' },
};

export const ghCloneRepo = {
  tool: 'ghCloneRepo',
  base: () => ({ owner: 'octo', repo: 'demo', paths: ['src'], historyDepth: 1 }),
  judge: (worker, { pages }) => {
    const location = dataOf(pages)[0]?.location;
    const file = location?.localPath && path.join(location.localPath, 'src/greeter.ts');
    return file && fs.existsSync(file) ? null : `clone holds no source: ${JSON.stringify(location)}`;
  },
};
