#!/usr/bin/env node

import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';

const HELP = `Validate Octocode RFC Generator artifacts.

Usage:
  node scripts/validate-rfc.mjs <file-or-folder>
  node scripts/validate-rfc.mjs --draft <file-or-folder>
  node scripts/validate-rfc.mjs --self-test
  node scripts/validate-rfc.mjs --help

Checks primary mode, required sections, decision-blocker closure, step dependency
order, acceptance links, KPI traceability, rollback-threshold ownership, and
mermaid diagram types (unknown type = error; an RFC with no diagram = warning).
A plan may live in RFC.md under ## Steps. IMPLEMENTATION.md is the plan that has left the RFC.
--draft checks an exploratory RFC's structure and explicit Draft/none declarations;
it permits declared open blockers and rejects common covert recommendation or
execution language. This bounded lint does not certify prose truth.`;

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

const DRAFT_COMMITMENT_PATTERNS = [
  {
    name: 'declared winner',
    pattern: /\b(?:option|candidate|approach|design)\s+[A-Z0-9][\w-]*\s+(?:wins|is selected|is recommended|is the (?:obvious|clear|best|preferred) (?:winner|choice))\b/i,
  },
  {
    name: 'committed implementation',
    pattern: /\b(?:we|the team|this rfc)\s+(?:will|shall)\s+(?:adopt|choose|select|implement|deploy|roll out|migrate to|use)\b/i,
  },
  {
    name: 'immediate execution',
    pattern: /\b(?:start|begin)\s+(?:the\s+)?(?:implementation|migration|rollout|deployment)\s+(?:now|immediately|today)\b/i,
  },
  {
    name: 'selection directive',
    pattern: /\b(?:proceed with|approve|adopt|select|choose)\s+(?:option|candidate|approach|design)\b/i,
  },
  {
    name: 'explicit recommendation',
    pattern: /\b(?:we|the team|this rfc)\s+recommend(?:s)?\s+(?:option|candidate|approach|design)\b/i,
  },
];

function draftCommitmentFindings(content) {
  const findings = [];
  let fenced = false;
  for (const [index, rawLine] of content.split('\n').entries()) {
    const line = rawLine.trim();
    if (line.startsWith('```')) { fenced = !fenced; continue; }
    if (fenced || !line || line.startsWith('>') || /^(Status|Recommendation):/i.test(line)) continue;
    if (/^if\b/i.test(line) || /\b(?:unless|would|could|only when|subject to|do not|must not|not selected|not recommended)\b/i.test(line) || /\bwins\s+if\b/i.test(line)) continue;
    const match = DRAFT_COMMITMENT_PATTERNS.find(({ pattern }) => pattern.test(line));
    if (match) findings.push({ line: index + 1, kind: match.name, text: line.slice(0, 180) });
  }
  return findings;
}

function validateFiles(files, { draft = false } = {}) {
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
  const planInsideRfc = hasRfc && heading(files['RFC.md'], 'Steps');
  if (planInsideRfc && hasImplementation) {
    errors.push('RFC.md contains ## Steps; keep the plan in that file or move it to IMPLEMENTATION.md, not both.');
  }
  if (planInsideRfc) {
    for (const requiredHeading of REQUIRED['IMPLEMENTATION.md']) {
      if (!heading(files['RFC.md'], requiredHeading)) {
        errors.push(`RFC.md: a plan in the same file is missing "## ${requiredHeading}".`);
      }
    }
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
    if (draft) {
      const statuses = [...files['RFC.md'].matchAll(/^Status:\s*(.+)$/gm)];
      const recommendations = [...files['RFC.md'].matchAll(/^Recommendation:\s*(.+)$/gm)];
      if (statuses.length !== 1 || statuses[0][1].trim() !== 'Draft') {
        errors.push('RFC.md: --draft requires exactly one Status: Draft declaration.');
      }
      if (recommendations.length !== 1 || recommendations[0][1].trim() !== 'none') {
        errors.push('RFC.md: --draft requires exactly one Recommendation: none declaration.');
      }
      const blockers = [...questions.matchAll(/^Decision blockers:\s*(none|resolved|open|blocked|contested)\.?\s*$/gm)];
      if (blockers.length !== 1) {
        errors.push('RFC.md: --draft requires one explicit Decision blockers: none, resolved, open, blocked or contested declaration.');
      } else if (!['none', 'resolved'].includes(blockers[0][1])) {
        if (!/^Q\d+:\s*\S.*\bowner\b.*\bevidence gap\b.*\bnext check\b/im.test(questions)) {
          errors.push('RFC.md: open draft blockers require a Q<number> entry with owner, evidence gap, and next check.');
        }
        const alternatives = section(files['RFC.md'], 'Rationale and Alternatives');
        if (!/^Comparison outcome:\s*unresolved\.?\s*$/im.test(alternatives)) {
          errors.push('RFC.md: an open-blocker Draft requires Comparison outcome: unresolved in Rationale and Alternatives.');
        }
        for (const finding of draftCommitmentFindings(files['RFC.md'])) {
          errors.push(`RFC.md: possible covert recommendation (${finding.kind}) at line ${finding.line}: ${finding.text}`);
        }
      }
    } else if (!/Decision blockers:\s*(?:none|resolved)\b/i.test(questions)) {
      errors.push('RFC.md: decision blockers must be none or resolved before recommendation.');
    }
  }
  if (draft && !hasRfc) errors.push('--draft requires RFC.md; standalone plans use readiness checks.');
  errors.push(...diagramFindings(files).errors);

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
  if (planInsideRfc) stepDocuments.push('RFC.md');
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

const MERMAID_TYPES = new Set([
  'flowchart', 'graph', 'sequenceDiagram', 'classDiagram', 'stateDiagram', 'stateDiagram-v2', 'erDiagram',
  'journey', 'gantt', 'pie', 'quadrantChart', 'requirementDiagram', 'gitGraph', 'C4Context', 'C4Container',
  'C4Component', 'C4Dynamic', 'C4Deployment', 'mindmap', 'timeline', 'zenuml', 'sankey-beta', 'xychart-beta',
  'block-beta', 'packet-beta', 'kanban', 'architecture-beta', 'radar-beta', 'treemap-beta',
]);

// Mermaid blocks must open with a known diagram keyword; an RFC without any diagram only warns.
function diagramFindings(files) {
  const errors = [];
  const warnings = [];
  for (const [name, content] of Object.entries(files)) {
    const blocks = [...content.matchAll(/^```mermaid[^\n]*\n([\s\S]*?)^```/gm)];
    for (const [, body] of blocks) {
      const first = body.split('\n').map((line) => line.trim()).find((line) => line && !line.startsWith('%%'));
      const keyword = first?.split(/\s+/)[0];
      if (!keyword || !MERMAID_TYPES.has(keyword)) {
        errors.push(`${name}: mermaid block starts with unknown diagram type "${keyword ?? ''}".`);
      }
    }
    if (name === 'RFC.md' && blocks.length === 0) {
      warnings.push('RFC.md: no mermaid diagram; show flows, comparisons, proportions or plan dependencies as diagrams (references/rfc-diagrams.md).');
    }
  }
  return { errors, warnings };
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
  const draftRfc = validRfc.replace('# RFC: Valid', '# RFC: Valid\nStatus: Draft\nRecommendation: none')
    .replace('Chosen option.', 'Comparison outcome: unresolved.\nOption A improves speed; Option B preserves compatibility. The result depends on Q1.')
    .replace('Decision blockers: none.', 'Decision blockers: open.\nQ1: compatibility — owner: API team — evidence gap: old-client behavior — next check: replay.');
  const draftCases = [
    ['honest draft', { 'RFC.md': draftRfc }, true],
    ['missing status', { 'RFC.md': draftRfc.replace('Status: Draft\n', '') }, false],
    ['accepted status', { 'RFC.md': draftRfc.replace('Status: Draft', 'Status: Accepted') }, false],
    ['duplicate status', { 'RFC.md': draftRfc.replace('Status: Draft', 'Status: Draft\nStatus: Accepted') }, false],
    ['final recommendation', { 'RFC.md': draftRfc.replace('Recommendation: none', 'Recommendation: final') }, false],
    ['missing recommendation', { 'RFC.md': draftRfc.replace('Recommendation: none\n', '') }, false],
    ['unnamed blocker', { 'RFC.md': draftRfc.replace(/^Q1:.*$/m, '') }, false],
    ['incomplete blocker', { 'RFC.md': draftRfc.replace(' — evidence gap: old-client behavior', '') }, false],
    ['missing unresolved comparison', { 'RFC.md': draftRfc.replace('Comparison outcome: unresolved.\n', '') }, false],
    ['declared winner', { 'RFC.md': draftRfc.replace('Option A improves speed;', 'Option A wins;') }, false],
    ['committed migration', { 'RFC.md': draftRfc.replace('The result depends on Q1.', 'We will migrate to Option A.') }, false],
    ['immediate execution', { 'RFC.md': draftRfc.replace('The result depends on Q1.', 'Start the migration now.') }, false],
    ['explicit recommendation', { 'RFC.md': draftRfc.replace('The result depends on Q1.', 'We recommend Option A.') }, false],
    ['conditional comparison', { 'RFC.md': draftRfc.replace('The result depends on Q1.', 'Option A would win if Q1 proves compatibility.') }, true],
    ['covert winner with later condition', { 'RFC.md': draftRfc.replace('The result depends on Q1.', 'Option A wins; if Q1 fails, revisit later.') }, false],
    ['missing section', { 'RFC.md': draftRfc.replace('## Summary', '## Other') }, false],
    ['plan only', { 'PLAN.md': validPlan }, false],
  ];
  for (const [label, files, expected] of draftCases) {
    const actual = validateFiles(files, { draft: true });
    if ((actual.length === 0) !== expected) throw new Error(`Draft self-test ${label}: ${JSON.stringify(actual)}`);
  }
  if (!validateFiles({ 'RFC.md': draftRfc }).some(error => error.includes('decision blockers'))) {
    throw new Error('Default readiness must still reject open blockers.');
  }
  const diagramRfc = `${validRfc}\n\`\`\`mermaid\nflowchart LR\n  A --> B\n\`\`\`\n`;
  const diagramCases = [
    ['known diagram', diagramRfc, 0, 0],
    ['unknown diagram type', diagramRfc.replace('flowchart LR', 'flowchar LR'), 1, 0],
    ['no diagram warns only', validRfc, 0, 1],
  ];
  for (const [label, rfc, errorCount, warningCount] of diagramCases) {
    const errors = validateFiles({ 'RFC.md': rfc });
    const { warnings } = diagramFindings({ 'RFC.md': rfc });
    if (errors.length !== errorCount || warnings.length !== warningCount) {
      throw new Error(`Diagram self-test ${label}: ${JSON.stringify({ errors, warnings })}`);
    }
  }
  const rfcWithPlan = `${validRfc}
## Plan Context
- Primary: §Summary
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
## Files, APIs, and Contracts
None.
## Risk Mitigations
None.
## Test and Verification Plan
Run tests.
## Rollout, Migration, and Rollback
Use the inline threshold.
`;
  const inFileErrors = validateFiles({ 'RFC.md': rfcWithPlan });
  if (inFileErrors.length) throw new Error(`In-file plan failed: ${inFileErrors.join('; ')}`);
  const splitErrors = validateFiles({ 'RFC.md': rfcWithPlan, 'IMPLEMENTATION.md': validImplementation });
  if (!splitErrors.some((error) => error.includes('not both'))) {
    throw new Error(`Split plan should fail: ${JSON.stringify(splitErrors)}`);
  }
  const forwardInRfc = validateFiles({
    'RFC.md': rfcWithPlan.replace('Depends on: none', 'Depends on: S2'),
  });
  if (!forwardInRfc.some((error) => error.includes('forward or unknown dependency S2'))) {
    throw new Error(`In-file forward dependency should fail: ${JSON.stringify(forwardInRfc)}`);
  }
  console.log(JSON.stringify({ valid: true, selfTest: true, cases: 30 }));
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
const draft = args[0] === '--draft';
const targets = draft ? args.slice(1) : args;
if (targets.length !== 1 || targets[0].startsWith('-')) {
  console.error(HELP);
  process.exit(2);
}

try {
  const files = loadFiles(targets[0]);
  const errors = validateFiles(files, { draft });
  const { warnings } = diagramFindings(files);
  const result = {
    valid: errors.length === 0, target: path.resolve(targets[0]),
    mode: draft ? 'draft' : 'readiness',
    ...(draft ? { reviewReady: false, semanticLint: 'bounded-pattern-check' } : {}), errors,
    ...(warnings.length ? { warnings } : {}),
  };
  console.log(JSON.stringify(result, null, 2));
  if (errors.length) {
    console.error(`Validation failed with ${errors.length} error(s).`);
    process.exit(1);
  }
} catch (error) {
  console.error(error instanceof Error ? error.message : String(error));
  process.exit(2);
}
