import Ajv2020 from 'ajv/dist/2020.js';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import {
  CONFIG_FIELDS,
  ENV_TOKEN_VARS,
  PROTECTED_KEY_NAMES,
} from '../src/config/contract.generated.js';

const contractPath = fileURLToPath(
  new URL('../config-contract.json', import.meta.url)
);
const schemaPath = fileURLToPath(
  new URL('../config-contract.schema.json', import.meta.url)
);

function loadJson(path: string): unknown {
  return JSON.parse(readFileSync(path, 'utf8')) as unknown;
}

describe('generated config contract', () => {
  it('validates the authoritative declaration with the shared meta-schema', () => {
    const ajv = new Ajv2020({ allErrors: true, strict: false });
    const schema = loadJson(schemaPath) as Record<string, unknown>;
    expect(ajv.validate(schema, loadJson(contractPath))).toBe(true);
  });

  it('generates one runtime field specification for every declared field', () => {
    const contract = loadJson(contractPath) as {
      sections: Record<string, { fields: Record<string, unknown> }>;
    };
    const declaredPaths = Object.entries(contract.sections).flatMap(
      ([section, definition]) =>
        Object.keys(definition.fields).map(field =>
          section.length === 0 ? field : `${section}.${field}`
        )
    );
    expect(CONFIG_FIELDS.map(field => field.path)).toEqual(declaredPaths);
  });

  it('derives token priority and dotenv protection from one environment policy', () => {
    expect(ENV_TOKEN_VARS).toEqual(['GH_TOKEN', 'GITHUB_TOKEN']);
    expect(
      ENV_TOKEN_VARS.every(name => !new Set<string>(PROTECTED_KEY_NAMES).has(name))
    ).toBe(true);
  });
});
