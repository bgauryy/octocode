import { isAbsolute, relative, resolve, sep } from 'node:path';
import { z } from 'zod';

export function createAnchorSchema(pathLimit = 1024) {
  const value = z.string().trim().min(1).regex(/^[^\p{Cc}]+$/u, 'anchor cannot contain control characters');
  return z.union([
    z.object({ kind: z.enum(['file', 'directory']), value: value.max(pathLimit) }).strict(),
    z.object({ kind: z.enum(['symbol', 'flow', 'failure', 'task', 'decision']), value: value.max(400) }).strict(),
  ]);
}
export const anchorSchema = createAnchorSchema();
export type KnowledgeAnchor = z.infer<typeof anchorSchema>;

/** Logical anchors retain explicit identity; only filesystem anchors resolve paths. */
export function normalizeAnchors(workspace: string, anchors: readonly KnowledgeAnchor[]): KnowledgeAnchor[] {
  const root = resolve(workspace);
  const normalized = anchors.map(input => {
    const anchor = anchorSchema.parse(input);
    if (anchor.kind !== 'file' && anchor.kind !== 'directory') return anchor;
    if (anchor.value.includes('\\')) throw new Error('filesystem anchor contains an invalid path character');
    const local = relative(root, resolve(root, anchor.value));
    if (local === '..' || local.startsWith(`..${sep}`) || isAbsolute(local) || (!local && anchor.kind === 'file')) {
      throw new Error('anchor must stay within the opened workspace');
    }
    return { kind: anchor.kind, value: local.split(sep).join('/') || '.' };
  });
  return [...new Map(normalized.map(anchor => [`${anchor.kind}:${anchor.value}`, anchor])).values()]
    .sort((a, b) => `${a.kind}:${a.value}`.localeCompare(`${b.kind}:${b.value}`));
}

export function knowledgeAnchorReference(anchor: KnowledgeAnchor): string {
  return `knowledge-anchor:${anchor.kind}:${anchor.value}`;
}

/** Ancestor reference candidates are exact, indexed lookups; src/a never matches src/ab. */
export function matchingAnchorReferences(anchor: KnowledgeAnchor): string[] {
  const refs = [knowledgeAnchorReference(anchor)];
  if (anchor.kind !== 'file' && anchor.kind !== 'directory') return refs;
  const parts = anchor.value === '.' ? [] : anchor.value.split('/');
  if (anchor.kind === 'file') parts.pop();
  while (parts.length) {
    refs.push(knowledgeAnchorReference({ kind: 'directory', value: parts.join('/') }));
    parts.pop();
  }
  refs.push(knowledgeAnchorReference({ kind: 'directory', value: '.' }));
  return [...new Set(refs)];
}
