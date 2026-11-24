// `search ... --provider`: the top rows of a search, judged by the classification provider the user configured for
// Octocode's clasify tool (classification.api, classification.apiHost, classification.model in @octocodeai/config;
// no setting of its own), and reranked by fusing that judgment with the model-free ranking. Node only: the query
// engine stays pure and marks the request (stats.provider: 'requested'); the CLI, MCP and the viewer server call
// withProvider on the answer. Without a configured provider clasify refuses before sending anything, and the answer
// stays exactly the model-free one with a warning that says so. No embeddings: no Octocode setting names an
// embedding endpoint.
import { octocode } from './lib.ts';
import { absOf } from '../ids.ts';
import { RRF_K } from './search-fusion.ts';
import type { QueryResult, Row } from './query-api.ts';

/** How many of the best rows the provider judges: one provider request each. */
export const RERANK = 20;

const QUESTION = {
  type: 'score',
  instructions:
    'How directly this code is what the code search asks for: it does or defines the thing asked, not just mentions it',
  criteria: [
    'unrelated to the search',
    'related to the topic, but not what the search asks for',
    'does part of what the search asks for',
    'does or defines what the search asks for',
  ],
};

interface Judged {
  score: number;
  confidence?: number;
}

/** The shape of a clasify answer this module reads: per resource, the score question's answer, and the model. */
interface ClasifyAnswer {
  results?: {
    data?: {
      model?: string;
      resources?: Record<
        string,
        {
          answers?: { answers?: Judged };
          usage?: {
            calls?: number;
            inputTokens?: number;
            outputTokens?: number;
          };
        }
      >;
    };
    error?: string;
    errorCode?: string;
  }[];
  error?: string;
  errorCode?: string;
}

type Call = (args: string[]) => Promise<unknown>;

const warn = (
  r: QueryResult,
  warning: string,
  provider: unknown
): QueryResult => ({
  ...r,
  warnings: [
    ...(Array.isArray(r.warnings) ? (r.warnings as string[]) : []),
    warning,
  ],
  stats: { ...r.stats, provider },
});

// The localFetch query that shows a row's code: a symbol's declaration span, or its whole file.
function sourceOf(root: string, row: Row) {
  const path = absOf(root, row.loc?.path ?? row.id);
  const start = row.symbol ? row.loc?.line : null;
  const end = (row.loc as { endLine?: number } | null)?.endLine;
  return {
    tool: 'localFetch',
    query: start
      ? {
          path,
          ranges: [
            { startLine: start, endLine: Math.max(start, end ?? start) },
          ],
        }
      : { path },
  };
}

/**
 * A search answer reranked by the configured classification provider when it asked for one (`--provider`);
 * every other answer as it is. `call` runs the octocode CLI (lib.ts octocode).
 */
export async function withProvider(
  r: QueryResult,
  call: Call = args => octocode(args, { partial: true })
): Promise<QueryResult> {
  if (r.op !== 'search' || !r.ok || r.stats?.provider !== 'requested') return r;
  const top = r.rows.slice(0, RERANK);
  if (!top.length)
    return { ...r, stats: { ...r.stats, provider: 'nothing to judge' } };
  const root = r.scope.root ?? '';
  const keys = top.map((_, i) => `r${i + 1}`);
  // The search's words, without its op and flags (a filter's value stays: it says what is asked).
  const text = r.text
    .split(/\s+/)
    .slice(1)
    .filter(w => !w.startsWith('--'))
    .join(' ');
  const request = {
    queries: [
      {
        operation: 'evaluate',
        // details: the answer names the model that judged.
        view: 'details',
        mainGoal: `Code search: ${text}`.slice(0, 500),
        reasoning:
          'Rerank the best rows of a code search by how directly each is what it asks for.',
        resources: Object.fromEntries(
          top.map((row, i) => [keys[i], sourceOf(root, row)])
        ),
        questions: { answers: QUESTION },
      },
    ],
  };
  // A refusal comes back as an answer or, with a failing exit status, as the call's error carrying that answer.
  const refusal = (message: string, code: string | undefined) =>
    code === 'missingConfiguration'
      ? warn(
          r,
          `--provider: no classification provider is configured (${message}); nothing was sent, and these rows are the model-free ranking`,
          'not configured'
        )
      : warn(
          r,
          `--provider: the provider did not answer (${message}); these rows are the model-free ranking`,
          'failed'
        );
  let answer: ClasifyAnswer;
  try {
    answer = (await call([
      'clasify',
      '--json',
      JSON.stringify(request),
    ])) as ClasifyAnswer;
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    const printed = /\{.*\}/s.exec(message)?.[0];
    let parsed: ClasifyAnswer | null = null;
    try {
      parsed = printed ? (JSON.parse(printed) as ClasifyAnswer) : null;
    } catch {
      parsed = null;
    }
    return refusal(parsed?.error ?? message, parsed?.errorCode);
  }
  const first = answer.results?.[0];
  const refused = answer.error ?? first?.error;
  if (refused) return refusal(refused, answer.errorCode ?? first?.errorCode);
  const model = first?.data?.model ?? 'the configured provider';
  const resources = first?.data?.resources ?? {};
  const scores = keys.map(k => resources[k]?.answers?.answers?.score);
  const judged = scores.filter(s => typeof s === 'number').length;
  if (!judged)
    return warn(
      r,
      '--provider: the provider judged no row; these rows are the model-free ranking',
      'unjudged'
    );
  // Reciprocal rank fusion of the model-free order and the provider's (an unjudged row keeps its own place only);
  // a tie goes to the provider's higher score, then to the model-free order.
  const byProvider = top
    .map((_, i) => i)
    .filter(i => typeof scores[i] === 'number')
    .sort((a, b) => (scores[b] as number) - (scores[a] as number) || a - b);
  const fused = top.map((_, i) => {
    const p = byProvider.indexOf(i);
    return 1 / (RRF_K + i + 1) + (p < 0 ? 0 : 1 / (RRF_K + p + 1));
  });
  const order = top
    .map((_, i) => i)
    .sort(
      (a, b) =>
        fused[b] - fused[a] || (scores[b] ?? -1) - (scores[a] ?? -1) || a - b
    );
  const usage = Object.values(resources).reduce(
    (u, x) => ({
      calls: u.calls + (x.usage?.calls ?? 0),
      inputTokens: u.inputTokens + (x.usage?.inputTokens ?? 0),
      outputTokens: u.outputTokens + (x.usage?.outputTokens ?? 0),
    }),
    { calls: 0, inputTokens: 0, outputTokens: 0 }
  );
  let moved = 0;
  const reranked = order.map((i, k) => {
    const row = top[i];
    const score = scores[i];
    if (k !== i) moved++;
    return {
      ...row,
      rank: k + 1,
      ...(typeof score === 'number' && { provider: { model, score } }),
      ...(k !== i && {
        found: [...((row.found as string[] | undefined) ?? []), 'provider'],
        movedFrom: i + 1,
      }),
    };
  });
  const best = sourceOf(root, reranked[0]);
  return {
    ...r,
    next: {
      ...(r.next as object),
      read: { tool: 'localFetch', input: { queries: [best.query] } },
    },
    rows: [...reranked, ...r.rows.slice(top.length)],
    stats: { ...r.stats, provider: { model, judged, moved, ...usage } },
    summary: `${r.summary.replace(/; best .+? \((?:[a-z]+)\)/, `; best ${reranked[0].symbol ?? reranked[0].id}`)}; ${model} judged the top ${judged} and moved ${moved}`,
  };
}
