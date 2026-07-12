#!/usr/bin/env node
// Offline benchmark gate for the agent-facing Octocode CLI surface.
//
// This validates that canonical octocode-core metadata is present and that the
// built CLI renders it through help, context, raw tool schemes, and OQL scheme.
// It intentionally avoids network/auth/tool execution so it can run in CI.

import { spawnSync } from 'node:child_process';
import { existsSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { completeMetadata } from '@octocodeai/octocode-core';
import { COMMAND_SPECS } from '@octocodeai/octocode-core/cli';
import {
  DIRECT_TOOL_DEFINITIONS,
  getDirectToolDisplayFields,
} from '@octocodeai/octocode-tools-core/schema';

const here = dirname(fileURLToPath(import.meta.url));
const benchmarkRoot = resolve(here, '..');
const packageRoot = resolve(benchmarkRoot, '..');
const repoRoot = resolve(packageRoot, '..', '..');
const cliPath = join(repoRoot, 'packages', 'octocode', 'out', 'octocode.js');

const failures = [];
let commandCount = 0;

function fail(message) {
  failures.push(message);
}

function assert(condition, message) {
  if (!condition) fail(message);
}

function normalize(text) {
  return String(text).replace(/\s+/g, ' ').trim();
}

function snippet(text, length = 96) {
  return normalize(text).slice(0, length);
}

function includesNormalized(haystack, needle) {
  return normalize(haystack).includes(normalize(needle));
}

function runCli(args, extraEnv = {}) {
  commandCount += 1;
  const result = spawnSync(process.execPath, [cliPath, ...args], {
    cwd: repoRoot,
    encoding: 'utf8',
    env: {
      ...process.env,
      ...extraEnv,
      NO_COLOR: '1',
      OCTOCODE_NO_STALE_BUILD_WARNING:
        process.env.OCTOCODE_NO_STALE_BUILD_WARNING ?? '1',
    },
    maxBuffer: 16 * 1024 * 1024,
  });

  const command = `octocode ${args.join(' ')}`;
  if (result.error) {
    fail(`${command}: ${result.error.message}`);
    return '';
  }
  if (result.status !== 0) {
    fail(
      `${command}: exit ${result.status}\nstdout:\n${result.stdout}\nstderr:\n${result.stderr}`
    );
    // Don't hand corrupt/partial stdout to downstream parsers — a non-zero exit
    // already recorded a failure; returning '' makes follow-on checks fail
    // cleanly instead of asserting against truncated output.
    return '';
  }
  return result.stdout;
}

function parseJsonFromCli(args) {
  const output = runCli(args);
  try {
    return JSON.parse(output);
  } catch (error) {
    fail(
      `octocode ${args.join(' ')} did not emit valid JSON: ${error.message}`
    );
    return null;
  }
}

function validateCanonicalToolMetadata() {
  assert(
    typeof completeMetadata.systemPrompt === 'string' &&
      completeMetadata.systemPrompt.length > 300,
    'completeMetadata.systemPrompt must contain agent instructions'
  );
  for (const phrase of [
    'Start broad, go specific',
    'Before raw calls, read the schema',
    'Failures are signals',
  ]) {
    assert(
      completeMetadata.systemPrompt.includes(phrase),
      `system prompt is missing required guidance: ${phrase}`
    );
  }

  assert(
    completeMetadata.baseSchema &&
      typeof completeMetadata.baseSchema === 'object',
    'completeMetadata.baseSchema must be defined'
  );
  for (const field of ['id', 'mainResearchGoal', 'researchGoal', 'reasoning']) {
    assert(
      typeof completeMetadata.baseSchema?.[field] === 'string' &&
        completeMetadata.baseSchema[field].trim().length > 0,
      `baseSchema.${field} must have a description`
    );
  }

  const namesFromConstants = Object.values(completeMetadata.toolNames ?? {});
  const toolNames = Object.keys(completeMetadata.tools ?? {});
  assert(toolNames.length > 0, 'completeMetadata.tools must not be empty');
  assert(
    new Set(namesFromConstants).size === namesFromConstants.length,
    'completeMetadata.toolNames must not contain duplicate tool names'
  );
  assert(
    JSON.stringify([...namesFromConstants].sort()) ===
      JSON.stringify([...toolNames].sort()),
    'completeMetadata.toolNames and completeMetadata.tools must list the same tools'
  );

  for (const toolName of toolNames) {
    const tool = completeMetadata.tools[toolName];
    assert(tool.name === toolName, `${toolName}: metadata.name must match key`);
    assert(
      typeof tool.type === 'string' && tool.type.trim().length > 0,
      `${toolName}: type is required`
    );
    assert(
      typeof tool.shortDescription === 'string' &&
        tool.shortDescription.trim().length > 20,
      `${toolName}: shortDescription is required`
    );
    assert(
      typeof tool.instructions === 'string' &&
        tool.instructions.trim().length > 40,
      `${toolName}: instructions are required`
    );
    assert(
      typeof tool.description === 'string' &&
        tool.description.includes(tool.shortDescription),
      `${toolName}: description must include the canonical shortDescription`
    );
    assert(
      tool.schema &&
        typeof tool.schema === 'object' &&
        Object.keys(tool.schema).length > 0,
      `${toolName}: schema descriptions are required`
    );
    for (const [field, description] of Object.entries(tool.schema ?? {})) {
      assert(
        typeof description === 'string' && description.trim().length > 0,
        `${toolName}: schema field ${field} must have a description`
      );
    }
  }

  return toolNames;
}

function validateCanonicalCommandSpecs() {
  assert(
    Array.isArray(COMMAND_SPECS) && COMMAND_SPECS.length > 0,
    'COMMAND_SPECS must not be empty'
  );
  const commandNames = COMMAND_SPECS.map(spec => spec.name);
  assert(
    new Set(commandNames).size === commandNames.length,
    'COMMAND_SPECS must not contain duplicate commands'
  );

  for (const spec of COMMAND_SPECS) {
    assert(spec.name, 'command spec name is required');
    assert(
      typeof spec.description === 'string' &&
        spec.description.trim().length > 0,
      `${spec.name}: description is required`
    );
    assert(
      typeof spec.usage === 'string' && spec.usage.includes(spec.name),
      `${spec.name}: usage must mention the command`
    );
    assert(
      Array.isArray(spec.scheme) && spec.scheme.length > 0,
      `${spec.name}: scheme entries are required`
    );
    assert(
      spec.whenToUse === undefined || Array.isArray(spec.whenToUse),
      `${spec.name}: whenToUse must be an array when present`
    );
    assert(
      spec.examples === undefined || Array.isArray(spec.examples),
      `${spec.name}: examples must be an array when present`
    );
    assert(
      Array.isArray(spec.options),
      `${spec.name}: options must be an array`
    );

    for (const line of spec.scheme) {
      assert(
        typeof line === 'string' && line.trim().length > 0,
        `${spec.name}: each scheme line must be descriptive`
      );
    }
    for (const option of spec.options) {
      assert(
        typeof option.name === 'string' && option.name.trim().length > 0,
        `${spec.name}: option names are required`
      );
      assert(
        typeof option.description === 'string' &&
          option.description.trim().length > 0,
        `${spec.name}: --${option.name} must have a description`
      );
    }
  }

  return commandNames;
}

function validateCliToolSurfaces(toolNames) {
  const liveToolNames = DIRECT_TOOL_DEFINITIONS.map(tool => tool.name);
  for (const directToolName of liveToolNames) {
    assert(
      toolNames.includes(directToolName),
      `live direct tool ${directToolName} must exist in canonical tool metadata`
    );
  }

  const mainHelp = runCli(['--help', '--no-color']);
  assert(
    mainHelp.includes('<AGENT_INSTRUCTIONS>'),
    'main help must include AGENT_INSTRUCTIONS'
  );
  assert(
    mainHelp.includes(`TOOLS (${liveToolNames.length})`),
    'main help must show the live tool count'
  );
  assert(
    mainHelp.includes('tools <name> --scheme'),
    'main help must tell agents to read schemes'
  );
  assert(
    mainHelp.includes('context [--full] [--json]'),
    'main help must expose context'
  );

  const toolsList = runCli(['tools', '--no-color']);
  assert(
    toolsList.includes(`Octocode Tools (${liveToolNames.length})`),
    'tools list must show the live tool count'
  );
  assert(
    toolsList.includes('name + concise description'),
    'tools list must use the concise default catalog format'
  );
  for (const toolName of liveToolNames) {
    assert(
      new RegExp(`\\n\\s+${toolName}\\s+`).test(toolsList),
      `tools list must include ${toolName}`
    );
  }
  assert(
    toolsList.includes('Search code contents or file paths'),
    'tools list must include concise tool descriptions'
  );
  assert(
    !new RegExp('\\n\\s+localSearchCode\\s+\\[').test(toolsList),
    'tools list must not show schema field signatures in the default catalog'
  );
  assert(
    toolsList.includes('tools <name> --scheme') &&
      toolsList.includes("tools <name> --queries '<json>' --compact") &&
      toolsList.includes('tools --json --compact'),
    'tools list must expose schema, run, and machine-catalog follow-up commands'
  );

  const fullToolCatalog = parseJsonFromCli([
    'tools',
    '--json',
    '--full',
    '--no-color',
  ]);
  assert(
    fullToolCatalog?.kind === 'octocode.toolCatalog.full',
    'tools --json --full must emit the full catalog wrapper'
  );
  assert(
    fullToolCatalog?.toolCount === liveToolNames.length &&
      Array.isArray(fullToolCatalog?.tools) &&
      fullToolCatalog.tools.length === liveToolNames.length,
    'tools --json --full must include every live tool'
  );
  for (const toolName of liveToolNames) {
    const entry = fullToolCatalog.tools.find(tool => tool?.name === toolName);
    assert(entry, `tools --json --full must include ${toolName}`);
    assert(
      entry?.inputSchema?.type === 'object',
      `tools --json --full must include ${toolName} inputSchema`
    );
    assert(
      Array.isArray(entry?.fields) &&
        entry.fields.some(field => typeof field?.description === 'string'),
      `tools --json --full must include ${toolName} field descriptions`
    );
  }

  const contextJson = parseJsonFromCli([
    'context',
    '--full',
    '--json',
    '--no-color',
  ]);
  const context = contextJson?.context ?? '';
  assert(
    typeof context === 'string' && context.length > 1000,
    'context --full --json must return a non-empty context string'
  );
  for (const phrase of [
    'SCHEMA CHECK',
    'Agent System Prompt',
    'Output contract',
    'Tools (grouped by source)',
    'Schemas are not shown here',
  ]) {
    assert(context.includes(phrase), `context output is missing: ${phrase}`);
  }
  for (const toolName of toolNames) {
    const tool = completeMetadata.tools[toolName];
    assert(
      context.includes(toolName),
      `context output must include ${toolName}`
    );
    assert(
      includesNormalized(context, tool.shortDescription),
      `context output must include ${toolName} shortDescription: ${snippet(tool.shortDescription)}`
    );
  }

  for (const toolName of liveToolNames) {
    const scheme = runCli([
      'tools',
      toolName,
      '--scheme',
      '--compact',
      '--no-color',
    ]);
    const tool = completeMetadata.tools[toolName];
    assert(
      scheme.includes(toolName),
      `${toolName}: scheme output must include tool name`
    );
    assert(
      includesNormalized(scheme, tool.shortDescription),
      `${toolName}: scheme output must include canonical shortDescription`
    );
    for (const heading of [
      'Description',
      'Input Schema',
      'Output Schema',
      'Flags',
      'Example',
    ]) {
      assert(
        scheme.includes(heading),
        `${toolName}: scheme output must include ${heading}`
      );
    }
    for (const field of getDirectToolDisplayFields(toolName).map(
      displayField => displayField.name
    )) {
      assert(
        new RegExp(`\\n\\s+${field}(\\s|\\.)`).test(scheme),
        `${toolName}: scheme output must include schema field ${field}`
      );
    }
  }
}

function validateCliCommandSurfaces(commandNames) {
  const mainHelp = runCli(['--help', '--no-color']);
  // `pr` was folded into `search owner/repo#N --target pullRequests` (same
  // search-first consolidation that removed grep/lsp/ast quick commands).
  for (const commandName of ['search', 'clone', 'cache']) {
    assert(
      new RegExp(`\\n\\s+${commandName}\\s+`).test(mainHelp),
      `main help must include quick command ${commandName}`
    );
  }
  for (const commandName of ['install', 'login', 'logout', 'status']) {
    assert(
      new RegExp(`\\n\\s+${commandName}\\s+`).test(mainHelp),
      `main help must include management command ${commandName}`
    );
  }

  const specByName = new Map(COMMAND_SPECS.map(spec => [spec.name, spec]));
  for (const commandName of commandNames) {
    const spec = specByName.get(commandName);
    const help = runCli([commandName, '--help', '--no-color']);
    assert(
      help.includes(`octocode ${commandName}`),
      `${commandName}: help title is missing`
    );
    assert(help.includes('USAGE'), `${commandName}: help must include USAGE`);
    assert(help.includes('SCHEME'), `${commandName}: help must include SCHEME`);
    if (spec.whenToUse?.length) {
      assert(
        help.includes('WHEN TO USE'),
        `${commandName}: help must include WHEN TO USE`
      );
    }
    if (spec.examples?.length) {
      assert(
        help.includes('EXAMPLES'),
        `${commandName}: help must include EXAMPLES`
      );
    }
    assert(
      includesNormalized(help, spec.description),
      `${commandName}: help must include canonical description: ${snippet(spec.description)}`
    );
    for (const schemeLine of spec.scheme) {
      assert(
        includesNormalized(help, schemeLine),
        `${commandName}: help must include scheme line: ${snippet(schemeLine)}`
      );
    }
    for (const option of spec.options) {
      assert(
        help.includes(`--${option.name}`),
        `${commandName}: help must document --${option.name}`
      );
    }
  }

  const toolsHelp = runCli(['tools', '--help', '--no-color']);
  assert(
    toolsHelp.includes(`Octocode Tools (${DIRECT_TOOL_DEFINITIONS.length})`),
    'tools --help must show the concise tool catalog'
  );
  assert(
    toolsHelp.includes('tools <name> --scheme'),
    'tools --help must expose schema-read guidance'
  );
  assert(
    toolsHelp.includes('Full protocol: context'),
    'tools --help must mention context for the full protocol'
  );
}

function validateOqlScheme() {
  const searchScheme = parseJsonFromCli([
    'search',
    '--scheme',
    '--json',
    '--no-color',
  ]);
  const compactSchemeOutput = runCli([
    'search',
    '--scheme',
    '--json',
    '--compact',
    '--no-color',
  ]);
  let compactScheme = null;
  try {
    compactScheme = JSON.parse(compactSchemeOutput);
  } catch (error) {
    fail(
      `octocode search --scheme --json --compact did not emit valid JSON: ${error.message}`
    );
  }
  assert(
    searchScheme?.schema === 'oql',
    'search --scheme JSON must declare schema:"oql"'
  );
  assert(
    typeof searchScheme?.description === 'string' &&
      searchScheme.description.includes('octocode search'),
    'search --scheme JSON must include an OQL description'
  );
  assert(
    Array.isArray(searchScheme?.activeTargets) &&
      searchScheme.activeTargets.length >= 10,
    'search --scheme JSON must include active OQL targets'
  );
  assert(
    searchScheme?.activeTargets?.includes('code') &&
      searchScheme.activeTargets.includes('materialize') &&
      searchScheme.activeTargets.includes('graph'),
    'search --scheme JSON must include code, graph, and materialize targets'
  );
  assert(
    searchScheme?.quickStart && Object.keys(searchScheme.quickStart).length > 0,
    'search --scheme JSON must include quickStart recipes'
  );
  assert(
    searchScheme?.evidenceSemantics?.['answerReady:false'],
    'search --scheme JSON must explain answerReady:false'
  );

  // ── Doc-quality regression guards ───────────────────────────────────────
  // These lock in the agent-validated guidance that resolved real first-try
  // failures (a 6-agent clean-room usability study). They must not regress.
  const structural = String(searchScheme?.predicates?.structural ?? '');
  assert(
    /rule/.test(structural) && /has/.test(structural),
    'structural predicate doc must show the robust rule shape ({kind,has:{pattern}}) — fixed the #1 structural failure'
  );
  assert(
    /complete node/i.test(structural),
    'structural predicate doc must state "match the COMPLETE node" (return-type completeness)'
  );
  const fetch = String(searchScheme?.query?.fetch ?? '');
  assert(
    /fetch\.content\.match|content\.match|match\b/.test(fetch),
    'fetch doc must document content match-anchoring (NOT a top-level where) — fixed the content invalidQuery'
  );
  const ev = searchScheme?.evidenceSemantics ?? {};
  for (const ps of [
    'proofStatus:confirmed-by-lsp',
    'proofStatus:conflicting-evidence',
    'proofStatus:needs-framework-graph',
    'proofStatus:candidate',
  ]) {
    assert(ev[ps], `evidenceSemantics must define ${ps} (no undefined proofStatus values)`);
  }
  assert(
    /not a failure/i.test(String(ev['answerReady:false'] ?? '')),
    'answerReady:false must be framed as "not a failure" (pagination, not error)'
  );
  assert(
    compactScheme?.kind === 'octocode.search.compactScheme',
    'search --scheme --json --compact must emit the compact agent guide JSON'
  );
  const compactTargets = new Set(
    (compactScheme?.targets ?? []).map(entry => entry?.target)
  );
  for (const target of searchScheme?.activeTargets ?? []) {
    assert(
      compactTargets.has(target),
      `compact search scheme must include active OQL target ${target}`
    );
  }
  assert(
    compactSchemeOutput.length < JSON.stringify(searchScheme).length,
    'compact search scheme JSON must be smaller than the full OQL schema JSON'
  );

  const oqlToolScheme = runCli(
    [
      'tools',
      'oqlSearch',
      '--scheme',
      '--compact',
      '--no-color',
    ],
    { ENABLE_OQL: '1' }
  );
  for (const target of searchScheme?.activeTargets ?? []) {
    assert(
      oqlToolScheme.includes(target),
      `oqlSearch tool scheme must include active OQL target ${target}`
    );
  }
}

function planFromSearchDryRun(name, args) {
  const envelope = parseJsonFromCli([
    'search',
    ...args,
    '--dry-run',
    '--json',
    '--no-color',
  ]);
  const plan = envelope?.plan;
  assert(
    plan && typeof plan === 'object',
    `${name}: dry-run must return a plan`
  );
  // Propagate null on failure (not {}), so the caller skips the per-case
  // assertions instead of masking the root cause behind empty-array fallbacks.
  return plan && typeof plan === 'object' ? plan : null;
}

function backendKeys(plan) {
  return (plan.backendCalls ?? []).map(
    call =>
      `${call.backend}:${call.operation}:${call.exact === false ? 'approx' : 'exact'}`
  );
}

function transformerIds(plan) {
  return (plan.transformers ?? []).map(transformer => transformer.id);
}

function diagnosticCodes(plan) {
  return (plan.diagnostics ?? []).map(diagnostic => diagnostic.code);
}

function validateSearchRouteMatrix() {
  const evalDoc = 'packages/octocode-benchmark/benchmark/octocode/README.md';
  const oqlSource = 'packages/octocode-tools-core/src/oql';
  const searchCommand = 'packages/octocode/src/cli/commands/search.ts';

  const cases = [
    {
      name: 'local code text',
      args: ['executeDirectTool', 'packages/octocode/src', '--lang', 'ts'],
      target: 'code',
      backend: 'localSearchCode:searchCode:exact',
      transformer: 'local.code.textRegex',
    },
    {
      name: 'github code text',
      args: ['useState', 'vercel/next.js', '--lang', 'ts'],
      target: 'code',
      backend: 'ghSearchCode:searchCode:exact',
      transformer: 'github.code',
    },
    {
      name: 'local code structural',
      args: [
        '--pattern',
        'function $NAME($$$ARGS) { $$$BODY }',
        'packages/octocode/src',
        '--lang',
        'ts',
      ],
      target: 'code',
      backend: 'localSearchCode:searchCode:exact',
      transformer: 'local.code.structural',
    },
    {
      name: 'local files',
      args: ['test', 'packages/octocode-benchmark', '--target', 'files'],
      target: 'files',
      backend: 'localSearchCode:findFiles:exact',
      transformer: 'local.files',
    },
    {
      name: 'github files approximate',
      args: ['TODO', 'vercel/next.js', '--target', 'files'],
      target: 'files',
      backend: 'ghSearchCode:findFiles:approx',
      transformer: 'github.files',
      diagnostic: 'providerSemanticsApproximate',
    },
    {
      name: 'bare local file content',
      args: [evalDoc],
      target: 'content',
      backend: 'localGetFileContent:getContent:exact',
      transformer: 'local.content',
    },
    {
      name: 'term in local file code',
      args: ['OQL', evalDoc],
      target: 'code',
      backend: 'localSearchCode:searchCode:exact',
      transformer: 'local.code.textRegex',
    },
    {
      name: 'local structure',
      args: ['packages/octocode-benchmark', '--tree', '--depth', '2'],
      target: 'structure',
      backend: 'localViewStructure:viewStructure:exact',
      transformer: 'local.structure',
    },
    {
      name: 'local semantics',
      args: [searchCommand, '--op', 'documentSymbols'],
      target: 'semantics',
      backend: 'lspGetSemantics:getSemantics:exact',
      transformer: 'local.semantics',
    },
    {
      name: 'repositories',
      args: ['mcp server', '--target', 'repositories', '--lang', 'TypeScript'],
      target: 'repositories',
      backend: 'ghSearchRepos:searchRepos:exact',
      transformer: 'github.repositories',
    },
    {
      name: 'packages',
      args: ['zod', '--target', 'packages'],
      target: 'packages',
      backend: 'npmSearch:searchPackages:exact',
      transformer: 'npm.packages',
    },
    {
      name: 'pull requests',
      args: ['vercel/next.js', '--target', 'pullRequests', '--state', 'open'],
      target: 'pullRequests',
      backend: 'ghHistoryResearch:searchPullRequests:exact',
      transformer: 'github.pullRequests',
    },
    {
      name: 'commits',
      args: [
        'vercel/next.js/packages/next/src',
        '--target',
        'commits',
        '--since',
        '2024-01-01T00:00:00Z',
      ],
      target: 'commits',
      backend: 'ghHistoryResearch:searchCommits:exact',
      transformer: 'github.commits',
    },
    {
      name: 'local diff',
      args: [evalDoc, 'docs/OCTOCODE_QUERY_LANGUAGE.md', '--target', 'diff'],
      target: 'diff',
      backend: 'localGetFileContent:diff:exact',
      transformer: 'local.diff.directFile',
    },
    {
      name: 'research',
      args: [
        '--query',
        JSON.stringify({
          schema: 'oql',
          target: 'research',
          from: { kind: 'local', path: oqlSource },
          params: {
            intent: 'symbols',
            facets: ['symbols', 'files'],
            maxFiles: 5,
          },
          itemsPerPage: 1,
        }),
      ],
      target: 'research',
      backend: 'smartOqlResearch:runResearchFlow:approx',
      transformer: 'local.research',
    },
    {
      name: 'graph',
      args: [
        '--query',
        JSON.stringify({
          schema: 'oql',
          target: 'graph',
          from: { kind: 'local', path: oqlSource },
          params: {
            intent: 'reachability',
            facets: ['symbols'],
            proof: 'lsp',
            proofLimit: 1,
            includePackets: true,
          },
          itemsPerPage: 1,
        }),
      ],
      target: 'graph',
      backend: 'smartOqlGraph:queryRelationshipGraph:approx',
      transformer: 'local.graph',
    },
    {
      name: 'materialize',
      args: ['vercel/next.js/packages/next/src', '--target', 'materialize'],
      target: 'materialize',
      backend: 'ghCloneRepo:materialize:exact',
      transformer: 'github.materialize',
    },
  ];

  for (const testCase of cases) {
    const plan = planFromSearchDryRun(testCase.name, testCase.args);
    // planFromSearchDryRun already recorded a failure when it returns null;
    // skip the dependent assertions rather than crashing on a null plan.
    if (!plan) continue;
    assert(
      plan.normalized?.target === testCase.target,
      `${testCase.name}: expected target ${testCase.target}, got ${plan.normalized?.target}`
    );
    assert(
      backendKeys(plan).includes(testCase.backend),
      `${testCase.name}: expected backend ${testCase.backend}, got ${backendKeys(plan).join(', ')}`
    );
    assert(
      transformerIds(plan).includes(testCase.transformer),
      `${testCase.name}: expected transformer ${testCase.transformer}, got ${transformerIds(plan).join(', ')}`
    );
    if (testCase.diagnostic) {
      assert(
        diagnosticCodes(plan).includes(testCase.diagnostic),
        `${testCase.name}: expected diagnostic ${testCase.diagnostic}`
      );
    }
  }
}

if (!existsSync(cliPath)) {
  fail(`built CLI not found at ${cliPath}; run yarn build first`);
} else {
  const toolNames = validateCanonicalToolMetadata();
  const commandNames = validateCanonicalCommandSpecs();
  validateCliToolSurfaces(toolNames);
  validateCliCommandSurfaces(commandNames);
  validateOqlScheme();
  validateSearchRouteMatrix();
}

if (failures.length > 0) {
  console.error(
    `CLI metadata benchmark failed with ${failures.length} issue(s):`
  );
  for (const failure of failures) {
    console.error(`- ${failure}`);
  }
  process.exit(1);
}

console.log(
  `OK CLI metadata benchmark: ${Object.keys(completeMetadata.tools).length} tools, ${COMMAND_SPECS.length} commands, ${commandCount} CLI help/scheme checks`
);
