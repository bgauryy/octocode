import { capabilityDefinitionRevision } from '../contracts/capability-sources.js';
import type { DiscoveredSkill } from './skill-discovery.js';

export interface SkillPageQuery { offset?: number; textOffset?: number; limit?: number; catalogRevision?: string }
export interface SkillPage {
  revision: string; total: number; skills: DiscoveredSkill[]; partial: boolean;
  fragment?: { row: number; field: 'description'; start: number; end: number; total: number };
  diagnostic?: { code: 'catalog-revision-changed' | 'invalid-cursor' | 'entry-limit'; message: string };
  next?: { tool: 'skill'; params: { queries: Array<SkillPageQuery & { reasoning: string; type: 'load'; action: 'list' }> } };
}

/** Bounded metadata pages retain exact descriptions and concrete grant identities. */
export function readSkillPage(skills: DiscoveredSkill[], query: SkillPageQuery = {}): SkillPage {
  const rows = [...skills].sort((a, b) => a.name.localeCompare(b.name));
  const revision = capabilityDefinitionRevision(rows);
  const next = (offset: number, textOffset = 0) => ({ tool: 'skill' as const, params: { queries: [{ reasoning: 'Continue the effective skill catalog', type: 'load' as const, action: 'list' as const, offset, textOffset, limit: query.limit ?? 50, catalogRevision: revision }] } });
  const page: SkillPage = { revision, total: rows.length, skills: [], partial: false };
  const offset = query.offset ?? 0;
  const textOffset = query.textOffset ?? 0;
  const limit = query.limit ?? 50;
  if (query.catalogRevision && query.catalogRevision !== revision) return { ...page, partial: true, diagnostic: { code: 'catalog-revision-changed', message: 'Skill catalog changed; restart the continuation.' }, next: next(0) };
  if (!Number.isInteger(offset) || offset < 0 || offset > rows.length || !Number.isInteger(textOffset) || textOffset < 0 || !Number.isInteger(limit) || limit < 1 || limit > 50) return { ...page, partial: true, diagnostic: { code: 'invalid-cursor', message: 'Invalid skill cursor; restart the continuation.' }, next: next(0) };
  let remaining = 8_000;
  for (let index = offset; index < rows.length && page.skills.length < limit; index++) {
    const row = rows[index]!;
    const description = row.description ?? '';
    const start = index === offset ? textOffset : 0;
    if (start > description.length) return { ...page, partial: true, diagnostic: { code: 'invalid-cursor', message: 'Description cursor is beyond this entry.' }, next: next(0) };
    const identityChars = JSON.stringify({ ...row, description: '' }).length;
    if (identityChars > 7_000) return { ...page, partial: true, diagnostic: { code: 'entry-limit', message: `Skill entry at row ${index} exceeds the identity size limit; shorten its metadata before restarting.` } };
    if (page.skills.length && remaining < identityChars + 1) return { ...page, partial: true, next: next(index) };
    remaining -= identityChars;
    const end = Math.min(description.length, start + Math.max(1, remaining));
    page.skills.push({ ...row, description: description.slice(start, end) });
    remaining -= end - start;
    if (start > 0 || end < description.length) page.fragment = { row: index, field: 'description', start, end, total: description.length };
    if (end < description.length) return { ...page, partial: true, next: next(index, end) };
    if ((remaining <= 0 || page.skills.length === limit) && index + 1 < rows.length) return { ...page, partial: true, next: next(index + 1) };
  }
  return page;
}
