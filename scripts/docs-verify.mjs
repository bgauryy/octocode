#!/usr/bin/env node

import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { DIRECT_TOOL_DISCOVERY_DEFINITIONS } from '@octocodeai/octocode-tools-core/schema';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const ROOT = path.resolve(__dirname, '..');
const REPO_BLOB_PREFIX = 'https://github.com/bgauryy/octocode-mcp/blob/main/';
const PUBLIC_TOOL_NAMES = DIRECT_TOOL_DISCOVERY_DEFINITIONS.map(
  definition => definition.name
);
const DISCOVERABLE_TOOL_COUNT = PUBLIC_TOOL_NAMES.length;
const DEFAULT_TOOL_NAMES = DIRECT_TOOL_DISCOVERY_DEFINITIONS.filter(
  definition => !definition.disabled
).map(definition => definition.name);
const DEFAULT_TOOL_COUNT = DEFAULT_TOOL_NAMES.length;
const DOC_ROOTS = [
  path.join(ROOT, 'docs'),
  ...fs
    .readdirSync(path.join(ROOT, 'packages'), { withFileTypes: true })
    .filter(entry => entry.isDirectory())
    .map(entry => path.join(ROOT, 'packages', entry.name, 'docs'))
    .filter(dirPath => fs.existsSync(dirPath)),
];

function collectMarkdownFiles(rootDir) {
  const files = [];
  const queue = [rootDir];

  while (queue.length > 0) {
    const currentDir = queue.pop();
    for (const entry of fs.readdirSync(currentDir, { withFileTypes: true })) {
      const absolutePath = path.join(currentDir, entry.name);
      if (entry.isDirectory()) {
        queue.push(absolutePath);
      } else if (entry.isFile() && absolutePath.endsWith('.md')) {
        files.push(absolutePath);
      }
    }
  }

  return files.sort();
}

function readMarkdownLinks(content) {
  const links = [];
  const linkPattern = /!?\[[^\]]*]\(([^)]+)\)/g;
  let match;

  while ((match = linkPattern.exec(content)) !== null) {
    const rawTarget = match[1].trim();
    const target = rawTarget.startsWith('<') && rawTarget.endsWith('>')
      ? rawTarget.slice(1, -1)
      : rawTarget;
    links.push(target.split(/\s+/)[0]);
  }

  return links;
}

function validateDocsLinks() {
  const failures = [];

  for (const rootDir of DOC_ROOTS) {
    for (const filePath of collectMarkdownFiles(rootDir)) {
      const content = fs.readFileSync(filePath, 'utf8');
      for (const linkTarget of readMarkdownLinks(content)) {
        if (
          linkTarget.startsWith('#') ||
          linkTarget.startsWith('mailto:') ||
          linkTarget.startsWith('tel:') ||
          linkTarget.startsWith('http://')
        ) {
          continue;
        }

        if (!linkTarget.startsWith('https://')) {
          failures.push(
            `${path.relative(ROOT, filePath)} uses non-absolute link target "${linkTarget}"`
          );
          continue;
        }

        if (linkTarget.startsWith(REPO_BLOB_PREFIX)) {
          const relativeTarget = linkTarget
            .slice(REPO_BLOB_PREFIX.length)
            .split('#')[0]
            .replace(/\/$/, '');
          const absoluteTarget = path.join(ROOT, relativeTarget);
          if (!fs.existsSync(absoluteTarget)) {
            failures.push(
              `${path.relative(ROOT, filePath)} points to missing repo file "${relativeTarget}"`
            );
          }
        }
      }
    }
  }

  return failures;
}

function validateWorkflowReadme() {
  const workflowDir = path.join(ROOT, '.github', 'workflows');
  const workflowReadmePath = path.join(workflowDir, 'README.md');
  const workflowContent = fs.readFileSync(workflowReadmePath, 'utf8');
  const failures = [];
  const referencedWorkflowFiles = new Set();
  const workflowReferencePattern = /`([A-Za-z0-9._-]+\.ya?ml)`/g;
  let match;

  while ((match = workflowReferencePattern.exec(workflowContent)) !== null) {
    referencedWorkflowFiles.add(match[1]);
  }

  for (const workflowFile of referencedWorkflowFiles) {
    const absoluteWorkflowPath = path.join(workflowDir, workflowFile);
    if (!fs.existsSync(absoluteWorkflowPath)) {
      failures.push(
        `.github/workflows/README.md references missing workflow "${workflowFile}"`
      );
    }
  }

  return failures;
}

function validateDocumentationContracts() {
  const failures = [];
  const configReference = fs.readFileSync(
    path.join(ROOT, 'docs', 'CONFIGURATION.md'),
    'utf8'
  );
  const architecturePaths = [
    path.join(ROOT, 'packages', 'octocode-tools-core', 'ARCHITECTURE.md'),
    path.join(ROOT, 'packages', 'octocode-mcp', 'ARCHITECTURE.md'),
  ];
  const architectures = architecturePaths.map(filePath => ({
    filePath,
    content: fs.readFileSync(filePath, 'utf8'),
  }));
  const agentsGuide = fs.readFileSync(path.join(ROOT, 'AGENTS.md'), 'utf8');
  const cliArchitecture = fs.readFileSync(
    path.join(ROOT, 'packages', 'octocode', 'ARCHITECTURE.md'),
    'utf8'
  );
  const scriptsGuide = fs.readFileSync(
    path.join(ROOT, 'scripts', 'README.md'),
    'utf8'
  );

  if (configReference.includes('enableAdditional')) {
    failures.push(
      'docs/CONFIGURATION.md documents unsupported tools.enableAdditional'
    );
  }

  for (const { filePath, content } of architectures) {
    for (const toolName of PUBLIC_TOOL_NAMES) {
      if (!content.includes(`\`${toolName}\``)) {
        failures.push(
          `${path.relative(ROOT, filePath)} is missing public tool \"${toolName}\"`
        );
      }
    }
  }

  const obsoletePublishingClaims = [
    'workspace-only build package',
    'It is not published to npm',
  ];
  for (const claim of obsoletePublishingClaims) {
    if (architectures[0].content.includes(claim)) {
      failures.push(
        `packages/octocode-tools-core/ARCHITECTURE.md contains obsolete publishing claim \"${claim}\"`
      );
    }
  }
  if (architectures[1].content.includes('unpublished tools-core package')) {
    failures.push(
      'packages/octocode-mcp/ARCHITECTURE.md calls the published tools-core package unpublished'
    );
  }
  if (cliArchitecture.includes('workspace `devDependency`')) {
    failures.push(
      'packages/octocode/ARCHITECTURE.md incorrectly describes tools-core as a devDependency'
    );
  }

  if (agentsGuide.includes('must match the root `package.json` version')) {
    failures.push(
      'AGENTS.md incorrectly requires independently versioned packages to match the root version'
    );
  }
  if (scriptsGuide.includes('version match with root')) {
    failures.push(
      'scripts/README.md incorrectly says the publish guard enforces root-version alignment'
    );
  }

  return failures;
}

function validatePrimaryToolGuidance() {
  const failures = [];
  const publicToolNames = new Set(PUBLIC_TOOL_NAMES);
  const contracts = [
    {
      file: 'README.md',
      required: [
        `**${DISCOVERABLE_TOOL_COUNT} tools in the full discovery catalog.**`,
        `| MCP, no flags | ${DEFAULT_TOOL_COUNT} |`,
        `| CLI, no flags | ${DEFAULT_TOOL_COUNT} |`,
        '| `ghSearch` |',
      ],
      forbidden: [
        '**15 tools in the full discovery catalog.**',
        '| `ghSearchCode` |',
        '| `ghSearchRepos` |',
        '| `ghViewRepoStructure` |',
      ],
    },
    {
      file: 'docs/OCTOCODE_TOOLS.md',
      required: [
        '### `ghSearch`',
        '`operation: "code"`',
        '`operation: "repositories"`',
        '`operation: "tree"`',
      ],
      forbidden: [
        '| Find examples of a pattern | `ghSearchCode`',
        '| **GitHub** | `ghSearchCode`',
      ],
    },
    {
      file: 'docs/MCP_TOOL_QUALITY_AND_AGENT_WORKFLOW.md',
      required: [
        `${DISCOVERABLE_TOOL_COUNT} tools`,
        `${DEFAULT_TOOL_COUNT} enabled by default`,
        'ghSearch(operation:"repositories")',
        'ghSearch(operation:"tree")',
        'ghSearch(operation:"code")',
      ],
      forbidden: [
        'all 15 tools',
        'ghSearchRepos / npmSearch',
        '-> ghViewRepoStructure',
        '-> ghSearchCode(',
      ],
    },
    {
      file: 'docs/OCTOCODE_RESEARCH_MANIFEST.md',
      required: [
        'ghSearch',
        'ghSearch(operation:"repositories"',
        'ghSearch operation:"tree"',
      ],
      forbidden: ['ghSearchCode', 'ghSearchRepos', 'ghViewRepoStructure'],
    },
    {
      file: 'packages/octocode/docs/OCTOCODE_CLI.md',
      required: ['| GitHub | `ghSearch`', 'tools ghSearch --queries'],
      forbidden: [
        'tools ghSearchCode --queries',
        'tools ghViewRepoStructure --queries',
      ],
    },
    {
      file: 'packages/octocode-benchmark/skills/octocode-benchmark/references/primer-octocode.md',
      required: ['| `ghSearch` |', '"operation":"repositories"'],
      forbidden: ['ghSearchCode', 'ghSearchRepos', 'ghViewRepoStructure'],
    },
    {
      file: 'packages/octocode-benchmark/skills/octocode-benchmark/references/run-preflight.md',
      required: ['tools ghSearch --queries', '"operation":"repositories"'],
      forbidden: ['ghSearchCode', 'ghSearchRepos', 'ghViewRepoStructure'],
    },
    {
      file: 'packages/octocode-benchmark/skills/octocode-benchmark/scripts/check-prereqs.sh',
      required: ['tools ghSearch --queries', '"operation":"repositories"'],
      forbidden: ['ghSearchCode', 'ghSearchRepos', 'ghViewRepoStructure'],
    },
    {
      file: 'skills/octocode-research/references/octocode.md',
      required: [
        `${DISCOVERABLE_TOOL_COUNT} input contracts`,
        `${DEFAULT_TOOL_COUNT} tools are enabled by default`,
        '| GitHub code / tree / repositories | `ghSearch`',
      ],
      forbidden: [],
    },
    {
      file: 'skills/octocode-research/references/workflow-external.md',
      required: ['ghSearch(operation:"repositories")'],
      forbidden: ['ghSearchCode', 'ghSearchRepos', 'ghViewRepoStructure'],
    },
    {
      file: 'skills/octocode-research/references/workflow-combination.md',
      required: ['ghSearch(operation:"repositories")'],
      forbidden: ['ghSearchCode', 'ghSearchRepos', 'ghViewRepoStructure'],
    },
    {
      file: 'skills/octocode-skills/references/octocode.md',
      required: ['tools ghSearch', '"operation":"code"'],
      forbidden: ['ghSearchCode', 'ghSearchRepos', 'ghViewRepoStructure'],
    },
    {
      file: 'docs/CONFIGURATION.md',
      required: ['Example: ["ghSearch", "localSearch", "npmSearch"]'],
      forbidden: ['Example: `["ghSearchCode", "localSearch"]`'],
    },
  ];

  for (const toolName of DEFAULT_TOOL_NAMES) {
    if (!publicToolNames.has(toolName)) {
      failures.push(
        `docs verifier default-tool contract references missing public tool "${toolName}"`
      );
    }
  }

  for (const contract of contracts) {
    const content = fs.readFileSync(path.join(ROOT, contract.file), 'utf8');
    for (const requiredText of contract.required) {
      if (!content.includes(requiredText)) {
        failures.push(
          `${contract.file} is missing current tool guidance "${requiredText}"`
        );
      }
    }
    for (const forbiddenText of contract.forbidden) {
      if (content.includes(forbiddenText)) {
        failures.push(
          `${contract.file} contains stale primary tool guidance "${forbiddenText}"`
        );
      }
    }
  }

  return failures;
}

function main() {
  const failures = [
    ...validateDocsLinks(),
    ...validateWorkflowReadme(),
    ...validateDocumentationContracts(),
    ...validatePrimaryToolGuidance(),
  ];

  if (failures.length > 0) {
    console.error('Documentation verification failed:');
    for (const failure of failures) {
      console.error(`- ${failure}`);
    }
    process.exit(1);
  }

  console.log('Documentation verification passed.');
}

main();
