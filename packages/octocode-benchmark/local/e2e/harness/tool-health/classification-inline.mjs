import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { sourceLines } from '../responses.mjs';
import { classificationAnswer } from './mock-routes.mjs';
import { data, providerSession, rowData, same } from './classification-checks.mjs';

const marker = kind => `classification_evidence ${kind} ${'bounded evidence '.repeat(5)}`;

function fixture(ctx, tool) {
  if (tool === 'ghSearchCode') return { query: { owner: 'octo', repo: 'demo', keywords: ['greet'] }, decide: state => state.evidence?.path === 'src/greeter.ts' ? 'selected' : state.evidence?.path === 'src/app.ts' ? 'unknown' : 'excluded' };
  if (tool === 'ghGetFileContent') return { query: { owner: 'octo', repo: 'demo-docs', path: 'guide.txt' }, decide: state => state.sourceLineRanges?.[0]?.[0] <= 100 ? 'selected' : state.sourceLineRanges?.[0]?.[0] <= 300 ? 'unknown' : 'excluded' };
  const directory = path.join(ctx.cwd, 'advanced-selection');
  fs.mkdirSync(directory, { recursive: true });
  if (tool === 'localSearch') {
    for (const kind of ['selected', 'unknown', 'excluded']) fs.writeFileSync(path.join(directory, `${kind}.txt`), `${marker(kind)}\n`);
    return { query: { path: directory, matchString: ['classification_evidence'], contextLines: { before: 0, after: 0 } }, decide: state => path.basename(state.path, '.txt') };
  }
  const file = path.join(directory, 'source.txt');
  fs.writeFileSync(file, Array.from({ length: 600 }, (_, i) => `${marker(i < 100 ? 'selected' : i < 300 ? 'unknown' : 'excluded')} ${i + 1}\n`).join(''));
  return { query: { path: file }, decide: state => state.sourceLineRanges?.[0]?.[0] <= 100 ? 'selected' : state.sourceLineRanges?.[0]?.[0] <= 300 ? 'unknown' : 'excluded' };
}

function selectedAnswer(question, decision) {
  const levels = question.criteria;
  const probabilities = Object.fromEntries(levels.map((_, i) => [String(i), decision === 'unknown' ? (i < 2 ? 0.4 : 0.2) : Number(i === (decision === 'selected' ? 2 : 0))]));
  return { type: 'score', score: Object.entries(probabilities).reduce((sum, [level, probability]) => sum + Number(level) * probability, 0), confidence: decision === 'unknown' ? 0.4 : 1, probabilities, legend: Object.fromEntries(levels.map((level, i) => [String(i), level])) };
}

function evidence(tool, rows) {
  return rows.flatMap(row => tool.endsWith('FileContent') || tool === 'localFetch'
    ? sourceLines(row).map(([line, text]) => `${line}:${text}`)
    : (row.files ?? []).flatMap(file => tool === 'localSearch'
      ? (file.matches ?? []).map(match => `${file.path}:${match.line}:${match.value}`)
      : [file.path]));
}
function selectedIdentity(tool, state, identity, scope) {
  if (tool === 'ghSearchCode') return identity === state.evidence.path;
  if (tool === 'localSearch') return state.matches.some(match => identity.startsWith(`${path.relative(scope.cwd, path.resolve(scope.query.path, state.path))}:${match.line}:`));
  const line = Number(identity.slice(0, identity.indexOf(':')));
  return state.sourceLineRanges.some(([start, end]) => line >= start && line <= end);
}
const ordered = values => values.toSorted((a, b) => a.localeCompare(b));

function validPartition(receipt) {
  return receipt.selectedItems > 0 && receipt.excludedItems > 0 && receipt.unknownItems > 0
    && receipt.selectedItems + receipt.excludedItems + (receipt.relatedItems ?? 0) + receipt.unknownItems === receipt.totalItems;
}
function restoredEvidenceMatches(report, tool, expected) {
  const restored = evidence(tool, report.responses.filter(entry => entry.tool === tool).flatMap(entry => rowData(entry.result)));
  return report.failures.length === 0 && same(ordered(restored), ordered(expected));
}

async function oneSurface(ctx, tool, surface, setup) {
  const provider = providerSession(ctx, tool, surface), checks = [];
  let attempt = 0;
  provider.set(request => ++attempt === 1 ? { status: 429, body: { error: 'temporary provider limit' } } : { body: classificationAnswer(request, question => selectedAnswer(question, setup.decide(request.state))) });
  try {
    const classification = { predicates: ['Keep direct selected evidence.'], context: 'Unknown evidence must remain available for inspection.', longContext: true };
    const report = await ctx.explore(tool, ctx.q({ ...setup.query, classification }), surface);
    const rows = report.responses.filter(entry => entry.tool === tool).flatMap(entry => rowData(entry.result));
    const selected = rows.filter(row => row.selection), receipt = selected[0]?.selection;
    assert.ok(receipt, JSON.stringify(rows));
    checks.push(['selection.partition', report.failures.length === 0 && validPartition(receipt), { failures: report.failures, receipt }]);
    const selectedEvidence = evidence(tool, selected);
    const selectedStates = provider.sent.filter(entry => setup.decide(entry.request.state) === 'selected').map(entry => entry.request.state);
    checks.push(['selection.retry', provider.sent.filter(entry => same(entry.request, provider.sent[0].request)).length === 2, provider.sent.length]);
    checks.push(['selection.context', provider.sent.length > 0 && provider.sent.every(entry => entry.headers['typesafe-danger-zone-long-context'] === 'true' && Object.values(entry.request.questions).every(question => question.instructions.context === classification.context)), provider.sent.length]);
    const before = provider.sent.length;
    const ordinary = await ctx.explore(tool, ctx.q(setup.query), surface);
    const unfiltered = selected[0].next.unfiltered;
    assert.ok(unfiltered, JSON.stringify(selected[0]));
    const recovery = await ctx.explore(tool, unfiltered.input, surface);
    const full = evidence(tool, ordinary.responses.filter(entry => entry.tool === tool).flatMap(entry => rowData(entry.result)));
    const recovered = evidence(tool, recovery.responses.filter(entry => entry.tool === tool).flatMap(entry => rowData(entry.result)));
    const expected = full.filter(value => selectedStates.some(state => selectedIdentity(tool, state, value, { cwd: ctx.cwd, query: setup.query })));
    checks.push(['selection.body', expected.length > 0 && same(ordered(selectedEvidence), ordered(expected)), { expected: expected.length, selected: selectedEvidence.length }]);
    checks.push(['selection.recovery', recovery.failures.length === 0 && provider.sent.length === before && same(ordered(recovered), ordered(full)) && new Set(recovered).size === recovered.length, { expected: full.length, recovered: recovered.length }]);
    const cached = data(await ctx.call(ctx.q({ ...setup.query, classification }), surface));
    checks.push(['selection.cache', provider.sent.length === before && cached.selection.selectedItems === receipt.selectedItems, cached.selection]);
    provider.set(() => ({ status: 401, body: { error: 'fixture refusal' } }));
    const failure = data(await ctx.call(ctx.q({ ...setup.query, classification: { ...classification, predicates: ['A separate uncached selection question.'] } }), surface));
    checks.push(['selection.failClosed', failure?.errorCode === 'classificationProviderError' && evidence(tool, [failure]).length === 0 && failure.next?.unfiltered?.kind === 'continue', failure]);
    const afterFailure = provider.sent.length;
    const restored = await ctx.explore(tool, failure.next.unfiltered.input, surface);
    checks.push(['selection.failureRecovery', provider.sent.length === afterFailure && restoredEvidenceMatches(restored, tool, full), { expected: full.length, failures: restored.failures }]);
  } finally { provider.close(); }
  return checks;
}

export async function inlineChecks(ctx, tool) {
  const setup = fixture(ctx, tool), checks = [];
  for (const surface of ctx.surfaces) for (const [label, ok, detail] of await oneSurface(ctx, tool, surface, setup)) checks.push([`${surface}.${label}`, ok, JSON.stringify(detail)]);
  if (tool.startsWith('local')) fs.rmSync(path.join(ctx.cwd, 'advanced-selection'), { recursive: true });
  return checks;
}
