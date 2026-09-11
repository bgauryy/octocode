#!/usr/bin/env node

import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';

const HELP = `Validate Octocode RFC Generator artifacts.

Usage:
  node scripts/validate-rfc.mjs <file-or-folder>
  node scripts/validate-rfc.mjs --self-test
  node scripts/validate-rfc.mjs --help

Checks primary mode, required sections, decision-blocker closure, step dependency
order, acceptance links, KPI traceability, and rollback-threshold ownership.`;

const REQUIRED = {
  'RFC.md': [
    'Summary',
    'Goals and Non-Goals',
    'Motivation and Current State',
    'Drawbacks and Pre-mortem',
    'Rationale and Alternatives',
    'Unresolved Questions',
  ],
  'PLAN.md': [
    'Plan Context',
    'Execution Questions',
    'Acceptance Contract',
    'Approach',
    'Steps',
    'Files, APIs, and Contracts',
    'Risk Mitigations',
    'Test and Verification Plan',
    'Rollout, Migration, and Rollback',
  ],
  'IMPLEMENTATION.md': [
    'Plan Context',
    'Execution Questions',
    'Acceptance Contract',
    'Approach',
    'Steps',
    'Files, APIs, and Contracts',
    'Risk Mitigations',
    'Test and Verification Plan',
    'Rollout, Migration, and Rollback',
  ],
  'KPI.md': ['Acceptance Criteria', 'Success Metrics', 'Decision Rule', 'Traceability'],
  'PREREQUISITES.md': [
    'Required Current-State Evidence',
    'Baseline Verification',
    'Blockers Before Implementation',
  ],
  'RESOURCES.md': ['Primary Sources', 'Local Code References'],
};

function heading(content, name) {
  return new RegExp(`^## ${escapeRegex(name)}\\s*$`, 'm').test(content);
}

function section(content, name) {
  const match = content.match(
    new RegExp(`^## ${escapeRegex(name)}\\s*$([\\s\\S]*?)(?=^## |(?![\\s\\S]))`, 'm'),
  );
  return match?.[1] ?? '';
}

function escapeRegex(value) {
  return value.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

function validateFiles(files) {
  const errors = [];
  const names = new Set(Object.keys(files));
  const hasRfc = names.has('RFC.md');
  const hasPlan = names.has('PLAN.md');
  const hasImplementation = names.has('IMPLEMENTATION.md');
  const hasKpi = names.has('KPI.md');

  if (!hasRfc && !hasPlan && !hasImplementation) {
    errors.push('Expected RFC.md, PLAN.md, or IMPLEMENTATION.md.');
  }
  if (hasPlan && (hasRfc || hasImplementation)) {
    errors.push('PLAN.md is standalone; use IMPLEMENTATION.md for an RFC-linked plan.');
  }
  if (hasImplementation && !hasRfc) {
    errors.push('IMPLEMENTATION.md requires RFC.md; use PLAN.md for a standalone plan.');
  }

  for (const [name, requiredHeadings] of Object.entries(REQUIRED)) {
    const content = files[name];
    if (!content) continue;
    for (const requiredHeading of requiredHeadings) {
      if (!heading(content, requiredHeading)) {
        errors.push(`${name}: missing "## ${requiredHeading}".`);
      }
    }
  }

  if (hasRfc) {
    const questions = section(files['RFC.md'], 'Unresolved Questions');
    if (!/Decision blockers:\s*(?:none|resolved)\b/i.test(questions)) {
      errors.push('RFC.md: decision blockers must be none or resolved before recommendation.');
    }
  }

  if (hasPlan) {
    const context = section(files['PLAN.md'], 'Plan Context');
    if (!/^- Goal(?: \(standalone only\))?:\s*\S/im.test(context)) {
      errors.push('PLAN.md: Plan Context requires a Goal.');
    }
    if (!/^- Scope(?: \(standalone only\))?:\s*\S/im.test(context)) {
      errors.push('PLAN.md: Plan Context requires Scope.');
    }
  }
  if (hasImplementation && !/RFC\.md/i.test(section(files['IMPLEMENTATION.md'], 'Plan Context'))) {
    errors.push('IMPLEMENTATION.md: Plan Context must reference RFC.md.');
  }

  const stepDocuments = ['PLAN.md', 'IMPLEMENTATION.md'].filter((name) => names.has(name));
  const allStepIds = [];
  for (const name of stepDocuments) {
    const { errors: stepErrors, ids } = validateSteps(name, files[name]);
    errors.push(...stepErrors);
    allStepIds.push(...ids);

    if (!hasKpi && !hasInlineAcceptance(files[name])) {
      errors.push(`${name}: inline Acceptance Contract needs at least one data row when KPI.md is absent.`);
    }

    const rollout = section(files[name], 'Rollout, Migration, and Rollback');
    if (hasKpi && !/KPI\.md.*Decision Rule/i.test(rollout)) {
      errors.push(`${name}: rollback procedure must reference KPI.md §Decision Rule.`);
    }
  }

  if (hasKpi) {
    const kpi = files['KPI.md'];
    const rule = section(kpi, 'Decision Rule');
    const traceability = section(kpi, 'Traceability');
    if (!/KPI\.md owns the measurable rollback threshold/i.test(rule)) {
      errors.push('KPI.md: Decision Rule must own the measurable rollback threshold.');
    }
    if (!/Implementation step\(s\)/i.test(traceability)) {
      errors.push('KPI.md: Traceability needs an "Implementation step(s)" column.');
    }
    for (const id of allStepIds) {
      if (!new RegExp(`\\b${escapeRegex(id)}\\b`).test(traceability)) {
        errors.push(`KPI.md: Traceability does not map implementation step ${id}.`);
      }
    }
  }

  return errors;
}

function validateSteps(name, content) {
  const errors = [];
  const seen = new Set();
  const ids = [];
  const lines = section(content, 'Steps').split('\n');
  const stepPattern = /^- \[[ xX]\] (S\d+)\.\s+(.+)$/;
  let previousNumber = 0;

  for (const line of lines) {
    const match = line.match(stepPattern);
    if (!match) continue;
    const [, id, body] = match;
    const number = Number(id.slice(1));
    if (seen.has(id)) errors.push(`${name}: duplicate step ID ${id}.`);
    if (number <= previousNumber) errors.push(`${name}: step IDs must increase in document order.`);
    previousNumber = number;

    const fields = body.match(
      /Depends on:\s*(.*?)\s*—\s*Produces:\s*(.*?)\s*—\s*Acceptance:\s*(.*?)\s*—\s*Verify:\s*(.*?)(?:\s*—|$)/,
    );
    if (!fields) {
      errors.push(`${name}: ${id} must declare Depends on, Produces, Acceptance, and Verify in that order.`);
    } else {
      const [, dependencies, produces, acceptance, verify] = fields;
      for (const [field, value] of [
        ['Depends on', dependencies],
        ['Produces', produces],
        ['Acceptance', acceptance],
        ['Verify', verify],
      ]) {
        if (!value.trim() || /[{}]/.test(value)) errors.push(`${name}: ${id} has an incomplete ${field} field.`);
      }
      for (const dependency of dependencies.match(/\bS\d+\b/g) ?? []) {
        if (!seen.has(dependency)) {
          errors.push(`${name}: ${id} has forward or unknown dependency ${dependency}.`);
        }
      }
    }
    seen.add(id);
    ids.push(id);
  }

  if (ids.length === 0) errors.push(`${name}: Steps needs at least one "- [ ] S<number>." entry.`);
  return { errors, ids };
}

function hasInlineAcceptance(content) {
  const acceptance = section(content, 'Acceptance Contract');
  return acceptance
    .split('\n')
    .some((line) => /^\|/.test(line) && !/^\|[-: |]+\|?$/.test(line) && !/Requirement.*Pass\/fail acceptance/i.test(line));
}

function loadFiles(target) {
  const resolved = path.resolve(target);
  if (!fs.existsSync(resolved)) throw new Error(`Path does not exist: ${resolved}`);
  const stat = fs.statSync(resolved);
  if (stat.isFile()) return { [path.basename(resolved)]: fs.readFileSync(resolved, 'utf8') };
  if (!stat.isDirectory()) throw new Error(`Expected a Markdown file or directory: ${resolved}`);

  const files = {};
  for (const name of Object.keys(REQUIRED)) {
    const candidate = path.join(resolved, name);
    if (fs.existsSync(candidate)) files[name] = fs.readFileSync(candidate, 'utf8');
  }
  return files;
}

function runSelfTest() {
  const validPlan = `# Plan: Valid
## Plan Context
- Goal: Ship safely
- Scope: Planner only
- Constraints: No compatibility shim
## Execution Questions
None.
## Acceptance Contract
| Requirement | Pass/fail acceptance | Guardrail or rollback threshold |
|---|---|---|
| R1 | command passes | error rate unchanged |
## Approach
Implement in order.
## Steps
- [ ] S1. Add contract — Depends on: none — Produces: contract — Acceptance: R1 — Verify: test contract
- [ ] S2. Add consumer — Depends on: S1 — Produces: consumer — Acceptance: R1 — Verify: test consumer
## Files, APIs, and Contracts
None.
## Risk Mitigations
None.
## Test and Verification Plan
Run tests.
## Rollout, Migration, and Rollback
Use the inline threshold.
`;
  const invalidPlan = validPlan.replace('Depends on: none', 'Depends on: S2');
  const validErrors = validateFiles({ 'PLAN.md': validPlan });
  const invalidErrors = validateFiles({ 'PLAN.md': invalidPlan });
  if (validErrors.length || !invalidErrors.some((error) => error.includes('forward or unknown dependency S2'))) {
    throw new Error(`Self-test failed: ${JSON.stringify({ validErrors, invalidErrors })}`);
  }

  const validRfc = `# RFC: Valid
## Summary
Choose the contract.
## Goals and Non-Goals
Ship safely.
## Motivation and Current State
Current evidence.
## Drawbacks and Pre-mortem
Known risk.
## Rationale and Alternatives
Chosen option.
## Unresolved Questions
Decision blockers: none.
`;
  const validImplementation = `# Implementation: Valid
## Plan Context
- Primary: RFC.md §Summary
## Execution Questions
None.
## Acceptance Contract
Use KPI.md.
## Approach
Implement in order.
## Steps
- [ ] S1. Add contract — Depends on: none — Produces: contract — Acceptance: KPI R1 — Verify: test contract
## Files, APIs, and Contracts
None.
## Risk Mitigations
None.
## Test and Verification Plan
Run tests.
## Rollout, Migration, and Rollback
Trigger: KPI.md §Decision Rule.
`;
  const validKpi = `# Success and Verification: Valid
## Acceptance Criteria
R1 passes.
## Success Metrics
Error rate.
## Decision Rule
KPI.md owns the measurable rollback threshold.
## Traceability
| Primary requirement (§) | Implementation step(s) | Acceptance check |
|---|---|---|
| R1 | S1 | command passes |
`;
  const validSet = {
    'RFC.md': validRfc,
    'IMPLEMENTATION.md': validImplementation,
    'KPI.md': validKpi,
  };
  const validSetErrors = validateFiles(validSet);
  const invalidTraceErrors = validateFiles({
    ...validSet,
    'KPI.md': validKpi.replace('| R1 | S1 |', '| R1 | none |'),
  });
  if (validSetErrors.length || !invalidTraceErrors.some((error) => error.includes('does not map implementation step S1'))) {
    throw new Error(`Cross-artifact self-test failed: ${JSON.stringify({ validSetErrors, invalidTraceErrors })}`);
  }

  const temp = fs.mkdtempSync(path.join(os.tmpdir(), 'octocode-rfc-validator-'));
  try {
    fs.writeFileSync(path.join(temp, 'PLAN.md'), validPlan);
    const loadedErrors = validateFiles(loadFiles(temp));
    if (loadedErrors.length) throw new Error(`Filesystem self-test failed: ${loadedErrors.join('; ')}`);
  } finally {
    fs.rmSync(temp, { recursive: true, force: true });
  }
  console.log(JSON.stringify({ valid: true, selfTest: true, cases: 5 }));
}

const args = process.argv.slice(2);
if (args.includes('--help') || args.includes('-h')) {
  console.log(HELP);
  process.exit(0);
}
if (args.includes('--self-test')) {
  runSelfTest();
  process.exit(0);
}
if (args.length !== 1 || args[0].startsWith('-')) {
  console.error(HELP);
  process.exit(2);
}

try {
  const errors = validateFiles(loadFiles(args[0]));
  const result = { valid: errors.length === 0, target: path.resolve(args[0]), errors };
  console.log(JSON.stringify(result, null, 2));
  if (errors.length) {
    console.error(`Validation failed with ${errors.length} error(s).`);
    process.exit(1);
  }
} catch (error) {
  console.error(error instanceof Error ? error.message : String(error));
  process.exit(2);
}
