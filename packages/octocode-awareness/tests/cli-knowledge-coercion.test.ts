import { expect, it } from 'vitest';
import { coerceFlag } from '../src/cli-adapter/cli-coercion.js';

it('supports nullable compare-and-set without changing ordinary string flags', () => {
  expect(coerceFlag('null', { anyOf: [{ type: 'string' }, { type: 'null' }] })).toBeNull();
  expect(coerceFlag('null', { type: 'string' })).toBe('null');
});

it('accepts JSON typed-anchor arrays and discriminated object items', () => {
  const schema = { type: 'array', items: { oneOf: [{ type: 'object' }, { type: 'object' }] } };
  expect(coerceFlag('[{"kind":"flow","value":"message.reply"}]', schema))
    .toEqual([{ kind: 'flow', value: 'message.reply' }]);
  expect(coerceFlag('{"kind":"flow","value":"message.reply"}', schema))
    .toEqual([{ kind: 'flow', value: 'message.reply' }]);
  expect(() => coerceFlag('[invalid', schema)).toThrow(/valid JSON/);
});

it('preserves repeated scalar arrays and coerces single scalar flags', () => {
  expect(coerceFlag(['a', 'b'], { type: 'array', items: { type: 'string' } })).toEqual(['a', 'b']);
  expect(coerceFlag(['result'], { type: 'string' })).toBe('result');
  expect(coerceFlag('2', { oneOf: [{ type: 'integer' }, { type: 'null' }] })).toBe(2);
});
