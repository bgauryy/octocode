/**
 * docs-catalog.ts — list/show skill reference docs for agents.
 *
 * Indexes skills/octocode-awareness/references/*.md (always present in the
 * skill bundle). Package docs/ are human/npm-only and are not required here.
 */

import { existsSync, readdirSync, readFileSync } from 'node:fs';
import { basename, dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

export interface DocCatalogEntry {
  name: string;
  title: string;
  description: string;
  kind: 'skill-ref';
  path: string;
}

export type DocCatalogSummary = Pick<DocCatalogEntry, 'name' | 'title'>;
export type DocCatalogListEntry = DocCatalogEntry | DocCatalogSummary;

export interface DocCatalogListResult<T extends DocCatalogListEntry = DocCatalogListEntry> {
  ok: true;
  count: number;
  docs: T[];
  root: string;
}

export interface ListSkillDocsOptions {
  cwd?: string;
  root?: string;
  lean?: boolean;
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

export function resolveSkillReferencesDir(here: string, cwd = process.cwd()): string {
  const invokedDir = process.argv[1] ? dirname(resolve(process.argv[1])) : here;
  const candidates = [
    process.env.OCTOCODE_SKILL_ROOT ? join(process.env.OCTOCODE_SKILL_ROOT, 'references') : null,
    join(invokedDir, 'skills', 'octocode-awareness', 'references'), // out/ CLI
    join(invokedDir, '..', 'references'), // standalone skill scripts/
    join(here, '..', 'references'), // standalone skill scripts/
    join(here, 'skills', 'octocode-awareness', 'references'), // out/index.js
    join(here, '..', 'skills', 'octocode-awareness', 'references'), // out/chunks or package src
    join(here, '..', '..', 'skills', 'octocode-awareness', 'references'),
    join(here, '..', '..', '..', 'skills', 'octocode-awareness', 'references'), // repo-root source
    join(cwd, 'skills', 'octocode-awareness', 'references'),
  ].filter((candidate): candidate is string => Boolean(candidate));
  return candidates.find((candidate) => existsSync(candidate)) ?? candidates[0]!;
}

function packageSkillReferencesDir(cwd = process.cwd()): string {
  return resolveSkillReferencesDir(dirname(fileURLToPath(import.meta.url)), cwd);
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

export function listSkillDocs(options: ListSkillDocsOptions & { lean: true }): DocCatalogListResult<DocCatalogSummary>;
export function listSkillDocs(options?: ListSkillDocsOptions & { lean?: false }): DocCatalogListResult<DocCatalogEntry>;
export function listSkillDocs(options: ListSkillDocsOptions): DocCatalogListResult;
export function listSkillDocs(options: ListSkillDocsOptions = {}): DocCatalogListResult {
  const root = options.root ?? packageSkillReferencesDir(options.cwd);
  const lean = Boolean(options.lean);
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
    docs: lean
      ? docs.map((doc) => ({ name: doc.name, title: doc.title }))
      : docs,
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
