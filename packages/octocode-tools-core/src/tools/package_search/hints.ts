import type { HintContext, ToolHintGenerators } from '../../types/metadata.js';

function buildVariations(name: string): string[] {
  const v: string[] = [];
  if (name.includes('-')) {
    v.push(name.replace(/-/g, '_'));
    v.push(name.replace(/-/g, ''));
  }
  if (name.includes('_')) v.push(name.replace(/_/g, '-'));
  if (name.startsWith('@')) {
    const u = name.split('/').pop();
    if (u) v.push(u);
  }
  if (!name.endsWith('js')) v.push(name + 'js');
  return [...new Set(v)].filter(s => s !== name).slice(0, 3);
}

export const hints: ToolHintGenerators = {
  empty: (ctx: HintContext = {}) => {
    const c = ctx as Record<string, unknown>;
    const name = typeof c.name === 'string' ? c.name : undefined;
    if (!name) return [];
    const variations = buildVariations(name);
    return [
      'Check spelling and remove any version suffix.',
      'Not on npm? Use `ghSearchRepos` instead.',
      ...(variations.length ? [`Try: ${variations.join(', ')}`] : []),
    ];
  },

  error: (ctx: HintContext = {}) => {
    if (ctx.isRateLimited) {
      return [
        `npm registry rate limited.${ctx.retryAfter ? ` Retry after ${ctx.retryAfter}s.` : ' Wait before retrying.'}`,
      ];
    }
    if (ctx.originalError) {
      return ['npm registry unreachable — use `ghSearchRepos` instead.'];
    }
    return [];
  },
};
