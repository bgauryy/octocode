// The hermetic loopback scorer for selection-execution.acceptance.mjs: scores
// the selected marker direct, the unknown marker ambiguous, anything else
// excluded, and refuses every request while `failure` is set. Selection
// policy v2 (core clasifyTemplates selectionPolicy: threshold 0.5; levels
// 0 excluded, 1 related, 2 selected) decides a level only above the
// threshold, so the ambiguous answer keeps every level at or below it; a
// confident level 1 would be a decided "related" near-miss, not unknown.
import assert from 'node:assert/strict';
import http from 'node:http';

/** Starts the scorer on 127.0.0.1; `requests` records every scoring body. */
export async function startSelectionScorer(markers) {
  const scorer = { failure: false, requests: [] };
  const provider = http.createServer(async (request, response) => {
    const chunks = [];
    for await (const chunk of request) chunks.push(chunk);
    const body = JSON.parse(Buffer.concat(chunks).toString());
    scorer.requests.push(body);
    if (scorer.failure) {
      response.writeHead(401, { 'content-type': 'application/json' });
      response.end(JSON.stringify({ error: 'synthetic refusal' }));
      return;
    }
    const state = JSON.stringify(body.state);
    const direct = state.includes(markers.selected);
    const unknown = state.includes(markers.unknown);
    const answers = Object.fromEntries(
      Object.entries(body.questions).map(([id, question]) => {
        assert.equal(question.type, 'score');
        assert.equal(question.criteria.length, 3);
        const probabilities = direct
          ? { 0: 0, 1: 0, 2: 1 }
          : unknown
            ? { 0: 0.4, 1: 0.4, 2: 0.2 }
            : { 0: 1, 1: 0, 2: 0 };
        const score = Object.entries(probabilities).reduce(
          (sum, [level, probability]) => sum + Number(level) * probability,
          0
        );
        return [
          id,
          {
            type: 'score',
            score,
            confidence: 1,
            probabilities,
            legend: Object.fromEntries(
              question.criteria.map((level, index) => [String(index), level])
            ),
          },
        ];
      })
    );
    response.writeHead(200, { 'content-type': 'application/json' });
    response.end(
      JSON.stringify({
        model: body.model,
        answers,
        usage: { input_tokens: 10, output_tokens: 2 },
      })
    );
  });
  await new Promise((resolve, reject) => {
    provider.once('error', reject);
    provider.listen(0, '127.0.0.1', () => {
      provider.off('error', reject);
      resolve();
    });
  });
  return Object.assign(scorer, {
    port: provider.address().port,
    close: () => new Promise(resolve => provider.close(resolve)),
  });
}
