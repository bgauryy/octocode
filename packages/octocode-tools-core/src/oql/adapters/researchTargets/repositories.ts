/**
 * `target:"repositories"` and `target:"packages"` adapters: thin wrappers
 * over ghSearchRepos / npmSearch that forward OQL paging and surface an
 * over-constraint hint when a multi-term repo search comes back empty.
 */
import { runDirect } from '../runner.js';
import { diagnostic } from '../../diagnostics.js';
import { toGithubRepositoryLanguage } from '../../transformers/language.js';
import type { AdapterResult } from '../local.js';
import { finishRecords } from './pagination.js';
import { splitRepo, withOqlPaging } from './shared.js';
import type { OqlQuery } from '../../types.js';

function firstScopeLanguage(query: OqlQuery): string | undefined {
  const lang = query.scope?.language;
  if (!lang) return undefined;
  return Array.isArray(lang) ? lang[0] : lang;
}

export async function executeRepositories(
  query: OqlQuery
): Promise<AdapterResult> {
  const { owner } = splitRepo(query.from);
  const forwarded = withOqlPaging(query, 'limit');
  const rawLanguage =
    typeof forwarded.language === 'string'
      ? forwarded.language
      : firstScopeLanguage(query);
  const language = toGithubRepositoryLanguage(rawLanguage);
  if (language) forwarded.language = language;
  const result = await runDirect('ghSearchRepos', {
    ...(owner ? { owner } : {}),
    ...forwarded,
  });
  const finished = finishRecords(
    result,
    'repository',
    'ghSearchRepos',
    query.from ?? { kind: 'github' }
  );
  // GitHub repo search ANDs every term across name/description/readme, so a
  // multi-term zero is usually over-constraint, not absence — say so instead
  // of letting "0 results, proof" read as a settled answer.
  if (finished.results.length === 0 && multiTermRepoQuery(forwarded)) {
    finished.diagnostics.push(
      diagnostic(
        'zeroMatches',
        'GitHub repository search requires EVERY term to match (AND semantics). Zero results for a multi-term query usually means over-constraint, not absence.',
        {
          backend: 'ghSearchRepos',
          severity: 'info',
          blocksAnswer: false,
          repair: {
            message:
              'Retry with the single most distinctive term (e.g. the project name), or move concepts to topic:"..." filters.',
          },
        }
      )
    );
  }
  return finished;
}

function multiTermRepoQuery(forwarded: Record<string, unknown>): boolean {
  // Shorthand lowers the positional text to `keywords` (term-split); raw
  // callers may pass `keywordsToSearch`. Either way, >1 term (or one term
  // containing spaces) means provider-AND over-constraint is in play.
  const terms = forwarded.keywords ?? forwarded.keywordsToSearch;
  if (Array.isArray(terms)) {
    return (
      terms.length > 1 ||
      (terms.length === 1 &&
        typeof terms[0] === 'string' &&
        terms[0].trim().includes(' '))
    );
  }
  return typeof terms === 'string' && terms.trim().includes(' ');
}

export async function executePackages(query: OqlQuery): Promise<AdapterResult> {
  const result = await runDirect('npmSearch', { ...withOqlPaging(query) });
  return finishRecords(
    result,
    'package',
    'npmSearch',
    query.from ?? { kind: 'npm' }
  );
}
