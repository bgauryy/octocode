import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { sourceLines } from '../responses.mjs';
import { data, noul, readCaptured, same } from './classification-checks.mjs';

const sectionSource = () => Array.from({ length: 70 }, (_, i) => `pub fn section_${i}() { println!("évidence λ ${i} ${'evidence '.repeat(600)}"); }\n`).join('');

export async function sectionChecks(ctx, surface, provider) {
  const file = path.join(ctx.cwd, `sections-${surface}.rs`);
  const source = sectionSource();
  fs.writeFileSync(file, source);
  const questions = { explicit: noul('Does the target print evidence?'), strength: { type: 'score', criteria: ['absent', 'direct'] } };
  const resources = {
    local: { tool: 'localFetch', query: { path: file } },
    github: { tool: 'ghGetFileContent', query: { owner: 'octo', repo: 'demo', ref: 'main', path: 'src/app.ts' } },
    missing: { tool: 'localFetch', query: { path: path.join(ctx.cwd, 'absent-sections.txt') } },
  };
  const report = await ctx.explore('clasify', ctx.q({ operation: 'sections', resources, questions }), surface);
  const pages = report.responses.filter(entry => entry.tool === 'clasify').flatMap(entry => entry.result.json?.results ?? []).map(row => row.data);
  const collected = collectSections(pages, source);
  const { byId, coordinates, cells } = collected;
  const last = pages.at(-1).resources.local;
  assert.ok(last, JSON.stringify(pages.at(-1)));
  const checks = [
    ['sections.complete', report.failures.length === 0 && completeSections(collected, last.coverage), { failures: report.failures, sections: byId.size, cells: [...cells.values()], coverage: last.coverage }],
    ['sections.coordinates', coordinates.length >= 70 && coordinates.every(item => item.ok), { checked: coordinates.length, mismatches: coordinates.filter(item => !item.ok).slice(0, 3) }],
    ['sections.mixedResources', pages.some(row => row.resources?.github?.status === 'ready') && pages.some(row => row.resources?.missing?.status === 'error') && pages.some(row => same(row.questions, questions)), pages[0]?.resources?.missing],
  ];
  const first = pages[0], before = provider.sent.length;
  const captured = await readCaptured(ctx, surface, first.resources.local.read);
  checks.push(['sections.exactRead', provider.sent.length === before && same(captured.flatMap(row => sourceLines(row)), source.trimEnd().split('\n').map((line, i) => [i + 1, line])), captured.length]);
  const continuation = pages.find(row => row.next?.nextPage)?.next.nextPage;
  assert.ok(continuation, 'section fixture must require assessment continuation');
  const original = report.responses.find(entry => entry.tool === 'clasify' && same(entry.input, continuation.input));
  assert.ok(original, 'issued assessment continuation must have been executed');
  const expected = data(original.result).resources.local.sections;
  const replay = data(await ctx.call(continuation.input, surface));
  checks.push(['sections.frozenReplay', provider.sent.length === before && replay.replayed === true && expected.length > 0 && same(replay.resources.local.sections, expected), replay.resources.local.coverage]);
  fs.rmSync(file);
  return checks;
}

function collectSections(pages, source) {
  const byId = new Map(), coordinates = [], lines = new Set(), cells = new Map();
  let offset = 0;
  const spans = (source.match(/[^\n]*\n|[^\n]+$/g) ?? []).map(line => {
    const span = [offset, offset + Buffer.byteLength(line)];
    offset = span[1];
    return span;
  });
  for (const row of pages) {
    for (const section of row.resources?.local?.sections ?? []) {
      const previous = byId.get(section.id) ?? {};
      byId.set(section.id, { ...previous, ...section.answers });
      const line = section.target.lines.startLine;
      lines.add(line);
      const expected = spans[line - 1];
      coordinates.push({ id: section.id, actual: section.target, expected,
        ok: section.target.lines.endLine === line && same(section.target.bytes, expected) });
      for (const [question, answer] of Object.entries(section.answers)) {
        if (answer.status !== 'answered') continue;
        const key = `${section.id}:${question}`;
        cells.set(key, (cells.get(key) ?? 0) + 1);
      }
    }
  }
  return { byId, coordinates, lines, cells };
}

const answered = answers => answers.explicit?.status === 'answered' && answers.explicit.answer.noul === 0.9 && answers.strength?.answer?.score === 1;

function completeSections({ byId, lines, cells }, coverage) {
  return byId.size === 70 && lines.size === 70 && cells.size === 140
    && [...cells.values()].every(count => count === 1) && [...byId.values()].every(answered)
    && (coverage.remainingSections ?? 0) === 0 && coverage.answered === 140;
}

export async function unreadSectionsChecks(ctx, surface, provider) {
  const missing = { operation: 'sections', resources: { missing: { tool: 'localFetch', query: { path: path.join(ctx.cwd, 'absent-section-resource.rs') } } }, questions: { evidence: noul('Is source evidence available?') } };
  const checks = [], before = provider.sent.length;
  for (const queries of [[missing], [{ operation: 'models' }, missing], [missing, { operation: 'models' }]]) {
    const response = await ctx.call({ queries }, surface);
    const index = queries.indexOf(missing);
    const rows = response.json?.results ?? [];
    const failed = rows.find(row => row.index === index);
    const success = rows.filter(row => row.index !== index);
    const shape = failed?.status === 'error' && failed.data.errorCode === 'pathNotFound'
      && typeof failed.data.error === 'string' && failed.data.resources.missing.error.errorCode === 'pathNotFound';
    const status = surface === 'cli' ? response.status === 3 : response.isError === (queries.length === 1);
    checks.push([`sections.unreadBatch.${queries.length}.${index}`, shape && status && rows.length === queries.length
      && success.every(row => row.data.models?.length > 0) && provider.sent.length === before, response.json]);
  }
  return checks;
}

/** A memory-storage refusal of a section plan: nothing read, no provider called, no continuation. */
function memoryRefused(row) {
  if (row?.status !== 'error' || row.data?.errorCode !== 'classificationContextUnavailable') return false;
  const { error = '', hints = [], usage } = row.data;
  return error.includes('no source was read') && error.includes('no classification provider was called')
    && hints.some(hint => hint.includes('storage.mode persistent') && hint.includes('MCP'))
    && (usage?.calls ?? 0) === 0 && !JSON.stringify(row).includes('"kind":"continue"');
}

/** A source evaluation answered under memory storage: its only read is a `related` call that a later runtime runs. */
function memoryAnswered(row, file) {
  if (row?.status || row?.data?.answers?.evidence?.noul !== 0.9 || row.data.next) return false;
  const reads = (row.data.related ?? []).filter(call => call.tool === 'localFetch');
  return reads.length === 1 && reads[0].input?.queries?.[0]?.path === file && !JSON.stringify(row).includes('cursor');
}

/** `queries` on the CLI under memory storage, with how many provider calls it made. */
async function memoryCall(ctx, provider, queries) {
  const before = provider.sent.length;
  const response = await ctx.cliCall('clasify', { queries, responseChars: 50_000 }, { env: { OCTOCODE_STORAGE_MODE: 'memory' } });
  return { json: response.json, status: response.status, rows: response.json?.results ?? [], providerCalls: provider.sent.length - before };
}

const unpaged = json => json?.responsePagination?.hasMore !== true;

/** One section plan refused under memory storage, with no provider call. */
const refusedAlone = run => run.status !== 0 && run.rows.length === 1 && memoryRefused(run.rows[0]) && unpaged(run.json) && run.providerCalls === 0;

/** One source evaluation answered under memory storage with one provider call. */
const answeredAlone = (run, file) => run.status === 0 && run.rows.length === 1 && memoryAnswered(run.rows[0], file) && unpaged(run.json) && run.providerCalls === 1;

/** The row's disclosure that its fresh read is larger than one response, so it pages and needs saved continuations. */
const pagedReadHint = row => (row?.data?.hints ?? []).some(hint => hint.includes('larger than one response')
  && hint.includes('storage.mode persistent') && hint.includes('MCP'));

/**
 * The offered read runs in the next memory CLI call. A read that fits one
 * response returns the file's text, and the row carries no paging hint. A
 * read larger than one response needs continuations, which a memory CLI call
 * refuses like any other paged read: the row said so beside the read. The
 * hint is there exactly when the read cannot complete.
 */
async function freshReadRuns(ctx, run, marker, env) {
  const row = run.rows[0];
  const call = row?.data?.related?.find(related => related.tool === 'localFetch');
  if (!call) return { ok: false, response: run.json };
  const response = await ctx.cliCall(call.tool, call.input, { credentialTool: 'clasify', env });
  const read = response.status === 0 && JSON.stringify(response.json ?? {}).includes(marker);
  const hinted = pagedReadHint(row);
  return { ok: hinted ? response.status !== 0 && !read : read, response: { hinted, read: response.json } };
}

/** A refused plan beside a literal and a source evaluation: both evaluations reach the provider once each. */
function refusedBesideAnswers(run, file) {
  const [plan, literal, source] = run.rows;
  return run.rows.length === 3 && memoryRefused(plan) && !literal?.status && literal?.data.answers?.evidence?.noul === 0.9
    && memoryAnswered(source, file) && unpaged(run.json) && run.providerCalls === 2;
}

export async function memorySectionsChecks(ctx, surface, provider) {
  if (surface !== 'cli') return [];
  const file = path.join(ctx.cwd, 'memory-section-resource.rs');
  const questions = { evidence: noul('Is source evidence available?') };
  const source = { tool: 'localFetch', query: { path: file } };
  const section = { operation: 'sections', resources: { local: source }, questions };
  const evaluate = { operation: 'evaluate', source, questions };
  const literal = { operation: 'evaluate', content: 'There is literal evidence here.', questions };
  const checks = [];
  try {
    const memory = { OCTOCODE_STORAGE_MODE: 'memory' };
    for (const [name, text] of [['simple', 'pub fn evidence() {}\n'], ['seventy', sectionSource()]]) {
      fs.writeFileSync(file, text);
      const plan = await memoryCall(ctx, provider, [section]);
      checks.push([`sections.memoryRefuses.${name}.sections`, refusedAlone(plan), { response: plan.json, providerCalls: plan.providerCalls }]);
      const answer = await memoryCall(ctx, provider, [evaluate]);
      checks.push([`sections.memoryAnswers.${name}.evaluate`, answeredAlone(answer, file), { response: answer.json, providerCalls: answer.providerCalls }]);
      const read = await freshReadRuns(ctx, answer, text.slice(0, text.indexOf('(')), memory);
      checks.push([`sections.memoryAnswers.${name}.freshRead.memory`, read.ok, read.response]);
    }
    const mixed = await memoryCall(ctx, provider, [section, literal, evaluate]);
    checks.push(['sections.memoryMixedLiteral', refusedBesideAnswers(mixed, file), { response: mixed.json, providerCalls: mixed.providerCalls }]);
    return checks;
  } finally { fs.rmSync(file); }
}
