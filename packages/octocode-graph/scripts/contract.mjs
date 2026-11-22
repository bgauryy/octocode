import { compile } from 'json-schema-to-typescript';
import { readFile } from 'node:fs/promises';

export async function graphDeclarations() {
  const contract = import.meta
    .resolve('@octocodeai/config/tool-types.schema.json');
  const { $defs } = JSON.parse(await readFile(new URL(contract), 'utf8'));
  const names = [
    'GraphBuildInput',
    'GraphBuildOutput',
    'GraphQueryInput',
    'GraphQueryOutput',
  ];
  const properties = Object.fromEntries(
    names.map(key => [key, { $ref: `#/$defs/${key}` }])
  );
  return compile(
    {
      type: 'object',
      $defs,
      properties,
      required: names,
      additionalProperties: false,
    },
    'GraphContract',
    {
      bannerComment:
        '// Generated from @octocodeai/config/tool-types.schema.json. The native engine validates the wire format.',
      format: false,
      ignoreMinAndMaxItems: true,
    }
  );
}
