import { isDeepStrictEqual } from 'node:util';

const pageCall = result => result.json?.responsePagination?.next;
const isPageCall = (call, tool) => call?.kind === 'continue' && call.tool === tool
  && typeof call.input?.cursor === 'string' && Object.keys(call.input).length === 1;
const answers = responses => responses.flatMap(response => (response.json?.results ?? [])
  .flatMap(row => Object.entries(row.data.answers ?? {})));

function exactAnswers(actual, expected) {
  return actual.length === expected.size && new Set(actual.map(([key]) => key)).size === expected.size
    && actual.every(([key, answer]) => expected.has(key) && isDeepStrictEqual(answer, expected.get(key)));
}

/** Resume a page concurrently, then reach every child and replay its original handle. */
async function replayBranches(ctx, surface, fixture) {
  const { tool, input, expected } = fixture;
  const call = request => ctx.callOn(surface)(tool, request);
  const first = await call(input), parent = pageCall(first);
  if (!isPageCall(parent, tool)) return [['cursor.branch.issued', false, first.json]];

  const forks = await Promise.all([call(parent.input), call(parent.input)]);
  const children = forks.map(pageCall);
  const checks = [['cursor.branch.samePage', forks.every(result => !result.isError)
    && isDeepStrictEqual(forks[0].json?.results, forks[1].json?.results), forks.map(result => result.json)]];
  if (!children.every(child => isPageCall(child, tool))) {
    checks.push(['cursor.branch.children', false, forks.map(result => result.json)]);
    return checks;
  }

  const tails = await Promise.all(children.map(child => ctx.explore(tool, child.input, surface)));
  const complete = tails.every((tail, index) => tail.failures.length === 0
    && exactAnswers(answers([first, forks[index], ...tail.responses.map(response => response.result)]), expected));
  checks.push(['cursor.branch.complete', complete, tails.map(tail => ({ calls: tail.calls.length, failures: tail.failures }))]);

  const replay = await ctx.explore(tool, parent.input, surface);
  checks.push(['cursor.branch.originalReplay', replay.failures.length === 0
    && isDeepStrictEqual(replay.responses[0]?.result.json?.results, forks[0].json?.results)
    && exactAnswers(answers([first, ...replay.responses.map(response => response.result)]), expected),
  { calls: replay.calls.length, failures: replay.failures }]);
  return checks;
}

async function cursorReplay(ctx) {
  const questions = Object.fromEntries(Array.from({ length: 25 }, (_, index) => [
    `q${index}`, { type: 'noul', instructions: `Is question ${index} present?` },
  ]));
  const fixture = {
    tool: 'clasify',
    input: { queries: [{ operation: 'evaluate', content: 'Concurrent persistent cursor evidence', questions }], responseChars: 1 },
    expected: new Map(Object.keys(questions).map(key => [key, { type: 'noul', noul: 0.9 }])),
  };
  const checks = [];
  for (const surface of ctx.surfaces) {
    for (const [label, ok, detail] of await replayBranches(ctx, surface, fixture)) {
      checks.push([`${label}.${surface}`, ok, JSON.stringify(detail)]);
    }
  }
  return checks;
}

export const ADVANCED_CURSOR_REPLAY = { clasify: cursorReplay };
