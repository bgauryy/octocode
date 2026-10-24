// Where a competitor task runs: its corpus workspace, the normalization mode,
// corpus-path rewriting, and the provider rate-limit text both sides skip on.
import { REPOS_REL } from '../paths.mjs';

/** Normalized (default): corpus-root workspaces and short briefs; OCTOCODE_COMPETITOR_NORMALIZE=0 is the legacy run. */
export const NORMALIZED = process.env.OCTOCODE_COMPETITOR_NORMALIZE !== '0';
export const RATE_LIMIT = /rate.?limit|secondary rate|abuse detection|API rate limit exceeded|HTTP 429/i;

/**
 * The corpus repo a task runs in, relative to ROOT ('' = the octocode root):
 * an explicit `workspace`, else the single `{{T}}/<corpus>` its recipe names.
 */
export function taskWorkspace(task) {
  if (!NORMALIZED) return '';
  if (task.workspace != null) return task.workspace;
  const corpora = new Set([...JSON.stringify(task.octocode).matchAll(/\{\{T\}\}\/([\w.-]+)/g)].map(m => m[1]));
  return corpora.size === 1 ? `${REPOS_REL}/${[...corpora][0]}` : '';
}

const escapeRe = s => s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');

/** Rewrite corpus-prefixed paths to be relative to the task workspace. */
export function relativize(text, ws, prefixes) {
  if (!ws) return text;
  const corpus = ws.slice(REPOS_REL.length + 1);
  let out = text;
  for (const prefix of prefixes) {
    const head = escapeRe(`${prefix}/${corpus}`);
    out = out.replace(new RegExp(`${head}/`, 'g'), '').replace(new RegExp(`${head}(?![\\w./-])`, 'g'), '.');
  }
  return out;
}
