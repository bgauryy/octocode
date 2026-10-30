import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { sourceLines } from '../responses.mjs';
import { classificationAnswer } from './mock-routes.mjs';
import { data, display, noul, PNG, providerSession, readCaptured, same } from './classification-checks.mjs';
import { sectionChecks, unreadSectionsChecks, memorySectionsChecks } from './classification-sections.mjs';
import { inlineChecks } from './classification-inline.mjs';
import { githubImageChecks, urlPolicyChecks } from './classification-network.mjs';

async function literalChecks(ctx, surface, provider) {
  const checks = [], call = query => ctx.call(ctx.q(query), surface);
  const models = data(await call({ operation: 'models' }));
  assert.ok(models.models, JSON.stringify(models));
  checks.push(['models', models.models.some(model => model.name === 'jev-1.14.0') && models.models.filter(model => model.default).length === 1, models]);
  const questions = {
    fact: { type: 'noul', criteria: { true: { field: 'enabled' }, false: null } },
    route: { type: 'choice', instructions: null, criteria: { allowed: 'enabled', unknown: null } },
    strength: { type: 'score', instructions: ['Inspect evidence'], criteria: ['absent', 'partial', { level: 'direct' }] },
  };
  checks.push(...await literalValues(call, provider, questions));
  const context = 'Use only the supplied evidence for this contextual question.';
  const contextual = { operation: 'evaluate', content: { contextEvidence: true }, context, questions: { fact: noul('Is contextEvidence present?') } };
  await call(contextual);
  const contextPacket = provider.sent.at(-1)?.request;
  checks.push(['literal.context', same(contextPacket.state, { context, content: contextual.content }) && same(Object.values(contextPacket.questions), Object.values(contextual.questions)), contextPacket]);
  const invalidBefore = provider.sent.length;
  for (const query of [
    { operation: 'evaluate', content: {}, questions: { bounds: { type: 'box' } } },
    { operation: 'evaluate', content: {}, source: { tool: 'localFetch', query: { path: 'src/app.ts' } }, questions },
    { operation: 'evaluate', content: {}, questions: { missing: { type: 'noul', instructions: ' ' } } },
  ]) {
    const result = await call(query);
    checks.push(['literal.rejectBeforeProvider', result.json?.errorCode === 'invalidInput' && provider.sent.length === invalidBefore, result.json]);
  }
  return checks;
}

async function literalValues(call, provider, questions) {
  const checks = [];
  for (const content of ['Literal evidence', { enabled: true, nested: [null, 4] }, ['evidence', { enabled: true }]]) {
    const query = { operation: 'evaluate', content, questions, model: 'jev-1.14.0', longContext: true };
    const before = provider.sent.length;
    const first = data(await call(query));
    const packet = provider.sent.at(-1)?.request;
    assert.ok(first.answers && packet, JSON.stringify(first));
    checks.push(['literal.wire', provider.sent.length === before + 1 && same(packet.state, content) && same(Object.values(packet.questions), Object.values(questions)) && provider.sent.at(-1).headers['typesafe-danger-zone-long-context'] === 'true' && packet.model === query.model, packet]);
    checks.push(['literal.typedAnswers', first.answers.fact.noul === 0.9 && first.answers.route.choice === 'allowed' && first.answers.strength.score === 2 && same(first.answers.strength.legend['2'], questions.strength.criteria[2]), first]);
    const cached = data(await call(query));
    checks.push(['literal.cache', provider.sent.length === before + 1 && cached.replayed === true && same(cached.answers, first.answers), cached]);
  }
  return checks;
}

const providerQuery = content => ({ operation: 'evaluate', content, questions: { fact: noul('Is the evidence explicit?') } });

async function providerChecks(ctx, surface, provider) {
  const checks = [];
  let attempts = 0;
  provider.set(request => ++attempts === 1 ? { status: 429, body: { error: 'temporary provider limit' } } : { body: classificationAnswer(request) });
  const retried = data(await ctx.call(ctx.q(providerQuery('retry evidence')), surface));
  const packets = provider.sent.slice(-2).map(entry => entry.request);
  checks.push(['provider.retry', attempts === 2 && retried?.answers?.fact?.noul === 0.9 && same(packets[0], packets[1]), { attempts, packets }]);
  for (const { label, response, code } of [
    { label: 'refusal', response: { status: 401, body: { error: 'fixture refused' } }, code: 'classificationProviderError' },
    { label: 'malformed', response: { body: { model: 'jev-latest', answers: {}, usage: { input_tokens: 0, output_tokens: 0 } } }, code: 'invalidClassificationResponse' },
  ]) {
    provider.set(() => response);
    const before = provider.sent.length;
    const failed = data(await ctx.call(ctx.q(providerQuery(`${label} evidence`)), surface));
    checks.push([`provider.${label}`, provider.sent.length === before + 1 && failed?.errorCode === code && !failed.answers, failed]);
  }
  provider.set(request => ({ body: classificationAnswer(request) }));
  return checks;
}

async function sourceChecks(ctx, surface, provider) {
  const file = path.join(ctx.cwd, `capture-${surface}.txt`);
  const original = 'outside first\nselected α\nselected 😀\noutside last\n';
  fs.writeFileSync(file, original);
  const source = { tool: 'localFetch', query: { path: file, ranges: [{ startLine: 2, endLine: 3 }] } };
  const questions = { evidence: noul('Does this contain selected evidence?') };
  const result = data(await ctx.call(ctx.q({ operation: 'evaluate', source, questions }), surface));
  const packet = provider.sent.at(-1)?.request;
  const read = await readCaptured(ctx, surface, result.next.read);
  const checks = [['source.ranges', JSON.stringify(packet.state).includes('selected α') && !JSON.stringify(packet.state).includes('outside') && same(read.flatMap(row => sourceLines(row)), [[2, 'selected α'], [3, 'selected 😀']]), packet]];
  fs.writeFileSync(file, 'replacement source\n');
  const stale = data(await ctx.callOn(surface)('localFetch', result.next.read.input));
  checks.push(['source.stale', stale?.errorCode === 'staleSnapshot' && !stale.content, stale]);
  const github = { tool: 'ghGetFileContent', query: { owner: 'octo', repo: 'demo', ref: 'main', path: 'src/greeter.ts', ranges: [{ startLine: 2, endLine: 4 }] } };
  const remote = data(await ctx.call(ctx.q({ operation: 'evaluate', source: github, questions }), surface));
  const before = provider.sent.length;
  const captured = await readCaptured(ctx, surface, remote.next.read);
  checks.push(['source.github', provider.sent.length === before && same(captured.flatMap(row => sourceLines(row)), [[2, 'export interface Greeting {'], [3, '  text: string;'], [4, '}']]), captured]);
  fs.rmSync(file);
  return checks;
}

async function imageChecks(ctx, surface, provider) {
  const file = path.join(ctx.cwd, `image-${surface}.png`);
  fs.writeFileSync(file, PNG);
  const query = { operation: 'evaluate', model: 'jev-1.14.0', content: { image: 'screen' }, assets: { screen: { source: { tool: 'localFetch', query: { path: file } } } }, questions: { object: { type: 'box', instructions: 'Locate the object in screen.' }, visible: noul('Is screen visible?') } };
  const first = data(await ctx.call(ctx.q(query), surface));
  const asset = first?.assets?.screen, packet = provider.sent.at(-1);
  assert.ok(asset && first.answers && packet, JSON.stringify(first));
  const checks = [['image.bytes', packet.request.assets.screen.data_base64 === PNG.toString('base64') && asset.width === 280 && asset.height === 280 && fs.readFileSync(asset.path).equals(PNG), asset],
    ['image.box', same(first.answers.object.box, { top: 0.1, bottom: 0.9, left: 0.2, right: 0.8 }) && first.answers.object.confidence === null && !JSON.stringify(first).includes(PNG.toString('base64')), first.answers]];
  const before = provider.sent.length;
  const repeated = data(await ctx.call(ctx.q(query), surface));
  checks.push(['image.cache', provider.sent.length === before && repeated.replayed === true && repeated.assets.screen.snapshot === asset.snapshot, repeated.assets]);
  fs.appendFileSync(file, Buffer.from([0]));
  const changed = data(await ctx.call(ctx.q(query), surface));
  checks.push(['image.mutation', provider.sent.length === before + 1 && changed.assets.screen.snapshot !== asset.snapshot && fs.readFileSync(asset.path).equals(PNG), changed.assets]);
  fs.rmSync(file);
  return checks;
}

async function answerPages(ctx, surface, provider) {
  const criteria = Object.fromEntries(Array.from({ length: 80 }, (_, i) => [`option-${i}-${'evidence'.repeat(4)}`, `Meaning ${i}`]));
  const questions = Object.fromEntries(Array.from({ length: 12 }, (_, i) => [`answer${i}`, { type: 'choice', instructions: `Choose for dimension ${i}`, criteria }]));
  const query = { operation: 'evaluate', content: { large: 'independent choices' }, questions };
  const before = provider.sent.length;
  const pages = await display(ctx, surface, 'clasify', { queries: [query], responseChars: 3000 });
  const entries = pages.rows.flatMap(row => Object.entries(row.answers ?? {}));
  const expected = classificationAnswer({ questions }).answers;
  const packets = provider.sent.slice(before);
  const judged = packets.flatMap(entry => Object.values(entry.request.questions));
  const sentOnce = judged.length === 12 && Object.values(questions).every(question => judged.filter(value => same(value, question)).length === 1);
  const after = provider.sent.length;
  const checks = [['answers.completeWalk', pages.results.length > 1 && entries.length === 12 && new Set(entries.map(([key]) => key)).size === 12 && entries.every(([key, answer]) => same(answer, expected[key])) && sentOnce, { pages: pages.results.length, answers: entries.length, providerCalls: packets.length }]];
  const next = pages.results[0].json.responsePagination.next;
  const replay = await display(ctx, surface, 'clasify', next.input);
  checks.push(['answers.savedReplay', provider.sent.length === after && same(replay.rows.flatMap(row => Object.entries(row.answers ?? {})), pages.rows.slice(1).flatMap(row => Object.entries(row.answers ?? {}))), { pages: replay.results.length }]);
  return checks;
}

function batchAnswer(request) {
  const body = classificationAnswer(request);
  if (request.state?.batch === 'left' || request.state?.batch === 'right') {
    for (const answer of Object.values(body.answers)) answer.noul = request.state.batch === 'left' ? 0.1 : 0.8;
  }
  return { body };
}

const batchQuery = batch => ({ operation: 'evaluate', content: { batch }, questions: { sameKey: noul('Is the batch value left?') } });

async function batchChecks(ctx, surface, provider) {
  const file = path.join(ctx.cwd, `batch-sections-${surface}.rs`);
  const source = Array.from({ length: 8 }, (_, i) => `pub fn batch_${i}() { println!("batch ${i}"); }\n`).join('');
  const starts = [0, ...Array.from(source.matchAll(/\n/g), match => Buffer.byteLength(source.slice(0, match.index + 1)))];
  fs.writeFileSync(file, source);
  const queries = [
    { ...batchQuery('invalid'), questions: {} }, batchQuery('left'),
    { operation: 'sections', resources: { picked: { tool: 'localFetch', query: { path: file, ranges: [{ startLine: 2, endLine: 3 }, { startLine: 7, endLine: 8 }] } } }, questions: { evidence: noul('Does the section print its batch number?') } },
    { operation: 'evaluate', source: { tool: 'localFetch', query: { path: path.join(ctx.cwd, 'missing-batch-source.txt') } }, questions: { sameKey: noul('Is the source available?') } },
    batchQuery('right'),
  ];
  const before = provider.sent.length;
  provider.set(batchAnswer);
  try {
    const report = await ctx.explore('clasify', { queries, responseChars: 700 }, surface);
    const rows = report.responses.filter(response => response.tool === 'clasify').flatMap(response => response.result.json?.results ?? []);
    const at = index => rows.filter(row => row.index === index);
    const packets = provider.sent.slice(before);
    const judged = packets.filter(packet => packet.request.state?.batch).map(packet => packet.request.state.batch).toSorted();
    const checks = [['batch.isolation', report.failures.length === 0 && same(judged, ['left', 'right'])
      && batchEvidence(at), { failures: report.failures, judged, rows }]];
    const sections = at(2).flatMap(row => row.data.resources?.picked?.sections ?? []);
    const lines = sections.filter(section => section.answers.evidence?.status === 'answered').flatMap(section => Array.from({ length: section.target.lines.endLine - section.target.lines.startLine + 1 }, (_, index) => section.target.lines.startLine + index)).toSorted((a, b) => a - b);
    checks.push(['batch.sectionsCoordinates', same(lines, [2, 3, 7, 8]) && sections.every(section => same(section.target.bytes, [starts[section.target.lines.startLine - 1], starts[section.target.lines.endLine]])), sections]);
    return checks;
  } finally {
    provider.set(request => ({ body: classificationAnswer(request) }));
    fs.rmSync(file);
  }
}

function batchEvidence(at) {
  const answer = index => at(index).flatMap(row => Object.entries(row.data.answers ?? {}));
  const failure = (index, code) => at(index).length === 1 && at(index)[0].data.errorCode === code;
  return failure(0, 'invalidInput') && failure(3, 'pathNotFound')
    && same(answer(1), [['sameKey', { type: 'noul', noul: 0.1 }]])
    && same(answer(4), [['sameKey', { type: 'noul', noul: 0.8 }]]);
}

async function boundaryQuestions(ctx, surface, provider) {
  const questions = Object.fromEntries(Array.from({ length: 25 }, (_, index) => [`q${index}`, noul(`Is question ${index} present?`)]));
  const before = provider.sent.length;
  const pages = await display(ctx, surface, 'clasify', { queries: [{ operation: 'evaluate', content: 'Question limit evidence', questions }], responseChars: 1 });
  const answers = pages.rows.flatMap(row => Object.entries(row.answers ?? {}));
  const sent = provider.sent.slice(before).flatMap(packet => Object.values(packet.request.questions));
  return [['questions.boundaryComplete', same(answers.map(([key]) => key).toSorted(), Object.keys(questions).toSorted())
    && answers.every(([, answer]) => answer.type === 'noul' && answer.noul === 0.9)
    && sent.length === 25 && Object.values(questions).every(question => sent.filter(value => same(value, question)).length === 1),
  { pages: pages.results.length, answers: answers.length, questionsSent: sent.length }]];
}

function invalidShapes() {
  const row = providerQuery('Rejected field evidence');
  const source = { tool: 'localFetch', query: { path: 'src/app.ts' } };
  return Object.entries({
    scalarContent: { ...row, content: true }, matchString: { ...row, matchString: ['evidence'] }, minify: { ...row, minify: true },
    sourceMatcher: { operation: 'evaluate', source: { ...source, query: { ...source.query, matchString: ['export'] } }, questions: row.questions },
    sourceMinify: { operation: 'evaluate', source: { ...source, query: { ...source.query, minify: true } }, questions: row.questions },
    assetRange: { ...row, assets: { image: { source: { ...source, query: { ...source.query, ranges: [{ startLine: 1, endLine: 2 }] } } } } },
    privateAssessment: { operation: 'sections', assessment: 'not-issued' },
    tooManyQuestions: { ...row, questions: Object.fromEntries(Array.from({ length: 26 }, (_, i) => [`q${i}`, noul('Is evidence present?')])) },
    emptyChoice: { ...row, questions: { choice: { type: 'choice', criteria: {} } } },
    emptyScore: { ...row, questions: { score: { type: 'score', criteria: [] } } },
    scoreOverflow: { ...row, questions: { score: { type: 'score', criteria: Array(11).fill('level') } } },
  });
}

async function admissionChecks(ctx, surface, provider) {
  const checks = [], before = provider.sent.length;
  for (const [label, query] of invalidShapes()) {
    const input = ctx.q(query), result = await ctx.call(input, surface);
    const errorCode = result.json?.errorCode ?? data(result)?.errorCode;
    checks.push([`admission.${label}`, ctx.validateInput('clasify', input) !== null && errorCode === 'invalidInput'
      && provider.sent.length === before, result.json]);
  }
  return checks;
}

async function clasifyChecks(ctx) {
  const checks = [];
  for (const surface of ctx.surfaces) {
    const provider = providerSession(ctx, 'clasify', surface);
    try {
      for (const run of [literalChecks, providerChecks, sourceChecks, imageChecks, githubImageChecks, urlPolicyChecks, answerPages, sectionChecks, unreadSectionsChecks, memorySectionsChecks, batchChecks, boundaryQuestions, admissionChecks]) {
        for (const [label, ok, detail] of await run(ctx, surface, provider)) checks.push([`${surface}.${label}`, ok, JSON.stringify(detail)]);
      }
    } finally { provider.close(); }
  }
  return checks;
}

export const ADVANCED_CLASSIFICATION = {
  clasify: clasifyChecks,
  ...Object.fromEntries(['localSearch', 'localFetch', 'ghSearchCode', 'ghGetFileContent'].map(tool => [tool, ctx => inlineChecks(ctx, tool)])),
};
