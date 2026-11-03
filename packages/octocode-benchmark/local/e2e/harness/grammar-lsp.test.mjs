import assert from 'node:assert/strict';
import test from 'node:test';
import { GRAMMAR_FIXTURES } from './grammar-fixtures.mjs';
import { grammarAnchors, grammarProtocol } from './grammar-lsp.mjs';
import { captureMatchesFixture } from './grammar-cases.mjs';

test('capture oracle rejects wrong text and byte columns on the same Unicode line', () => {
  const fixture = GRAMMAR_FIXTURES.TypeScript;
  const { position } = grammarAnchors('TypeScript', fixture);
  const sourceLine = fixture.src.split('\n')[position.line];
  const capture = { text: fixture.argument, column: sourceLine.indexOf(fixture.argument) + 1 };
  assert.equal(captureMatchesFixture(fixture, sourceLine, capture), true);
  assert.equal(captureMatchesFixture(fixture, sourceLine, { ...capture, text: 'helper', column: sourceLine.indexOf('helper') + 1 }), false);
  assert.equal(captureMatchesFixture(fixture, sourceLine, { ...capture, column: capture.column + 2 }), false);
});

test('every grammar fixture defines an independent UTF-16 anchor and declaration', () => {
  for (const [language, fixture] of Object.entries(GRAMMAR_FIXTURES)) {
    const { symbol, position, target } = grammarAnchors(language, fixture);
    const lines = fixture.src.split('\n');
    assert.equal(lines[position.line].slice(position.character, position.character + symbol.length), symbol, language);
    assert.equal(lines[target.start.line].slice(target.start.character, target.end.character), symbol, language);
    assert.notDeepEqual(position, target.start, language);
    if (fixture.call) assert.ok(lines[position.line].slice(0, position.character).includes('😀'), language);
  }
});

test('grammar fixture rejects byte offsets and preserves definition/reference ranges', () => {
  const fixture = GRAMMAR_FIXTURES.TypeScript;
  const anchors = grammarAnchors('TypeScript', fixture);
  const uri = 'file:///workspace/main.ts';
  const messages = [], traced = [];
  const receive = grammarProtocol({ [uri]: structuredClone(anchors) }, value => messages.push(value), value => traced.push(value));
  const params = { textDocument: { uri }, position: anchors.position };
  receive({ id: 1, method: 'textDocument/definition', params });
  assert.deepEqual(messages[0].result, [{ uri, range: anchors.target }]);
  receive({ id: 2, method: 'textDocument/references', params: { ...params, context: { includeDeclaration: false } } });
  assert.equal(messages[1].result.length, anchors.locations.length - 1);
  assert.ok(messages[1].result.every(row => row.range.start.line !== anchors.target.start.line));
  receive({ id: 3, method: 'textDocument/definition', params: { ...params, position: { ...anchors.position, character: anchors.position.character + 2 } } });
  assert.equal(messages[2].error.code, -32602);
  assert.equal(traced.length, 3);
});
