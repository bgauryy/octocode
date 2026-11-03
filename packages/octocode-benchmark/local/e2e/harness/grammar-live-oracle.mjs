import path from 'node:path';

/** Compare source-derived positions and file identity, retaining duplicates as failures. */
export function semanticLocationEvidence(locations, ranges, file, base) {
  const observed = locations.map(row => [path.resolve(base, row.path), row.displayRange.startLine, row.displayRange.startCharacter])
    .toSorted((a, b) => a[0].localeCompare(b[0]) || a[1] - b[1] || a[2] - b[2]);
  const expected = ranges.map(row => [file, row.start.line + 1, row.start.character + 1])
    .toSorted((a, b) => a[1] - b[1] || a[2] - b[2]);
  return { ok: JSON.stringify(observed) === JSON.stringify(expected), expected, observed, locations };
}

export function selectedGrammarCases(capabilities, selected) {
  const cases = capabilities.flatMap(capability => capability.extensions.map(ext => ({ capability, ext })));
  for (const selection of selected) if (!cases.some(row => row.ext === selection || row.capability.language === selection)) {
    throw new Error(`Unknown grammar selection: ${selection}`);
  }
  return cases.filter(row => !selected.size || selected.has(row.ext) || selected.has(row.capability.language));
}
