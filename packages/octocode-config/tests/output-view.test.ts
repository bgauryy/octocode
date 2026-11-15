import { expect, it } from 'vitest';
import { outputFields } from '../scripts/tool-contract/output-view.ts';

it('states the meaning of every row form that shares a field path', () => {
  const schema = {
    $defs: {
      Impact: { type: 'object', properties: { from: { type: 'string', description: 'impact: the node the change came from. A page states a shared one once.' } } },
      Walk: { type: 'object', properties: { from: { type: 'string', description: 'callers: the node the row was reached from. A page states a shared one once.' } } },
      Range: { type: 'object', description: 'Generic range.' },
    },
    type: 'object',
    properties: {
      results: { type: 'array', items: { type: 'object', properties: { data: { type: 'object', properties: {
        rows: { type: 'array', items: { anyOf: [{ $ref: '#/$defs/Impact' }, { $ref: '#/$defs/Walk' }] } },
        range: { description: 'This field\'s range.', anyOf: [{ $ref: '#/$defs/Range' }, { type: 'null' }] },
      } } } } },
    },
  };
  const fields = outputFields(schema, {});
  // Each row form's meaning, and the sentence they share once.
  expect(fields['rows[].from']).toBe(
    'impact: the node the change came from. A page states a shared one once. callers: the node the row was reached from.',
  );
  // A field's own description stands for the schema it refers to.
  expect(fields.range).toBe("This field's range.");
});
