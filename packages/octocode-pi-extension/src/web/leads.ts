import { OCTOCODE_TOOL_PREFIX } from '../shared/octocode-tools.js';

/** The Octocode research tools that can work on a saved page, when this session has them. */
type SavedPageTools = { clasify: boolean; localSearch: boolean; localFetch: boolean };

/** Which of the tools that read a saved page are active; `clasify` is listed only when its classification provider is set up. */
export function savedPageTools(active: readonly string[]): SavedPageTools {
  const has = (name: string) => active.includes(`${OCTOCODE_TOOL_PREFIX}${name}`);
  return { clasify: has('clasify'), localSearch: has('localSearch'), localFetch: has('localFetch') };
}

/**
 * How to reach the rest of a saved page, naming the tools this session has: `clasify` to screen unread text,
 * `localSearch` for literal text, `localFetch` for line ranges. Without any, the generic wording stands.
 */
export function savedPageLead(tools: SavedPageTools): string {
  const reads = [
    tools.clasify && `screen a bounded unread selection with ${OCTOCODE_TOOL_PREFIX}clasify (source {tool:"localFetch", query:{path:<this file>, ranges:<observed line ranges>}}, a noul question about one stated fact; typed answers only, read deciding evidence)`,
    tools.localSearch && `search literal text with ${OCTOCODE_TOOL_PREFIX}localSearch (headings: matchString:["^#{1,3} "], regexEngine:"rust")`,
    tools.localFetch && `read line ranges with ${OCTOCODE_TOOL_PREFIX}localFetch`,
  ].filter(Boolean);
  return reads.length > 0 ? reads.join('; ') : 'search it or read it by line range';
}
