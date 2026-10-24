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
      initial.next = { nextSectionPage: { kind: 'continue', tool: 'clasify', input: { cursor: 'tail' } } };
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

test('terminal memory plan requests enough room and rejects incomplete display evidence', async () => {
  const cwd = fs.mkdtempSync(path.join(os.tmpdir(), 'octocode-memory-plan-oracle-'));
  try {
    for (const hasMore of [false, true]) {
      const checks = await memorySectionsChecks({
        cwd, q: query => ({ queries: [query] }),
        cliCall: async (tool, input, options) => {
          assert.equal(tool, 'clasify');
          assert.equal(input.responseChars, 50_000);
          assert.equal(options.env.OCTOCODE_STORAGE_MODE, 'memory');
          return { status: 5, json: { responsePagination: { hasMore }, results: [{ index: 0, status: 'error', data: {
            errorCode: 'classificationContextUnavailable', error: 'The plan cannot resume.',
            resources: { local: { sections: Array.from({ length: 70 }, () => ({})), coverage: { unassessed: 70 } } },
          } }] } };
        },
      }, 'cli', { sent: [] });
      assert.equal(checks[0][1], !hasMore);
    }
  } finally { fs.rmSync(cwd, { recursive: true, force: true }); }
});
