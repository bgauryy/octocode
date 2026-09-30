#!/usr/bin/env node

import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import {
  DIRECT_TOOL_DEFINITIONS,
  TOOL_NAMES,
  prepareDirectToolInput,
} from '@octocodeai/config/schema';
import { DEFAULT_CONFIG } from '@octocodeai/config';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const ROOT = path.resolve(__dirname, '..', '..', '..');
const REPO_BLOB_PREFIXES = ['octocode', 'octocode-mcp'].map(repo => `https://github.com/bgauryy/${repo}/blob/main/`);
const PUBLIC_TOOL_NAMES = DIRECT_TOOL_DEFINITIONS.map(
  definition => definition.name
);
const DISCOVERABLE_TOOL_COUNT = PUBLIC_TOOL_NAMES.length;
// Mirrors native `ToolId::is_beta` (crates/runtime/src/tools/id.rs): hidden
// unless OCTOCODE_BETA / local.beta is enabled.
const BETA_TOOL_NAMES = new Set([TOOL_NAMES.AST_TOPOLOGY, TOOL_NAMES.AST_REWRITE]);
const DEFAULT_TOOL_NAMES = PUBLIC_TOOL_NAMES.filter(name => {
  if (name === TOOL_NAMES.CLASIFY) return false;
  if (BETA_TOOL_NAMES.has(name)) return DEFAULT_CONFIG.local.beta;
  // Mirrors native `is_available`: ghCloneRepo is CLI-only, never in the MCP catalog.
  return name !== TOOL_NAMES.GITHUB_CLONE_REPO;
});
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

function listPackageRoots() {
  const packagesRoot = path.join(ROOT, 'packages');
  const packageRoots = fs
    .readdirSync(packagesRoot, { withFileTypes: true })
    .filter(entry => entry.isDirectory())
    .map(entry => path.join(packagesRoot, entry.name));
  const nativePackageRoots = packageRoots.flatMap(packageRoot => {
    const npmRoot = path.join(packageRoot, 'npm');
    if (!fs.existsSync(npmRoot)) return [];
    return fs
      .readdirSync(npmRoot, { withFileTypes: true })
      .filter(entry => entry.isDirectory())
      .map(entry => path.join(npmRoot, entry.name));
  });

  return [...packageRoots, ...nativePackageRoots].filter(packageRoot =>
    fs.existsSync(path.join(packageRoot, 'package.json'))
  );
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

  const packageRoots = listPackageRoots();
  const entryDocs = [ROOT, ...packageRoots].flatMap(dir => fs.readdirSync(dir, { withFileTypes: true }).filter(entry => entry.isFile() && entry.name.endsWith('.md')).map(entry => path.join(dir, entry.name)));
  const docFiles = [...new Set([...DOC_ROOTS.flatMap(collectMarkdownFiles), ...entryDocs])];
  for (const filePath of docFiles) {
    {
      const content = fs.readFileSync(filePath, 'utf8');
      for (const linkTarget of readMarkdownLinks(content)) {
        if (
          linkTarget.startsWith('#') ||
          linkTarget.startsWith('mailto:') ||
          linkTarget.startsWith('tel:') ||
          ( /^[a-z][a-z0-9+.-]*:/i.test(linkTarget) && !linkTarget.startsWith('https://') )
        ) {
          continue;
        }

        if (!linkTarget.startsWith('https://')) {
          const relativeTarget = decodeURIComponent(linkTarget.split('#')[0]);
          if (!fs.existsSync(path.resolve(path.dirname(filePath), relativeTarget))) {
            failures.push(`${path.relative(ROOT, filePath)} points to missing relative file "${linkTarget}"`);
          }
          continue;
        }

        const repoPrefix = REPO_BLOB_PREFIXES.find(prefix => linkTarget.startsWith(prefix));
        if (repoPrefix) {
          const relativeTarget = linkTarget
            .slice(repoPrefix.length)
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

function validatePackageReadmes() {
  const failures = [];
  const rootReadme = fs.readFileSync(path.join(ROOT, 'README.md'), 'utf8');
  const ownersByContent = new Map();
  const checkScripts = (manifest, manifestPath) => {
    for (const [scriptName, command] of Object.entries(manifest.scripts ?? {})) {
      if (scriptName === 'readme:sync' || command.includes('sync-package-readmes')) {
        failures.push(
          `${manifestPath} retains removed README synchronization in scripts.${scriptName}`
        );
      }
    }
  };

  for (const packageRoot of listPackageRoots()) {
    const packagePath = path.relative(ROOT, packageRoot);
    const readmePath = path.join(packageRoot, 'README.md');
    if (!fs.existsSync(readmePath)) {
      failures.push(`${packagePath} is missing its package-owned README.md`);
      continue;
    }

    const readme = fs.readFileSync(readmePath, 'utf8');
    if (readme === rootReadme) {
      failures.push(`${packagePath}/README.md duplicates the root README.md`);
    }
    const priorOwner = ownersByContent.get(readme);
    if (priorOwner) {
      failures.push(`${packagePath}/README.md duplicates ${priorOwner}/README.md`);
    } else {
      ownersByContent.set(readme, packagePath);
    }

    const manifest = JSON.parse(
      fs.readFileSync(path.join(packageRoot, 'package.json'), 'utf8')
    );
    checkScripts(manifest, `${packagePath}/package.json`);
  }

  const rootManifest = JSON.parse(
    fs.readFileSync(path.join(ROOT, 'package.json'), 'utf8')
  );
  checkScripts(rootManifest, 'package.json');

  if (fs.existsSync(path.join(ROOT, 'scripts', 'sync-package-readmes.mjs'))) {
    failures.push('scripts/sync-package-readmes.mjs must not overwrite package-owned READMEs');
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
  const toolReferencePath = path.join(ROOT, 'docs', 'OCTOCODE_TOOLS.md');
  const toolReference = fs.readFileSync(toolReferencePath, 'utf8');
  const agentsGuide = fs.readFileSync(path.join(ROOT, 'AGENTS.md'), 'utf8');
  const cliArchitecture = fs.readFileSync(
    path.join(ROOT, 'packages', 'octocode', 'ARCHITECTURE.md'),
    'utf8'
  );
  const scriptsGuide = fs.readFileSync(
    path.join(ROOT, 'skills-dev', 'octocode-dev', 'scripts', 'README.md'),
    'utf8'
  );

  if (configReference.includes('enableAdditional')) {
    failures.push(
      'docs/CONFIGURATION.md documents unsupported tools.enableAdditional'
    );
  }

  for (const toolName of PUBLIC_TOOL_NAMES) {
    if (!toolReference.includes(`\`${toolName}\``)) {
      failures.push(
        `${path.relative(ROOT, toolReferencePath)} is missing public tool \"${toolName}\"`
      );
    }
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
        `| MCP, no flags | ${DEFAULT_TOOL_COUNT} of ${DISCOVERABLE_TOOL_COUNT} |`,
        `| CLI, no flags | ${DISCOVERABLE_TOOL_COUNT} discoverable |`,
        '| `ghSearchRepo` |',
        '| `ghSearchCode` |',
        '| `ghStructure` |',
      ],
      forbidden: [
        '**15 tools in the full discovery catalog.**',
        '| `ghSearch` |',
        '| `ghSearchRepos` |',
        '| `ghViewRepoStructure` |',
      ],
    },
    {
      file: 'docs/OCTOCODE_TOOLS.md',
      required: [
        '### `ghSearchRepo`',
        '### `ghSearchCode`',
        '### `ghStructure`',
      ],
      forbidden: [
        '### `ghSearch`\n',
        'ghSearch(operation:',
        '`operation: "repositories"`',
      ],
    },
    {
      file: 'skills-dev/octocode-dev/docs/TOOL_QUALITY.md',
      required: [
        `${DISCOVERABLE_TOOL_COUNT} tools`,
        `${DEFAULT_TOOL_COUNT} enabled by default`,
        '`ghSearchRepo`, `ghSearchCode`, `ghStructure`',
        'code, repository, and tree variants',
      ],
      forbidden: [
        'all 15 tools',
        'ghSearchRepos / artifactSearch',
        '-> ghViewRepoStructure',
        '| `ghSearch` |',
      ],
    },
    {
      file: 'docs/OCTOCODE_RESEARCH_MANIFEST.md',
      required: [
        '`ghSearchRepo`',
        '`ghSearchCode`',
        '`ghStructure`',
      ],
      forbidden: ['ghSearch(operation:', 'ghSearchRepos', 'ghViewRepoStructure'],
    },
    {
      file: 'packages/octocode/docs/OCTOCODE_CLI.md',
      required: ['| GitHub | `ghSearchRepo`', 'octocode ghSearchCode', 'octocode ghStructure'],
      forbidden: [
        'octocode ghSearch \'',
        'octocode ghViewRepoStructure',
      ],
    },
    {
      file: 'packages/octocode-benchmark/skills/octocode-benchmark/references/primer-octocode.md',
      required: ['| `ghSearchRepo`', 'octocode ghSearchRepo'],
      forbidden: ['"operation":"repositories"', 'ghSearchRepos', 'ghViewRepoStructure'],
    },
    {
      file: 'packages/octocode-benchmark/skills/octocode-benchmark/references/run-preflight.md',
      required: ['ghSearchRepo'],
      forbidden: ['"operation":"repositories"', 'ghSearchRepos', 'ghViewRepoStructure'],
    },
    {
      file: 'packages/octocode-benchmark/skills/octocode-benchmark/scripts/check-prereqs.sh',
      required: ['ghSearchRepo'],
      forbidden: ['"operation":"repositories"', 'ghSearchRepos', 'ghViewRepoStructure'],
    },
    {
      file: 'skills/octocode-research/references/octocode.md',
      required: [
        `## ${DISCOVERABLE_TOOL_COUNT} public tools`,
        `The default catalog contains ${DEFAULT_TOOL_COUNT} tools`,
        '| GitHub code / tree / repositories | `ghSearchCode` / `ghStructure` / `ghSearchRepo` |',
      ],
      forbidden: [],
    },
    {
      file: 'skills/octocode-research/references/workflow-external.md',
      required: ['`ghSearchRepo`', '`ghSearchCode`', '`ghStructure`'],
      forbidden: ['ghSearch operation:', 'ghSearchRepos', 'ghViewRepoStructure'],
    },
    {
      file: 'skills/octocode-research/references/workflow-combination.md',
      required: ['`ghSearchRepo`', '`ghStructure`'],
      forbidden: ['ghSearch operation:', 'ghSearchRepos', 'ghViewRepoStructure'],
    },
    {
      file: 'docs/CONFIGURATION.md',
      required: ['(e.g. `["ghSearchCode","localSearch"]`)'],
      forbidden: ['(e.g. `["ghSearch","localSearch"]`)'],
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
    ...validatePackageReadmes(),
    ...validateWorkflowReadme(),
    ...validateDocumentationContracts(),
    ...validatePrimaryToolGuidance(),
    ...validateToolExamples(),
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

function validateToolExamples() {
  const failures = [];
  const content = fs.readFileSync(path.join(ROOT, 'docs/OCTOCODE_TOOLS.md'), 'utf8');
  const sections = content.split(/^### /m);
  for (const section of sections) {
    const name = section.match(/^`(\w+)`/)?.[1];
    if (!PUBLIC_TOOL_NAMES.includes(name)) continue;
    for (const [, json] of section.matchAll(/```json\s*\n([\s\S]*?)```/g)) {
      let examples;
      try {
        try { examples = [JSON.parse(json)]; }
        catch { examples = json.trim().split('\n').filter(line => line.trim()).map(line => JSON.parse(line)); }
        for (const example of examples) prepareDirectToolInput(name, example);
      } catch (error) {
        failures.push(`docs/OCTOCODE_TOOLS.md ${name} example: ${error.message}; ${(error.details ?? []).join('; ')}`);
      }
    }
  }
  return failures;
}

main();
