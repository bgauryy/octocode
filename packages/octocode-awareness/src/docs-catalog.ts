/**
 * docs-catalog.ts — list/show skill reference docs for agents.
 *
 * Indexes skills/octocode-awareness/references/*.md (always present in the
 * skill bundle). Package docs/ are human/npm-only and are not required here.
 */

import { existsSync, readdirSync, readFileSync } from 'node:fs';
import { basename, dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

export interface DocCatalogEntry {
  name: string;
  title: string;
  description: string;
  kind: 'skill-ref';
  path: string;
}

export interface DocCatalogListResult {
  ok: true;
  count: number;
  docs: Array<Omit<DocCatalogEntry, 'path'> & { path: string }>;
  root: string;
}

export interface DocCatalogShowResult {
  ok: true;
  name: string;
  title: string;
  description: string;
  kind: 'skill-ref';
  path: string;
  content: string;
}

function packageSkillReferencesDir(cwd = process.cwd()): string {
  const here = dirname(fileURLToPath(import.meta.url));
  const candidates = [
    join(here, '..', 'skills', 'octocode-awareness', 'references'), // dist/skills
    join(here, '..', '..', 'skills', 'octocode-awareness', 'references'), // package root
    join(cwd, 'packages', 'octocode-awareness', 'skills', 'octocode-awareness', 'references'),
    join(cwd, 'skills', 'octocode-awareness', 'references'),
  ];
  return candidates.find((candidate) => existsSync(candidate)) ?? candidates[0]!;
}

function firstParagraph(lines: string[], start = 0): string {
  const chunks: string[] = [];
  let i = start;
  while (i < lines.length && !lines[i]!.trim()) i++;
  while (i < lines.length) {
    const line = lines[i]!.trim();
    if (!line) break;
    if (line.startsWith('#') || line.startsWith('```') || line.startsWith('|') || line.startsWith('- ')) break;
    chunks.push(line);
    i++;
  }
  return chunks.join(' ').replace(/\s+/g, ' ').trim();
}

function parseDocFile(filePath: string): DocCatalogEntry {
  const name = basename(filePath, '.md');
  const raw = readFileSync(filePath, 'utf8');
  const lines = raw.split(/\r?\n/);
  let title = name;
  let bodyStart = 0;
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i]!.trim();
    if (line.startsWith('# ')) {
      title = line.slice(2).trim();
      bodyStart = i + 1;
      break;
    }
  }
  let description = firstParagraph(lines, bodyStart);
  if (!description) {
    const whenLine = lines.find((line) => /^(use|read|load)\s+this\b/i.test(line.trim()));
    description = whenLine?.trim() ?? `${title} skill reference.`;
  }
  if (description.length > 180) description = `${description.slice(0, 177).trimEnd()}...`;
  return {
    name,
    title,
    description,
    kind: 'skill-ref',
    path: filePath,
  };
}

export function listSkillDocs(options: { cwd?: string; root?: string } = {}): DocCatalogListResult {
  const root = options.root ?? packageSkillReferencesDir(options.cwd);
  if (!existsSync(root)) {
    return { ok: true, count: 0, docs: [], root };
  }
  const docs = readdirSync(root)
    .filter((name) => name.endsWith('.md'))
    .sort((a, b) => a.localeCompare(b))
    .map((name) => parseDocFile(join(root, name)));
  return {
    ok: true,
    count: docs.length,
    docs: docs.map((doc) => ({
      name: doc.name,
      title: doc.title,
      description: doc.description,
      kind: doc.kind,
      path: doc.path,
    })),
    root,
  };
}

export function showSkillDoc(
  nameOrPath: string,
  options: { cwd?: string; root?: string } = {},
): DocCatalogShowResult | { ok: false; error: string; suggestions: string[] } {
  const list = listSkillDocs(options);
  const needle = nameOrPath.replace(/\.md$/i, '').trim().toLowerCase();
  const match = list.docs.find((doc) => doc.name.toLowerCase() === needle)
    ?? list.docs.find((doc) => doc.title.toLowerCase() === needle);
  if (!match) {
    const suggestions = list.docs
      .filter((doc) => doc.name.includes(needle) || doc.title.toLowerCase().includes(needle))
      .map((doc) => doc.name)
      .slice(0, 5);
    return {
      ok: false,
      error: `unknown doc "${nameOrPath}". Run docs list --compact.`,
      suggestions: suggestions.length > 0 ? suggestions : list.docs.slice(0, 5).map((doc) => doc.name),
    };
  }
  const content = readFileSync(match.path, 'utf8');
  return {
    ok: true,
    name: match.name,
    title: match.title,
    description: match.description,
    kind: match.kind,
    path: match.path,
    content,
  };
}
