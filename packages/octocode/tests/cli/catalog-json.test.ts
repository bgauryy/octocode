import { describe, expect, it, vi } from 'vitest';

import {
  getCompactToolSchemaShape,
  printMultipleToolSchemasJson,
  scopeCompactSchema,
} from '../../src/cli/tool-command/catalog-json.js';

function fieldForVariant(
  shape: ReturnType<typeof getCompactToolSchemaShape>,
  variantName: string,
  fieldName: string
) {
  return (
    shape.variants
      .find(variant => variant.name === variantName)
      ?.fields.find(field => field.name === fieldName) ??
    shape.fieldGroups
      .find(group => group.variants.includes(variantName))
      ?.fields.find(field => field.name === fieldName) ??
    shape.fields.find(field => field.name === fieldName)
  );
}

describe('compact tool schema branch fidelity', () => {
  it('renders a shared required field from the typed branch shape', () => {
    const pathField = {
      name: 'path',
      type: 'string',
      required: false,
    };
    const shape = scopeCompactSchema(
      [pathField],
      [
        {
          name: 'one',
          when: 'test',
          example: {},
          requires: ['path'],
          fields: ['path'],
        },
        {
          name: 'two',
          when: 'test',
          example: {},
          requires: ['path'],
          fields: ['path'],
        },
      ],
      { one: [pathField], two: [pathField] }
    );

    expect(shape.fields).toEqual([{ ...pathField, required: true }]);
  });

  it('keeps differing branch requirements scoped to their variants', () => {
    const pathField = {
      name: 'path',
      type: 'string',
      required: false,
    };
    const fileField = {
      name: 'file',
      type: 'string',
      required: false,
    };
    const shape = scopeCompactSchema(
      [pathField, fileField],
      [
        {
          name: 'match',
          when: 'test',
          example: {},
          requires: ['path'],
          fields: ['path'],
        },
        {
          name: 'topology:dependencies',
          when: 'test',
          example: {},
          requires: ['file'],
          fields: ['path', 'file'],
        },
        {
          name: 'topology:cycles',
          when: 'test',
          example: {},
          requires: ['path'],
          fields: ['path'],
        },
      ],
      {
        match: [pathField],
        'topology:dependencies': [pathField, fileField],
        'topology:cycles': [pathField],
      }
    );

    expect(fieldForVariant(shape, 'match', 'path')?.required).toBe(true);
    expect(
      fieldForVariant(shape, 'topology:dependencies', 'path')?.required
    ).toBe(false);
    expect(
      fieldForVariant(shape, 'topology:dependencies', 'file')?.required
    ).toBe(true);
    expect(fieldForVariant(shape, 'topology:cycles', 'path')?.required).toBe(
      true
    );
  });

  it('derives nested branch fields when typed variant metadata is absent', () => {
    const fields = [
      { name: 'content', type: 'object', required: false },
      { name: 'content.body', type: 'boolean', required: false },
      { name: 'path', type: 'string', required: false },
    ];
    const shape = scopeCompactSchema(
      fields,
      [
        {
          name: 'item',
          when: 'test',
          example: {},
          requires: [],
          fields: ['content'],
        },
        {
          name: 'file',
          when: 'test',
          example: {},
          requires: [],
          fields: ['path'],
        },
      ],
      {}
    );

    expect(fieldForVariant(shape, 'item', 'content.body')).toMatchObject({
      name: 'content.body',
    });
    expect(fieldForVariant(shape, 'file', 'path')).toMatchObject({
      name: 'path',
    });
  });

  it('keeps the real catalog renderer callable', () => {
    expect(
      getCompactToolSchemaShape('astSearch').variants.length
    ).toBeGreaterThan(0);
  });

  it('prints multiple compact schemas as one machine-readable payload', async () => {
    const output = vi.spyOn(console, 'log').mockImplementation(() => {});

    await expect(
      printMultipleToolSchemasJson(['ghSearch', 'localFetch'], {
        compact: true,
      })
    ).resolves.toBe(true);
    expect(JSON.parse(String(output.mock.calls[0]?.[0]))).toMatchObject({
      kind: 'octocode.toolSchemas.compact',
      schemas: [{ name: 'ghSearch' }, { name: 'localFetch' }],
    });

    output.mockRestore();
  });
});
