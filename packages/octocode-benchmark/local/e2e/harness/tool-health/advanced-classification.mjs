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
  checks.push(['literal.context', same(contextPacket.state, contextual.content) && JSON.stringify(contextPacket.questions).includes(context) && JSON.stringify(contextPacket.questions).includes('Is contextEvidence present?'), contextPacket]);
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

async function clasifyChecks(ctx) {
  const checks = [];
  for (const surface of ctx.surfaces) {
    const provider = providerSession(ctx, 'clasify', surface);
    try {
      for (const run of [literalChecks, providerChecks, sourceChecks, imageChecks, githubImageChecks, urlPolicyChecks, answerPages, sectionChecks, unreadSectionsChecks, memorySectionsChecks]) {
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
