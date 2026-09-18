#!/usr/bin/env node
// Frozen eval for contentRef evidence resolution.
//
// KPI (primary):   host authored-input bytes per fork  (pointer vs inline paste)
// Guardrail (hard): evidence IDENTITY — the content Jev receives from a resolved
//                   pointer must byte-equal what the host would have pasted
//                   inline, and both packets must validate the route contract.
//
// A win that changed the evidence Jev sees would be a Goodhart failure, so the
// guardrail gates the KPI: reduction only counts when identity holds.

import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join, resolve } from 'node:path';
import { existsSync } from 'node:fs';
import { resolveEvidenceRefs } from './resolve-content-ref.mjs';
import { prepareCompactRun } from './decision-contract.mjs';

function gitRoot(start) {
  let cur = resolve(start);
  while (true) {
    if (existsSync(join(cur, '.git'))) return cur;
    const parent = dirname(cur);
    if (parent === cur) return resolve(start);
    cur = parent;
  }
}
const ROOT = gitRoot(fileURLToPath(import.meta.url));
const bytes = v => Buffer.byteLength(JSON.stringify(v));

// Held-out cases: real repo files, real spans. The span content is the "truth"
// the host would otherwise have pasted.
const CASES = [
  { id: 'c1', file: 'skills/octocode-jev-reasoning-loop/scripts/resolve-content-ref.mjs', lines: '91-95',
    claim: 'resolveEvidenceItem rejects an item that supplies both content and contentRef.' },
  { id: 'c2', file: 'skills/octocode-jev-reasoning-loop/scripts/run-loop.mjs', lines: '84-91',
    claim: 'metricBlock reports compact_input_bytes and request_bytes.' },
  { id: 'c3', file: 'skills/octocode-jev-reasoning-loop/package.json', lines: '1-3',
    claim: 'package.json declares a build script.' }
];

function truthContent(file, lines) {
  const [a, b] = lines.split('-').map(Number);
  const all = readFileSync(join(ROOT, file), 'utf8').split('\n');
  return all.slice(a - 1, b ?? a).join('\n');
}

function gatePacket(evidence) {
  return {
    route: 'hallucination_gate',
    willChangeAction: true,
    state: { goal: 'gate the claim', claim: 'x'.repeat(20), evidence: [evidence] }
  };
}

const results = [];
for (const c of CASES) {
  const truth = truthContent(c.file, c.lines);
  const inlineInput = gatePacket({ id: 'E1', scope: c.id, source: `${c.file}:${c.lines}`, content: truth });
  const refInput = gatePacket({ id: 'E1', scope: c.id, contentRef: { path: c.file, lines: c.lines, maxChars: 4000 } });

  const { input: resolved } = resolveEvidenceRefs(refInput, { rootDir: ROOT, allowedRoots: [ROOT] });
  const resolvedContent = resolved.state.evidence[0].content;

  const inlinePrep = prepareCompactRun(inlineInput);
  const refPrep = prepareCompactRun(resolved);
  const inlinePacketContent = inlinePrep.request?.state?.evidence?.[0]?.content;
  const refPacketContent = refPrep.request?.state?.evidence?.[0]?.content;

  const identity = resolvedContent === truth && inlinePacketContent === refPacketContent && refPacketContent === truth;
  const bothReady = inlinePrep.status === 'ready' && refPrep.status === 'ready';
  const authoredInline = bytes(inlineInput);
  const authoredRef = bytes(refInput);
  results.push({
    id: c.id, identity, bothReady,
    authored_inline_bytes: authoredInline,
    authored_ref_bytes: authoredRef,
    reduction: Number((1 - authoredRef / authoredInline).toFixed(4))
  });
}

const identityHeld = results.every(r => r.identity && r.bothReady);
const meanReduction = Number((results.reduce((s, r) => s + r.reduction, 0) / results.length).toFixed(4));
const verdict = {
  suiteVersion: 1,
  frozen: true,
  kpi: 'host_authored_input_bytes',
  guardrail: 'evidence_identity_and_route_validation',
  cases: results.length,
  results,
  meanReduction,
  checks: {
    evidence_identity_passed: identityHeld,
    host_reduction_passed: meanReduction > 0
  },
  verdict: identityHeld && meanReduction > 0 ? 'ACCEPT' : 'REJECT'
};
console.log(JSON.stringify(verdict));
process.exitCode = verdict.verdict === 'ACCEPT' ? 0 : 1;
