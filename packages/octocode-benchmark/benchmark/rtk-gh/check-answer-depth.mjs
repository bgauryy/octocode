#!/usr/bin/env node
// Automated depth/coverage pre-check for rtk-gh benchmark answers.
//
// Runs BEFORE any human/LLM quality scoring (quality.json). It is a cheap,
// deterministic gate that catches the two failure modes manual review keeps
// finding by hand:
//   1. Coverage gaps -- the answer never mentions a ground-truth keyTerm
//      anywhere (not even as a rejected alternative), which is the exact
//      signature of "confidently answered the wrong thing" (see q5
//      criticalKeyTerm below -- this is what caught rtk-gh-2's q5 miss).
//   2. Calibration mismatches -- Confidence: high paired with hedge language
//      ("approximately", "not fully verified", "ran out of budget", ...)
//      in the same answer/evidence text.
//
// This script does NOT assign a quality score. It produces a flags report
// that the judge must read before writing quality.json. It is intentionally
// heuristic and literal (substring/regex matching) -- false positives are
// expected and fine; false NEGATIVES on the coverage check would defeat the
// point, so keyTerms in ground-truth.json should be exact identifiers that
// any correct answer would naturally contain.
//
// Usage:
//   node check-answer-depth.mjs <runDir> <groundTruthPath> [--json]
//
// <runDir> must contain agents/<agentId>/answers.md for each agent.

import { readFileSync, readdirSync, existsSync, writeFileSync } from 'fs';
import { join } from 'path';

const [, , runDirArg, groundTruthArg, ...rest] = process.argv;
const asJson = rest.includes('--json');

if (!runDirArg || !groundTruthArg) {
  console.error(
    'Usage: node check-answer-depth.mjs <runDir> <groundTruthPath> [--json]'
  );
  process.exit(2);
}

const runDir = runDirArg;
const groundTruth = JSON.parse(readFileSync(groundTruthArg, 'utf8'));

const HEADER_RE = /^#{2,3}\s*Q(\d+)\b/i;

const HEDGE_WORDS = [
  'approximat', // approximately, approximate
  'not fully verified',
  'could not confirm',
  'could not verify',
  'ran out of',
  'ran out',
  'best guess',
  'best localization',
  'not certain',
  'unable to confirm',
  'unable to verify',
  'counted manually',
  'off by',
  'off-by',
  'rough estimate',
  'may be wrong',
  'might be wrong',
  'treat as approximate',
];

const REASONING_CONNECTIVES = [
  'because',
  'therefore',
  'as a result',
  'which means',
  'this means',
  'so that',
  'confirms',
  'consistent with',
  'given that',
  'this implies',
  'in other words',
  'that is why',
  'the reason',
];

const ANCHOR_PATTERNS = [
  /\b[\w./-]+\.\w{1,5}:\d+\b/g, // file.ts:123
  /\bline\s*\d+\b/gi, // "line 123"
  /#\d{2,6}\b/g, // PR/issue number
  /\b[0-9a-f]{7,40}\b/g, // sha-like
  /`[^`\n]{2,80}`/g, // inline code span
];

function splitSections(md) {
  const lines = md.split('\n');
  const sections = {};
  let current = null;
  for (const line of lines) {
    const m = line.match(HEADER_RE);
    if (m) {
      current = `q${m[1]}`;
      sections[current] = [];
      continue;
    }
    if (current) sections[current].push(line);
  }
  const out = {};
  for (const [k, v] of Object.entries(sections)) out[k] = v.join('\n');
  return out;
}

function extractField(text, field) {
  // Tolerate Markdown emphasis around the field label (e.g. "**Confidence:** high")
  // -- an earlier version only matched a bare "Confidence:" at line start and silently
  // treated bold-labeled fields as "(missing)", masking a real calibration mismatch.
  const re = new RegExp(`^\\**${field}\\**:\\s*\\**(.+?)\\**\\s*$`, 'im');
  const m = text.match(re);
  return m ? m[1].trim() : null;
}

function countMatches(text, patterns) {
  let n = 0;
  for (const p of patterns) {
    const matches = text.match(p);
    if (matches) n += matches.length;
  }
  return n;
}

function containsAny(text, words) {
  const lower = text.toLowerCase();
  return words.filter(w => lower.includes(w.toLowerCase()));
}

function checkAnswer(sectionText, qSpec) {
  const confidence = (extractField(sectionText, 'Confidence') || '').toLowerCase();
  const flags = [];

  const missingKeyTerms = (qSpec.keyTerms || []).filter(
    term => !sectionText.includes(term)
  );
  if (missingKeyTerms.length > 0) {
    const isCritical =
      qSpec.criticalKeyTerm && missingKeyTerms.includes(qSpec.criticalKeyTerm);
    flags.push({
      type: isCritical ? 'COVERAGE_CRITICAL' : 'COVERAGE_GAP',
      detail: `missing key term(s): ${missingKeyTerms.join(', ')}${
        isCritical ? ` -- ${qSpec.criticalKeyTermNote || ''}` : ''
      }`,
    });
  }

  const hedges = containsAny(sectionText, HEDGE_WORDS);
  if (confidence.startsWith('high') && hedges.length > 0) {
    flags.push({
      type: 'CALIBRATION_MISMATCH',
      detail: `Confidence: high but hedge language present: "${hedges.join('", "')}"`,
    });
  }

  const anchorCount = countMatches(sectionText, ANCHOR_PATTERNS);
  if (anchorCount === 0) {
    flags.push({ type: 'NO_ANCHORS', detail: 'no file:line/PR#/sha/code-span anchors found' });
  }

  const reasoningHits = containsAny(sectionText, REASONING_CONNECTIVES);
  const hasReasoning = reasoningHits.length > 0;

  return {
    confidence: confidence || '(missing)',
    anchorCount,
    hasReasoning,
    missingKeyTerms,
    flags,
  };
}

const agentsDir = join(runDir, 'agents');
if (!existsSync(agentsDir)) {
  console.error(`No agents/ dir under ${runDir}`);
  process.exit(2);
}

const report = { runDir, agents: {} };
let totalFlags = 0;
let criticalFlags = 0;

for (const agentId of readdirSync(agentsDir).sort()) {
  const answersPath = join(agentsDir, agentId, 'answers.md');
  if (!existsSync(answersPath)) continue;
  const md = readFileSync(answersPath, 'utf8');
  const sections = splitSections(md);
  const agentReport = {};
  for (const [qid, qSpec] of Object.entries(groundTruth.questions)) {
    const sectionText = sections[qid];
    if (!sectionText) {
      agentReport[qid] = {
        flags: [{ type: 'MISSING_SECTION', detail: `no ${qid} section found in answers.md` }],
      };
      totalFlags++;
      continue;
    }
    const result = checkAnswer(sectionText, qSpec);
    agentReport[qid] = result;
    totalFlags += result.flags.length;
    criticalFlags += result.flags.filter(f => f.type === 'COVERAGE_CRITICAL').length;
  }
  report.agents[agentId] = agentReport;
}

report._summary = { totalFlags, criticalFlags };

if (asJson) {
  console.log(JSON.stringify(report, null, 2));
} else {
  console.log(`Depth/coverage pre-check: ${runDir}`);
  console.log(`(read this BEFORE writing quality.json -- it is not a quality score)\n`);
  for (const [agentId, qs] of Object.entries(report.agents)) {
    const lines = [];
    for (const [qid, r] of Object.entries(qs)) {
      if (r.flags.length === 0) continue;
      for (const f of r.flags) {
        lines.push(`  ${qid}: [${f.type}] ${f.detail}`);
      }
    }
    if (lines.length > 0) {
      console.log(`${agentId}:`);
      console.log(lines.join('\n'));
    } else {
      console.log(`${agentId}: no flags`);
    }
  }
  console.log(
    `\nTotal flags: ${totalFlags} (critical coverage flags: ${criticalFlags})`
  );
}

const outPath = join(runDir, 'depth-check.json');
if (existsSync(runDir)) {
  writeFileSync(outPath, JSON.stringify(report, null, 2));
  if (!asJson) console.log(`\nWrote ${outPath}`);
}
