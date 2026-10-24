#!/usr/bin/env node

import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import {
  DIRECT_TOOL_DEFINITIONS,
  TOOL_NAMES,
  isBetaTool,
  isCliOnlyTool,
} from '@octocodeai/config/schema';
import { DEFAULT_CONFIG } from '@octocodeai/config';
import { ROOT } from './lib/workspaces.mjs';

const REPO_BLOB_PREFIXES = ['octocode', 'octocode-mcp'].map(repo => `https://github.com/bgauryy/${repo}/blob/main/`);
const PUBLIC_TOOL_NAMES = DIRECT_TOOL_DEFINITIONS.map(
  definition => definition.name
);
const DISCOVERABLE_TOOL_COUNT = PUBLIC_TOOL_NAMES.length;
// Default MCP catalog: core tool policy decides beta and CLI-only tools;
// clasify additionally needs a classification key, absent by default.
const DEFAULT_TOOL_NAMES = PUBLIC_TOOL_NAMES.filter(name => {
  if (name === TOOL_NAMES.CLASIFY) return false;
  if (isBetaTool(name) && !DEFAULT_CONFIG.local.beta) return false;
  return !isCliOnlyTool(name);
});
const DEFAULT_TOOL_COUNT = DEFAULT_TOOL_NAMES.length;
// CLI listing: every tool except beta tools while the beta gate is off, and
// clasify, which the CLI lists only with a classification key.
const CLI_DEFAULT_TOOL_COUNT = PUBLIC_TOOL_NAMES.filter(
  name =>
    name !== TOOL_NAMES.CLASIFY &&
    (!isBetaTool(name) || DEFAULT_CONFIG.local.beta)
).length;
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

// GitHub heading anchors: the rendered heading text, lowercased, with
// everything but letters, marks, numbers, `_`, `-` and spaces removed and
// spaces turned into `-`; a repeated slug gets `-1`, `-2`, ... HTML
// `<a id|name>` anchors count too. Fenced code is not a heading.
const anchorsByFile = new Map();

function headingSlug(heading) {
  return heading
    .replace(/\s+#+\s*$/, '')
    .replace(/!?\[([^\]]*)]\([^)]*\)/g, '$1')
    .replace(/<[^>]+>/g, '')
    .replace(/`|\*\*|__/g, '')
    .trim()
    .toLowerCase()
    .replace(/[^\p{L}\p{M}\p{N}\p{Pc} -]/gu, '')
    .replace(/ /g, '-');
}

function readAnchors(filePath) {
  if (anchorsByFile.has(filePath)) return anchorsByFile.get(filePath);
  const anchors = new Set();
  const slugCounts = new Map();
  let fence = null;
  for (const line of fs.readFileSync(filePath, 'utf8').split('\n')) {
    const fenceMark = line.match(/^\s*(`{3,}|~{3,})/)?.[1][0];
    if (fenceMark) {
      if (!fence) fence = fenceMark;
      else if (fence === fenceMark) fence = null;
      continue;
    }
    if (fence) continue;
    for (const [, id] of line.matchAll(/<a\s+(?:id|name)=["']([^"']+)["']/g)) anchors.add(id);
    const heading = line.match(/^#{1,6}\s+(.*)$/)?.[1];
    if (heading === undefined) continue;
    const slug = headingSlug(heading);
    const count = slugCounts.get(slug) ?? 0;
    slugCounts.set(slug, count + 1);
    anchors.add(count ? `${slug}-${count}` : slug);
  }
  anchorsByFile.set(filePath, anchors);
  return anchors;
}

// A `#fragment` into a Markdown file must name one of its anchors.
function missingAnchor(targetPath, linkTarget) {
  const hash = linkTarget.indexOf('#');
  if (hash < 0 || !targetPath.endsWith('.md') || !fs.existsSync(targetPath)) return null;
  const fragment = decodeURIComponent(linkTarget.slice(hash + 1));
  return readAnchors(targetPath).has(fragment) ? null : fragment;
}

// Ignored files are never in a clone, so a link to one cannot resolve for readers.
function gitIgnoredPaths(paths) {
  if (paths.length === 0) return new Set();
  try {
    const output = execFileSync('git', ['check-ignore', '--no-index', '--stdin'], { cwd: ROOT, input: paths.join('\n'), encoding: 'utf8' });
    return new Set(output.split('\n').filter(Boolean));
  } catch (error) {
    if (error.status === 1) return new Set(); // nothing ignored
    throw error;
  }
}

function validateDocsLinks() {
  const failures = [];
  const existingLocalLinks = [];

  const packageRoots = listPackageRoots();
  const entryDocs = [ROOT, ...packageRoots].flatMap(dir => fs.readdirSync(dir, { withFileTypes: true }).filter(entry => entry.isFile() && entry.name.endsWith('.md')).map(entry => path.join(dir, entry.name)));
  const docFiles = [...new Set([...DOC_ROOTS.flatMap(collectMarkdownFiles), ...entryDocs])];
  for (const filePath of docFiles) {
    {
      const content = fs.readFileSync(filePath, 'utf8');
      for (const linkTarget of readMarkdownLinks(content)) {
        const relativeFile = path.relative(ROOT, filePath);
        if (linkTarget.startsWith('#')) {
          const fragment = missingAnchor(filePath, linkTarget);
          if (fragment !== null) failures.push(`${relativeFile} points to missing heading "#${fragment}"`);
          continue;
        }
        if (
          linkTarget.startsWith('mailto:') ||
          linkTarget.startsWith('tel:') ||
          ( /^[a-z][a-z0-9+.-]*:/i.test(linkTarget) && !linkTarget.startsWith('https://') )
        ) {
          continue;
        }

        if (!linkTarget.startsWith('https://')) {
          const relativeTarget = decodeURIComponent(linkTarget.split('#')[0]);
          const targetPath = path.resolve(path.dirname(filePath), relativeTarget);
          if (!fs.existsSync(targetPath)) {
            failures.push(`${relativeFile} points to missing relative file "${linkTarget}"`);
          } else {
            existingLocalLinks.push({ relativeFile, linkTarget, targetPath });
            if (missingAnchor(targetPath, linkTarget) !== null) {
              failures.push(`${relativeFile} points to missing heading "${linkTarget}"`);
            }
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
            failures.push(`${relativeFile} points to missing repo file "${relativeTarget}"`);
          } else if (missingAnchor(absoluteTarget, linkTarget) !== null) {
            failures.push(`${relativeFile} points to missing heading "${linkTarget}"`);
          }
        }
      }
    }
  }

  const ignored = gitIgnoredPaths([...new Set(existingLocalLinks.map(link => link.targetPath))]);
  for (const { relativeFile, linkTarget, targetPath } of existingLocalLinks) {
    if (ignored.has(targetPath)) failures.push(`${relativeFile} points to gitignored file "${linkTarget}"`);
  }

  return failures;
}

function validatePackageReadmes() {
  const failures = [];
  const rootReadme = fs.readFileSync(path.join(ROOT, 'README.md'), 'utf8');
  const ownersByContent = new Map();

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

// Every public tool is documented, and stated tool counts match the contract.
function validateToolCatalog() {
  const failures = [];
  const toolReference = fs.readFileSync(path.join(ROOT, 'docs', 'OCTOCODE_TOOLS.md'), 'utf8');
  for (const toolName of PUBLIC_TOOL_NAMES) {
    if (!toolReference.includes(`\`${toolName}\``)) {
      failures.push(`docs/OCTOCODE_TOOLS.md is missing public tool "${toolName}"`);
    }
  }

  const counts = {
    'README.md': [
      `**${DISCOVERABLE_TOOL_COUNT} tools.**`,
      `${DEFAULT_TOOL_COUNT} tools over MCP and ${CLI_DEFAULT_TOOL_COUNT} in the CLI`,
    ],
    'skills-dev/octocode-dev/docs/TOOL_QUALITY.md': [
      `${DISCOVERABLE_TOOL_COUNT} tools`,
      `${DEFAULT_TOOL_COUNT} enabled by default`,
    ],
    'skills/common/references/octocode-usage.md': [
      `## ${DISCOVERABLE_TOOL_COUNT} public tools`,
      `The default catalog contains ${DEFAULT_TOOL_COUNT} tools`,
    ],
  };
  for (const [file, required] of Object.entries(counts)) {
    const content = fs.readFileSync(path.join(ROOT, file), 'utf8');
    for (const text of required) {
      if (!content.includes(text)) failures.push(`${file} is missing the current tool count "${text}"`);
    }
  }

  return failures;
}

function main() {
  const failures = [
    ...validateDocsLinks(),
    ...validatePackageReadmes(),
    ...validateWorkflowReadme(),
    ...validateToolCatalog(),
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
        // A reference example is one query row unless it shows the whole `{queries}` input.
        const schema = DIRECT_TOOL_DEFINITIONS.find(definition => definition.name === name).inputSchema;
        for (const example of examples) {
          const parsed = schema.safeParse('queries' in example ? example : { queries: [example] });
          if (!parsed.success) throw new Error(parsed.error.issues.map(issue => `${issue.path.join('.')}: ${issue.message}`).join('; '));
        }
      } catch (error) {
        failures.push(`docs/OCTOCODE_TOOLS.md ${name} example: ${error.message}`);
      }
    }
  }
  return failures;
}

main();
