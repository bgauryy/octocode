// Naming-hypothesis routing eval for the semanticAssess -> clasify rename.
//
// Hypothesis: renaming the public bounded-judgment tool to `clasify` improves an
// agent's FIRST-tool selection (route to it exactly when a bounded Noul/Choice/
// Score judgment over unread or supplied evidence changes the next action, and
// NOT for exact lookup, deterministic checks, required proof, or free-form
// summarization) without inflating unnecessary provider-backed classification
// calls.
//
// `category` partitions the corpus:
//   positive     -> the first tool should be `clasify`
//   negative     -> the first tool should NOT be `clasify` (exact/proof/summary)
//   availability -> gated by the classification provider key
//
// `reference` is grader-only and must never enter a model prompt. For `clasify`
// the runtime accepts a bare SemanticQuery (resources[] x questions[]); the
// reference records the intended judgment type, not an exact-match query.
const positive = (id, prompt, judgment) => ({
  id,
  category: 'positive',
  prompt,
  expected: { tool: 'clasify', judgment },
  reference: { calls: [{ tool: 'clasify' }], answer: '' },
});
const negative = (id, prompt, tool, why) => ({
  id,
  category: 'negative',
  prompt,
  expected: { tool, notTool: 'clasify', why },
  reference: { calls: [{ tool }], answer: '' },
});

export const cases = [
  // --- positive: bounded judgment over UNREAD candidates changes the next read ---
  positive(
    'unread-candidate-noul',
    'A ghSearch returned five candidate files for "retry backoff". Without pulling their bodies into my context, judge which single file most likely implements exponential backoff so I read only that one.',
    'noul',
  ),
  positive(
    'unread-choice-scout',
    'I have three unread GitHub files for the auth-middleware question. Classify each as relevant, unrelated, or insufficient without returning their contents, so I skip the ones that will not help.',
    'choice',
  ),
  positive(
    'unread-score-rank',
    'Rate how strongly each of these four unread documentation pages matches "GraphQL cursor pagination" on a 0-4 scale before I decide which one to open.',
    'score',
  ),
  // --- positive: self-review over SUPPLIED work (context.value, triggers no read) ---
  positive(
    'supplied-draft-overclaim',
    'Here is my draft answer plus its rationale, cited evidence, and stated assumptions. Judge whether the draft overclaims relative to the supplied evidence before I present it.',
    'noul',
  ),

  // --- negative: exact/deterministic/proof/summary must NOT route to clasify ---
  negative(
    'exact-line-read',
    'Read lines 10 through 40 of /workspace/project/src/app.ts exactly.',
    'localFetch',
    'exact bounded read is deterministic, not a judgment',
  ),
  negative(
    'deterministic-membership',
    'Does /workspace/project/config.json contain a top-level "timeout" key? Read it and answer.',
    'localFetch',
    'exact key membership is a deterministic check, not a probabilistic judgment',
  ),
  negative(
    'required-proof-callers',
    'Prove that enqueue is called by dequeue by finding the real call site near /workspace/project/src/scheduler.ts.',
    'lspSearch',
    'a claim that needs proof must be grounded in references, not a routed judgment',
  ),
  negative(
    'free-form-summary',
    'Write a prose summary of this repository\'s overall architecture.',
    'none',
    'free-form summarization is outside a bounded typed judgment',
  ),

  // --- availability: gated by OCTOCODE_CLASSIFICATION_API ---
  {
    id: 'availability-key-present',
    category: 'availability',
    keyPresent: true,
    prompt:
      'With the classification provider configured: judge which of these three unread candidate files is the interceptor implementation without reading their bodies.',
    expected: { tool: 'clasify', judgment: 'choice' },
    reference: { calls: [{ tool: 'clasify' }], answer: '' },
  },
  {
    id: 'availability-key-absent',
    category: 'availability',
    keyPresent: false,
    disabledTools: ['clasify'],
    prompt:
      'No classification provider is configured. Decide which of these three unread candidate files is the interceptor implementation.',
    expected: { tool: 'none', notTool: 'clasify', why: 'clasify is unavailable without OCTOCODE_CLASSIFICATION_API; fall back to reading or lexical search' },
    reference: { calls: [], answer: 'clasify is unavailable without OCTOCODE_CLASSIFICATION_API; read the candidates or use localSearch instead.' },
  },
];

export const categories = ['positive', 'negative', 'availability'];
