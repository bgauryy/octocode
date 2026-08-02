#!/usr/bin/env node
import { execFileSync } from 'node:child_process';
import { existsSync, readdirSync, readFileSync, statSync } from 'node:fs';
import { dirname, join, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const scriptDir = dirname(fileURLToPath(import.meta.url));
const packageDir = resolve(scriptDir, '..');
const repoRoot = resolve(packageDir, '..', '..');
const cli = join(repoRoot, 'packages', 'octocode', 'out', 'octocode.js');

const errors = [];
const warnings = [];
const notes = [];

function rel(path) {
  return relative(repoRoot, path) || '.';
}

function fail(message) {
  errors.push(message);
}

function warn(message) {
  warnings.push(message);
}

function readText(path) {
  return readFileSync(path, 'utf8');
}

function readJson(path) {
  try {
    return JSON.parse(readText(path));
  } catch (error) {
    fail(`${rel(path)} is not valid JSON: ${error.message}`);
    return undefined;
  }
}

const WALK_SKIP_DIRS = new Set(['node_modules', '.git', 'context', 'output', 'target', 'dist', 'out']);

function walk(dir, predicate = () => true) {
  const out = [];
  for (const entry of readdirSync(dir)) {
    if (WALK_SKIP_DIRS.has(entry)) continue;
    const path = join(dir, entry);
    const stats = statSync(path);
    if (stats.isDirectory()) {
      out.push(...walk(path, predicate));
    } else if (predicate(path)) {
      out.push(path);
    }
  }
  return out;
}

function runCli(args, options = {}) {
  return execFileSync(process.execPath, [cli, ...args], {
    cwd: repoRoot,
    encoding: 'utf8',
    stdio: ['ignore', 'pipe', 'pipe'],
    timeout: options.timeout ?? 30_000,
  });
}

function getActiveTools() {
  if (!existsSync(cli)) {
    fail(`Built CLI not found at ${rel(cli)}. Run: yarn workspace octocode build:dev`);
    return [];
  }

  let parsed;
  try {
    parsed = JSON.parse(runCli(['tools', '--json'], { timeout: 60_000 }));
  } catch (error) {
    fail(`Could not read live tool catalog via ${rel(cli)} tools --json: ${error.message}`);
    return [];
  }

  const tools = Array.isArray(parsed) ? parsed : parsed.tools ?? parsed.data ?? [];
  return tools.map((tool) => (typeof tool === 'string' ? tool : tool.name)).filter(Boolean).sort();
}

function checkJsonFiles() {
  for (const file of walk(packageDir, (path) => path.endsWith('.json'))) {
    readJson(file);
  }
}

function headingSection(text, heading) {
  const escaped = heading.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  const match = text.match(new RegExp(`^##\\s+${escaped}\\b[\\s\\S]*?(?=^##\\s+|(?![\\s\\S]))`, 'mi'));
  return match?.[0] ?? '';
}

function countNumberedChecks(text) {
  const checks = headingSection(text, 'Checks');
  return (checks.match(/^\s*\d+\.\s+/gm) ?? []).length;
}

function countWorkflowBullets(text) {
  const workflows = headingSection(text, 'Workflows');
  return (workflows.match(/^\s*-\s+/gm) ?? []).length;
}

const SMART_PATTERNS = {
  scheme: /tools\s+[A-Za-z0-9]+\s+--scheme|`tools\s+[A-Za-z0-9]+\s+--scheme`/i,
  checks: /##\s+Checks/i,
  workflows: /##\s+Workflows/i,
  pagination: /pagination|page\s*2|next\.|hasMore|charOffset|itemsPerPage|continuation|continue/i,
  honestFailure: /honest|empty|404|unsupported|diagnostic|false proof|absence|misspell|not found|error/i,
  differentiator: /differentiat|AST|structural|LSP|semantic|symbols|minify|matchString|clone|dead\s*code|repositoryDirectory|path|history|rate.limit|PR|issue|commit|structure|cache|workflow/i,
};

function getQuestionCountContract() {
  const schema = readJson(join(packageDir, 'benchmark', 'schemas', 'questions-input.schema.json'));
  const questionSchema = schema?.properties?.questions;
  return {
    min: questionSchema?.minItems ?? 1,
    max: questionSchema?.maxItems ?? Number.POSITIVE_INFINITY,
  };
}

function describeCountRange({ min, max }) {
  return min === max ? `${min}` : `${min}-${Number.isFinite(max) ? max : '∞'}`;
}

function getToolParamNames(tool) {
  let schema;
  try {
    schema = JSON.parse(runCli(['tools', tool, '--scheme', '--json'], { timeout: 60_000 }));
  } catch (error) {
    fail(`Live scheme JSON failed for ${tool}: ${error.message}`);
    return [];
  }
  const queryProps = schema?.inputSchema?.properties?.queries?.items?.properties;
  if (!queryProps || typeof queryProps !== 'object') {
    fail(`Live scheme for ${tool} does not expose queries.items.properties`);
    return [];
  }
  const harnessFields = new Set(['id', 'mainResearchGoal', 'researchGoal', 'reasoning']);
  return Object.keys(queryProps).filter((name) => !harnessFields.has(name));
}

function mentionsParam(text, param) {
  return new RegExp(`(?<![A-Za-z0-9_-])${param.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}(?![A-Za-z0-9_-])`).test(text);
}

function checkToolDocs(activeTools) {
  const perToolDir = join(packageDir, 'benchmark', 'per-tool');
  const docFiles = walk(perToolDir, (path) => path.endsWith('.md') && !path.endsWith('README.md'));
  const docsByTool = new Map(docFiles.map((path) => [path.slice(0, -3).split('/').pop(), path]));

  for (const tool of activeTools) {
    const path = docsByTool.get(tool);
    if (!path) {
      fail(`Active tool ${tool} has no per-tool benchmark doc under ${rel(perToolDir)}`);
      continue;
    }

    const text = readText(path);
    const checks = countNumberedChecks(text);
    const workflows = countWorkflowBullets(text);
    const missing = [];
    const missingParams = getToolParamNames(tool).filter((param) => !mentionsParam(text, param));

    if (!new RegExp(`^#\\s+${tool}\\b`, 'm').test(text)) missing.push('title');
    if (!SMART_PATTERNS.scheme.test(text)) missing.push('scheme command');
    if (!SMART_PATTERNS.checks.test(text)) missing.push('Checks section');
    if (!SMART_PATTERNS.workflows.test(text)) missing.push('Workflows section');
    if (!SMART_PATTERNS.pagination.test(text)) missing.push('pagination/continuation check');
    if (!SMART_PATTERNS.honestFailure.test(text)) missing.push('honest-failure/absence check');
    if (!SMART_PATTERNS.differentiator.test(text)) missing.push('differentiating capability check');

    if (checks < 4) warn(`${rel(path)} has only ${checks} checks; per-tool README recommends enough checks to cover happy path, differentiator, pagination, and honest failure`);
    if (workflows < 2) warn(`${rel(path)} has only ${workflows} workflow(s); per-tool README recommends multiple multi-tool chains`);

    if (missingParams.length) {
      missing.push(`schema params not documented: ${missingParams.join(', ')}`);
    }

    if (missing.length) {
      fail(`${rel(path)} misses required benchmark content: ${missing.join(', ')}`);
    }
  }

  for (const [tool, path] of docsByTool) {
    if (activeTools.includes(tool)) continue;
    const text = readText(path);
    const clearlyNonActive = /deprecated|superseded|opt-in|env-gated|gated|legacy|ENABLE_/i.test(text);
    if (!clearlyNonActive) {
      fail(`${rel(path)} documents non-active tool ${tool} but is not marked deprecated/gated/legacy`);
    }
  }
}

function checkLiveSchemes(activeTools) {
  for (const tool of activeTools) {
    try {
      const output = runCli(['tools', tool, '--scheme', '--compact'], { timeout: 60_000 });
      if (!output.includes(tool) || output.trim().length < 80) {
        fail(`Live scheme for ${tool} returned an unexpected/empty response`);
      }
    } catch (error) {
      fail(`Live scheme failed for ${tool}: ${error.message}`);
    }
  }
}

function questionEntries(groundTruth) {
  const questions = groundTruth?.questions;
  if (Array.isArray(questions)) return questions.map((q, index) => [`q${q.q ?? index + 1}`, q]);
  if (questions && typeof questions === 'object') return Object.entries(questions);
  return [];
}

function mdQuestionNumbers(text) {
  return [...text.matchAll(/^\s*(?:##\s*)?Q(\d+)\b/gim)].map((match) => Number(match[1]));
}

function hasOracle(q) {
  return Boolean(q?.oracle || q?.answer);
}

function hasScoring(q, suiteTruth) {
  return Boolean(q?.scoring || suiteTruth?.sharedScoring);
}

function hasCapability(q) {
  return Boolean(q?.capabilityPoint || q?.astGrepCeiling || q?.lane || q?.criticalKeyTerm || (Array.isArray(q?.keyTerms) && q.keyTerms.length > 0));
}

function checkCompareSuites() {
  const compareDir = join(packageDir, 'benchmark', 'compare');
  const suites = readdirSync(compareDir)
    .map((entry) => join(compareDir, entry))
    .filter((path) => statSync(path).isDirectory())
    .sort();

  if (suites.length === 0) fail(`No compare suites found under ${rel(compareDir)}`);

  const questionCount = getQuestionCountContract();

  for (const suiteDir of suites) {
    const suiteName = suiteDir.split('/').pop();
    const readmePath = join(suiteDir, 'README.md');
    const questionsPath = join(suiteDir, 'questions.md');
    const truthPath = join(suiteDir, 'ground-truth.json');

    for (const path of [readmePath, questionsPath, truthPath]) {
      if (!existsSync(path)) fail(`${suiteName} missing ${rel(path)}`);
    }
    if (!existsSync(questionsPath) || !existsSync(truthPath)) continue;

    const mdQuestions = mdQuestionNumbers(readText(questionsPath));
    if (mdQuestions.length < questionCount.min || mdQuestions.length > questionCount.max) {
      fail(`${rel(questionsPath)} must contain ${describeCountRange(questionCount)} Q headings per questions-input schema; found ${mdQuestions.length}`);
    }
    const expectedMdQuestions = Array.from({ length: mdQuestions.length }, (_, index) => index + 1);
    if (mdQuestions.some((q, index) => q !== expectedMdQuestions[index])) {
      fail(`${rel(questionsPath)} Q headings must be sequential from Q1; found ${mdQuestions.join(', ')}`);
    }

    const truth = readJson(truthPath);
    const entries = questionEntries(truth);
    if (entries.length < questionCount.min || entries.length > questionCount.max) {
      fail(`${rel(truthPath)} must contain ${describeCountRange(questionCount)} ground-truth questions per questions-input schema; found ${entries.length}`);
    }
    if (!/judge-only|solvers must never read/i.test(truth?.note ?? '')) {
      fail(`${rel(truthPath)} must start with a judge-only / solver-blind note`);
    }
    if (!truth?.verification && !truth?.verificationCaveat) {
      fail(`${rel(truthPath)} must include verification or verificationCaveat`);
    }

    const truthQuestionNumbers = entries.map(([qid]) => Number(String(qid).replace(/^q/i, ''))).sort((a, b) => a - b);
    if (truthQuestionNumbers.some((q, index) => q !== expectedMdQuestions[index])) {
      fail(`${rel(truthPath)} question keys must match questions.md Q1..Q${mdQuestions.length}; found q${truthQuestionNumbers.join(', q')}`);
    }

    const suiteTargetTools = new Set();
    for (const [qid, q] of entries) {
      if (!hasOracle(q)) fail(`${rel(truthPath)} ${qid} missing oracle/answer`);
      if (!hasScoring(q, truth)) fail(`${rel(truthPath)} ${qid} missing scoring/sharedScoring`);
      if (!hasCapability(q)) fail(`${rel(truthPath)} ${qid} missing capability/trajectory marker`);
      for (const targetTool of q?.targetTools ?? []) suiteTargetTools.add(targetTool);
    }

    if (suiteName === 'octocode-mcp-vs-cli') {
      const activeTools = getActiveTools();
      const missingTools = activeTools.filter((tool) => !suiteTargetTools.has(tool));
      const staleTools = [...suiteTargetTools].filter((tool) => !activeTools.includes(tool));
      if (missingTools.length) fail(`${rel(truthPath)} must cover every active tool in targetTools; missing ${missingTools.join(', ')}`);
      if (staleTools.length) fail(`${rel(truthPath)} targetTools includes non-active tools: ${staleTools.join(', ')}`);
    }
  }
}

function checkSchemas() {
  const schemasDir = join(packageDir, 'benchmark', 'schemas');
  const required = [
    'questions-input.schema.json',
    'solver-output.schema.json',
    'ground-truth.schema.json',
    'kpi.schema.json',
  ];

  for (const name of required) {
    const path = join(schemasDir, name);
    if (!existsSync(path)) {
      fail(`Missing schema ${rel(path)}`);
      continue;
    }
    const schema = readJson(path);
    if (!schema) continue;
    if (schema.$schema !== 'https://json-schema.org/draft/2020-12/schema') {
      fail(`${rel(path)} must declare JSON Schema draft 2020-12`);
    }
    if (!schema.$id || !schema.title || schema.type !== 'object') {
      fail(`${rel(path)} must include $id, title, and type:"object"`);
    }
  }
}

function checkMarkdownLinks() {
  const mdFiles = walk(packageDir, (path) => path.endsWith('.md'));
  for (const file of mdFiles) {
    const text = readText(file);
    const links = [...text.matchAll(/\[[^\]]+\]\(([^)]+)\)/g)].map((match) => match[1]);
    for (const link of links) {
      if (/^(https?:|mailto:|#)/i.test(link)) continue;
      const clean = link.split('#')[0];
      if (!clean) continue;
      const target = resolve(dirname(file), clean);
      if (!existsSync(target)) {
        if (clean.startsWith('output/') || clean.includes('/output/')) {
          warn(`${rel(file)} references gitignored run artifact ${clean}; keep a fixture for CI if used as an exemplar`);
        } else {
          fail(`${rel(file)} has broken local link: ${link}`);
        }
      }
    }
  }
}

function main() {
  checkJsonFiles();
  const activeTools = getActiveTools();
  notes.push(`active tools: ${activeTools.length} (${activeTools.join(', ')})`);
  if (activeTools.length) {
    checkToolDocs(activeTools);
    checkLiveSchemes(activeTools);
  }
  checkCompareSuites();
  checkSchemas();
  checkMarkdownLinks();

  for (const note of notes) console.log(`note: ${note}`);
  for (const warning of warnings) console.warn(`warning: ${warning}`);

  if (errors.length) {
    console.error('\nBenchmark verification failed:');
    for (const error of errors) console.error(`- ${error}`);
    process.exit(1);
  }

  console.log('Benchmark verification passed: tool coverage, live schemes/params, required per-tool content, compare question shape/differentiators, MCP-vs-CLI all-tool coverage, schemas, and links are sane.');
}

main();
