// Invalid envelopes and batches containing no admissible row do no provider work.
export function inputBoundaries(seed, inputSchema) {
  const row = seed.queries[0];
  const batch = inputSchema.anyOf.find(branch => branch.properties?.queries).properties.queries;
  return [
    ['bareRow', row],
    ['nullEnvelope', null],
    ['scalarQueries', { queries: row }],
    ['emptyBatch', { queries: [] }],
    ['nullRow', { queries: [null] }],
    ['oversizedBatch', { queries: Array.from({ length: batch.maxItems + 1 }, () => row) }],
    ['unknownEnvelopeField', { ...seed, unexpectedField: true }],
    ['unknownRowField', { queries: [{ ...row, unexpectedField: true }] }],
    ['cursorWithQueries', { ...seed, cursor: 'foreign-cursor' }],
    ['cursorWithBudget', { cursor: 'foreign-cursor', responseChars: 100 }],
    ['zeroBudget', { ...seed, responseChars: 0 }],
    ['fractionalBudget', { ...seed, responseChars: 1.5 }],
  ];
}
