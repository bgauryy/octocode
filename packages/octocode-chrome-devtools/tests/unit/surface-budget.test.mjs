import test from 'node:test';
import assert from 'node:assert/strict';
import { measure } from '../../tools/measure-surface.mjs';

// Invariants of the schema token cost clients load on connect (see docs/mcp-cli.md, Token budget).
const [slim, research, full] = await Promise.all(
  ['slim', 'research', 'full'].map(measure)
);
const size = (surface, name) =>
  surface.rows.find(row => row.name === name).schemaChars;

test('presets cost less the smaller they are', () => {
  assert.ok(slim.chars < String(research.chars));
  assert.ok(research.chars < full.chars);
});

test('the default surface stays at most 62% of the full surface (it holds cdp and protocol)', () => {
  assert.ok(research.chars <= full.chars * 0.62);
});

test('the slim surface stays at most 52% of the default surface', () => {
  assert.ok(slim.chars <= research.chars * 0.52);
});

test('flow does not embed a copy of the plan schema', () => {
  assert.ok(size(research, 'flow') < size(research, 'run') * 0.25);
});

test('the default surface stays under a documented ceiling', () => {
  // Measured 2026-11: about 32.8k chars (8.2k tokens at chars/4) for the default surface (cdp and protocol included); ceiling is that value + 10%.
  const measuredDefaultChars = 32800;
  assert.ok(
    research.chars <= measuredDefaultChars * 1.1,
    String(research.chars)
  );
});
