export interface MemoryEvaluationCaseV1 {
  expectedIds: string[];
  returnedIds: string[];
  staleIds?: string[];
  falseRecallWeight?: number;
}

export interface MemoryEvaluationResultV1 {
  version: 1;
  precision: number;
  recall: number;
  staleRecallRate: number;
  falseRecallCost: number;
}

export const MEMORY_RECALL_MODES_V1 = ['lexical', 'semantic', 'hybrid'] as const;
export type MemoryRecallModeV1 = typeof MEMORY_RECALL_MODES_V1[number];

export interface MemoryEvaluationQueryV1 {
  caseId: string;
  query: string;
  mode: MemoryRecallModeV1;
  expectedSourceDigests: string[];
  staleSourceDigests?: string[];
  forbiddenSourceDigests?: string[];
  scope?: 'project' | 'artifact';
  falseRecallWeight?: number;
  risk?: 'ordinary' | 'stale' | 'cross-scope' | 'secret';
}

export interface MemoryEvaluationCorpusV1 {
  version: 1;
  corpusId: string;
  cases: MemoryEvaluationQueryV1[];
}

export interface MemoryEvaluationCaseResultV1 extends MemoryEvaluationQueryV1 {
  returnedSourceDigests: string[];
  precision: number;
  recall: number;
  staleRecallRate: number;
  falseRecallCost: number;
}

export interface MemoryEvaluationReportV1 {
  version: 1;
  corpusId: string;
  aggregate: MemoryEvaluationResultV1;
  byMode: Record<MemoryRecallModeV1, MemoryEvaluationResultV1>;
  staleRecallCost: number;
  crossScopeRecallCost: number;
  secretRecallCost: number;
  cases: MemoryEvaluationCaseResultV1[];
}

/**
 * Maintained smoke corpus for the production evaluator. The digests are fixture
 * identities, not real repository content; callers can provide a domain corpus
 * through the same API/CLI contract.
 */
export const MEMORY_EVALUATION_CORPUS_V1: MemoryEvaluationCorpusV1 = {
  version: 1,
  corpusId: 'octocode-memory-hardening-v1',
  cases: [
    { caseId: 'lexical-migration', query: 'sqlite migration transaction', mode: 'lexical', expectedSourceDigests: ['eval:fresh:migration'] },
    { caseId: 'semantic-authorization', query: 'single use permission race', mode: 'semantic', expectedSourceDigests: ['eval:fresh:authorization'] },
    { caseId: 'hybrid-recovery', query: 'resume after compact', mode: 'hybrid', expectedSourceDigests: ['eval:fresh:recovery'] },
    { caseId: 'stale-command', query: 'release command', mode: 'hybrid', expectedSourceDigests: ['eval:fresh:release'], staleSourceDigests: ['eval:stale:release'], risk: 'stale', falseRecallWeight: 3 },
    { caseId: 'artifact-scope', query: 'artifact decision', mode: 'lexical', scope: 'artifact', expectedSourceDigests: ['eval:artifact:decision'], forbiddenSourceDigests: ['eval:project:decision'], risk: 'cross-scope', falseRecallWeight: 4 },
    { caseId: 'secret-exclusion', query: 'credential token', mode: 'hybrid', expectedSourceDigests: [], forbiddenSourceDigests: ['eval:secret:credential'], risk: 'secret', falseRecallWeight: 10 },
  ],
};

export function containsSecretLikeText(text: string): boolean {
  return /-----BEGIN [A-Z ]*PRIVATE KEY-----|\b(?:api[_-]?key|access[_-]?token|client[_-]?secret|password)\s*[:=]\s*[^\s]{6,}|\b(?:sk|ghp|github_pat)_[A-Za-z0-9_-]{16,}\b/i.test(text);
}

export function evaluateMemoryRecall(cases: MemoryEvaluationCaseV1[]): MemoryEvaluationResultV1 {
  let relevantReturned = 0;
  let returned = 0;
  let expected = 0;
  let stale = 0;
  let falseRecallCost = 0;
  for (const item of cases) {
    const expectedSet = new Set(item.expectedIds);
    const staleSet = new Set(item.staleIds ?? []);
    expected += expectedSet.size;
    returned += item.returnedIds.length;
    for (const id of item.returnedIds) {
      if (expectedSet.has(id)) relevantReturned++;
      else falseRecallCost += item.falseRecallWeight ?? 1;
      if (staleSet.has(id)) stale++;
    }
  }
  return {
    version: 1,
    precision: returned === 0 ? 1 : relevantReturned / returned,
    recall: expected === 0 ? 1 : relevantReturned / expected,
    staleRecallRate: returned === 0 ? 0 : stale / returned,
    falseRecallCost,
  };
}

export function runMemoryEvaluationCorpus(
  corpus: MemoryEvaluationCorpusV1,
  recall: (item: MemoryEvaluationQueryV1) => Array<{ sourceDigest: string }>,
): MemoryEvaluationReportV1 {
  if (corpus.version !== 1 || !corpus.corpusId?.trim() || !Array.isArray(corpus.cases)) throw new Error('memory evaluation corpus must be version 1 with a corpusId and cases');
  const observed = corpus.cases.map((item): MemoryEvaluationCaseResultV1 => {
    if (!item.caseId.trim() || !item.query.trim() || !MEMORY_RECALL_MODES_V1.includes(item.mode)) throw new Error('memory evaluation case is invalid');
    const returnedSourceDigests = recall(item).map((memory) => memory.sourceDigest);
    const metrics = evaluateMemoryRecall([{
      expectedIds: item.expectedSourceDigests,
      returnedIds: returnedSourceDigests,
      staleIds: item.staleSourceDigests,
      falseRecallWeight: item.falseRecallWeight,
    }]);
    const { version: _version, ...scores } = metrics;
    return { ...item, returnedSourceDigests, ...scores };
  });
  const metricCases = observed.map((item) => ({
    expectedIds: item.expectedSourceDigests,
    returnedIds: item.returnedSourceDigests,
    staleIds: item.staleSourceDigests,
    falseRecallWeight: item.falseRecallWeight,
  }));
  const empty = (): MemoryEvaluationResultV1 => ({ version: 1, precision: 1, recall: 1, staleRecallRate: 0, falseRecallCost: 0 });
  const byMode = Object.fromEntries(MEMORY_RECALL_MODES_V1.map((mode) => {
    const selected = observed.filter((item) => item.mode === mode).map((item) => ({ expectedIds: item.expectedSourceDigests, returnedIds: item.returnedSourceDigests, staleIds: item.staleSourceDigests, falseRecallWeight: item.falseRecallWeight }));
    return [mode, selected.length ? evaluateMemoryRecall(selected) : empty()];
  })) as Record<MemoryRecallModeV1, MemoryEvaluationResultV1>;
  const riskCost = (risk: MemoryEvaluationQueryV1['risk']): number => observed
    .filter((item) => item.risk === risk)
    .reduce((sum, item) => {
      const forbidden = new Set(item.forbiddenSourceDigests ?? []);
      return sum + item.returnedSourceDigests.filter((id) => forbidden.has(id)).length * (item.falseRecallWeight ?? 1);
    }, 0);
  return {
    version: 1,
    corpusId: corpus.corpusId,
    aggregate: evaluateMemoryRecall(metricCases),
    byMode,
    staleRecallCost: observed.filter((item) => item.risk === 'stale').reduce((sum, item) => {
      const stale = new Set(item.staleSourceDigests ?? []);
      return sum + item.returnedSourceDigests.filter((id) => stale.has(id)).length * (item.falseRecallWeight ?? 1);
    }, 0),
    crossScopeRecallCost: riskCost('cross-scope'),
    secretRecallCost: riskCost('secret'),
    cases: observed,
  };
}
