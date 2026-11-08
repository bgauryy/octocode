import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { memorySectionsChecks, sectionChecks } from './classification-sections.mjs';

const result = data => ({ json: { results: [{ index: 0, data }] } });
const read = { kind: 'continue', tool: 'localFetch', input: { cursor: 'source' } };
const resource = rows => ({ status: 'ready', sections: rows, read, coverage: { remainingSections: 0, answered: 140 } });

function fixtureContext(cwd, defect) {
  let last;
  return {
    cwd, q: query => ({ queries: [query] }),
    explore: async (tool, input) => {
      const source = fs.readFileSync(path.join(cwd, 'sections-cli.rs'), 'utf8');
      const lines = source.trimEnd().split('\n');
      if (tool === 'localFetch') return { failures: [], responses: [{ tool, result: result({ content: lines.map((line, i) => `${i + 1}\t${line}`).join('\n') }) }] };
      let offset = 0;
      const sections = lines.map((line, i) => {
        const start = offset;
        offset += Buffer.byteLength(line) + 1;
        return { id: `section-${i}`, target: { bytes: [start, offset], lines: { startLine: i + 1, endLine: i + 1 } }, answers: { explicit: { status: 'answered', answer: { noul: 0.9 } }, strength: { status: 'answered', answer: { score: 1 } } } };
      });
      if (defect === 'wideBytes') sections[0].target.bytes = [0, Buffer.byteLength(source)];
      if (defect === 'missingNewline') sections[0].target.bytes[1] -= 1;
      if (defect === 'overlap') sections[1].target.bytes[0] -= 1;
      if (defect === 'wrongLine') sections[1].target.lines.endLine += 1;
      if (defect === 'characterOffsets') sections[0].target.bytes[1] = lines[0].length + 1;
      const first = sections.slice(0, 35), tail = sections.slice(35);
      if (defect === 'duplicateCell') tail.push(sections[0]);
      const page = rows => ({ questions: input.queries[0].questions, resources: { local: resource(rows), github: { status: 'ready' }, missing: { status: 'error' } } });
      const initial = page(first);
      initial.next = { nextPage: { kind: 'continue', tool: 'clasify', input: { cursor: 'tail' } } };
      last = page(tail);
      return { failures: [], responses: [{ tool, input, result: result(initial) }, { tool, input: { cursor: 'tail' }, result: result(last) }] };
    },
    call: async () => {
      const replay = structuredClone(last);
      replay.replayed = true;
      if (defect === 'emptyReplay') replay.resources.local.sections = [];
      if (defect === 'changedReplay') replay.resources.local.sections[0].answers.explicit.answer.noul = 0.1;
      return result(replay);
    },
  };
}

test('section acceptance requires exact UTF-8 source coverage, unique cells, and identical replay', async () => {
  for (const [defect, check] of [
    [null, null], ['duplicateCell', 'sections.complete'], ['wideBytes', 'sections.coordinates'],
    ['missingNewline', 'sections.coordinates'], ['overlap', 'sections.coordinates'],
    ['wrongLine', 'sections.coordinates'], ['characterOffsets', 'sections.coordinates'],
    ['emptyReplay', 'sections.frozenReplay'], ['changedReplay', 'sections.frozenReplay'],
  ]) {
    const cwd = fs.mkdtempSync(path.join(os.tmpdir(), 'octocode-section-oracle-'));
    try {
      const checks = await sectionChecks(fixtureContext(cwd, defect), 'cli', { sent: [] });
      if (check) assert.equal(checks.find(row => row[0] === check)?.[1], false, `${defect} must fail ${check}`);
      else assert.ok(checks.every(row => row[1]), JSON.stringify(checks));
    } finally { fs.rmSync(cwd, { recursive: true, force: true }); }
  }
});

/** The runtime's default response window (core `limits.defaultResponseChars`). */
const ONE_RESPONSE = 20_000;

/** A memory-mode CLI stub: plans refused, evaluations answered with a fresh read; `defect` breaks one promise. */
function memoryStub(cwd, defect, sent) {
  const answer = { evidence: { type: 'noul', noul: 0.9 } };
  const refuse = query => {
    if (defect === 'billedRefusal') sent.push(query);
    return { status: 'error', data: {
      errorCode: 'classificationContextUnavailable',
      error: 'This runtime keeps nothing for a later call; no source was read and no classification provider was called.',
      hints: defect === 'missingGuidance' ? [] : ['Section plans across CLI calls need storage.mode persistent, or one running MCP server.'],
    } };
  };
  const evaluate = query => {
    if (defect === 'refusedEvaluate') return refuse(query);
    sent.push(query);
    const related = [{ kind: 'related', tool: 'localFetch', input: { queries: [{ path: query.source.query.path }] } }];
    const pages = fs.statSync(query.source.query.path).size > ONE_RESPONSE;
    const hinted = defect === 'falsePagedHint' || (pages && defect !== 'missingPagedHint');
    const hints = hinted ? ['This source is larger than one response, so its read in related[] pages: run it with storage.mode persistent, or in one running MCP server.'] : undefined;
    return { data: { answers: answer, related, ...(hints ? { hints } : {}), ...(defect === 'deadRead' ? { next: { read } } : {}) } };
  };
  const row = query => {
    if (query.operation === 'sections') return refuse(query);
    if (!Object.hasOwn(query, 'content')) return evaluate(query);
    if (defect !== 'lostSibling') sent.push(query);
    return { data: { answers: defect === 'lostSibling' ? {} : answer } };
  };
  return async (tool, input, options) => {
    if (tool === 'localFetch') {
      assert.equal(options.env.OCTOCODE_STORAGE_MODE, 'memory');
      // A memory CLI call refuses a read that pages past one response.
      if (fs.statSync(input.queries[0].path).size > ONE_RESPONSE) return { status: 5, json: { error: 'This answer requires continuations' } };
      const text = defect === 'staleRead' ? '' : fs.readFileSync(input.queries[0].path, 'utf8').slice(0, 200);
      return { status: 0, json: { results: [{ index: 0, data: { content: text } }] } };
    }
    assert.equal(tool, 'clasify');
    assert.equal(options.env.OCTOCODE_STORAGE_MODE, 'memory');
    assert.equal(input.responseChars, 50_000);
    const rows = input.queries.map((query, index) => ({ index, ...row(query) }));
    const status = rows.some(outcome => outcome.status) ? 5 : 0;
    return { status, json: { responsePagination: { hasMore: defect === 'pagedRefusal' }, results: rows } };
  };
}

test('memory classification refuses section plans before billing and answers source evaluations with a fresh read', async () => {
  const cwd = fs.mkdtempSync(path.join(os.tmpdir(), 'octocode-memory-plan-oracle-'));
  try {
    for (const defect of [null, 'billedRefusal', 'lostSibling', 'deadRead', 'pagedRefusal', 'missingGuidance', 'refusedEvaluate', 'staleRead', 'missingPagedHint', 'falsePagedHint']) {
      const sent = [];
      const checks = await memorySectionsChecks({ cwd, cliCall: memoryStub(cwd, defect, sent) }, 'cli', { sent });
      assert.equal(checks.length, 7, 'simple and seventy-section plan refusal, evaluation and fresh read, plus mixed literal');
      if (defect === 'lostSibling') assert.equal(checks.at(-1)[1], false);
      else assert.equal(checks.every(row => row[1]), defect === null, `${defect ?? 'valid'}: ${JSON.stringify(checks.map(row => [row[0], row[1]]))}`);
    }
  } finally { fs.rmSync(cwd, { recursive: true, force: true }); }
});
