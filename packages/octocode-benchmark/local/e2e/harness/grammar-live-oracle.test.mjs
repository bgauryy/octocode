import assert from 'node:assert/strict';
import test from 'node:test';
import { semanticLocationEvidence, selectedGrammarCases } from './grammar-live-oracle.mjs';

const expected = [{ start: { line: 2, character: 4 } }, { start: { line: 5, character: 16 } }];
const locations = [{ path: 'build.sbt', displayRange: { startLine: 3, startCharacter: 5 } },
  { path: 'build.sbt', displayRange: { startLine: 6, startCharacter: 17 } }];
const evidence = rows => semanticLocationEvidence(rows, expected, '/fixture/build.sbt', '/fixture');
test('semantic oracle accepts reordered evidence and resolves relative file identity', () => {
  assert.equal(evidence(locations.toReversed()).ok, true);
});
test('semantic oracle rejects duplicate, missing, wrong-file and wrong Unicode-column evidence', () => {
  assert.equal(evidence([locations[0], locations[0]]).ok, false);
  assert.equal(evidence(locations.slice(1)).ok, false);
  assert.equal(evidence(locations.map(row => ({ ...row, path: 'other.sbt' }))).ok, false);
  assert.equal(evidence([locations[0], { ...locations[1], displayRange: { startLine: 6, startCharacter: 16 } }]).ok, false);
});
test('selection retains every owned extension and rejects empty accidental selections', () => {
  const capabilities = [{ language: 'Scala', extensions: ['scala', 'sbt', 'sc'] }, { language: 'Java', extensions: ['java'] }];
  assert.equal(selectedGrammarCases(capabilities, new Set()).length, 4);
  assert.deepEqual(selectedGrammarCases(capabilities, new Set(['Scala'])).map(row => row.ext), ['scala', 'sbt', 'sc']);
  assert.deepEqual(selectedGrammarCases(capabilities, new Set(['sc'])).map(row => row.ext), ['sc']);
  assert.throws(() => selectedGrammarCases(capabilities, new Set(['scalaa'])), /Unknown grammar selection/);
});
