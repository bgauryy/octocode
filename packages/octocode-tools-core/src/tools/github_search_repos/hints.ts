import type { HintContext, ToolHintGenerators } from '../../types/metadata.js';

export const hints: ToolHintGenerators = {
  empty: (ctx: HintContext = {}) => {
    const c = ctx as Record<string, unknown>;
    const query = typeof c.query === 'string' ? c.query : undefined;
    const keywords = Array.isArray(c.keywords) ? c.keywords : undefined;
    const language = typeof c.language === 'string' ? c.language : undefined;
    const owner = typeof c.owner === 'string' ? c.owner : undefined;
    const topic = typeof c.topic === 'string' ? c.topic : undefined;
    const hasFilters = language || owner || topic;
    const searchTerm =
      query ??
      (keywords && keywords.length > 0 ? String(keywords[0]) : undefined);

    if (!searchTerm && !hasFilters) return [];

    const out: string[] = [
      hasFilters
        ? 'Remove owner/language/topic first, then retry fewer keywords.'
        : 'Try fewer/simpler keywords, or use match:["name"] for exact-name lookup.',
    ];

    if (
      searchTerm &&
      /^@[\w-]+\/[\w.-]+$|^[a-z][\w]*[-.][\w.-]+$/.test(searchTerm)
    ) {
      out.push(
        `"${searchTerm}" looks like a package — use \`npmSearch\` instead.`
      );
    }

    return out;
  },

  error: (ctx: HintContext = {}) => {
    if (ctx.isRateLimited) {
      return [
        `GitHub API rate limited.${ctx.retryAfter ? ` Retry after ${ctx.retryAfter}s.` : ' Wait before retrying.'}`,
      ];
    }
    if (ctx.status === 401) return ['GITHUB_TOKEN is missing or expired.'];
    if (ctx.status === 403) return ['Token lacks `public_repo` scope.'];
    return [];
  },
};
