import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { sourceLines } from '../responses.mjs';
import { data, noul, readCaptured, same } from './classification-checks.mjs';

export async function sectionChecks(ctx, surface, provider) {
  const file = path.join(ctx.cwd, `sections-${surface}.rs`);
  const source = Array.from({ length: 70 }, (_, i) => `pub fn section_${i}() { println!("évidence λ ${i}"); }\n`).join('');
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
  const continuation = pages.find(row => row.next?.nextSectionPage)?.next.nextSectionPage;
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
    && coverage.remainingSections === 0 && coverage.answered === 140;
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

export async function memorySectionsChecks(ctx, surface, provider) {
  if (surface !== 'cli') return [];
  const file = path.join(ctx.cwd, 'memory-section-resource.rs');
  fs.writeFileSync(file, Array.from({ length: 70 }, (_, i) => `pub fn section_${i}() {}\n`).join(''));
  try {
    const before = provider.sent.length;
    const input = { ...ctx.q({ operation: 'sections', resources: { local: { tool: 'localFetch', query: { path: file } } }, questions: { evidence: noul('Is source evidence available?') } }), responseChars: 50_000 };
    const response = await ctx.cliCall('clasify', input, { env: { OCTOCODE_STORAGE_MODE: 'memory' } });
    const row = response.json?.results?.[0], resource = row?.data.resources.local;
    return [['sections.memoryPlanFailsWithoutInference', response.status === 5 && row?.status === 'error'
      && row.data.errorCode === 'classificationContextUnavailable' && typeof row.data.error === 'string'
      && resource?.sections.length === 70 && resource.coverage.unassessed === 70
      && response.json.responsePagination?.hasMore !== true
      && provider.sent.length === before, response.json]];
  } finally { fs.rmSync(file); }
}
