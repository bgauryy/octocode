import Ajv2020 from 'ajv/dist/2020.js';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import {
  CONFIG_FIELDS,
  ENV_TOKEN_VARS,
  PROTECTED_KEY_NAMES,
} from '../src/config/contract.generated.js';
import { resolveConfigFields } from '../src/config/resolverSections.js';
import type { OctocodeConfig } from '../src/config/types.js';

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
    expect(ENV_TOKEN_VARS).toEqual([
      'OCTOCODE_TOKEN',
      'GH_TOKEN',
      'GITHUB_TOKEN',
      'GITHUB_PERSONAL_ACCESS_TOKEN',
    ]);
    expect(
      ENV_TOKEN_VARS.every(name => PROTECTED_KEY_NAMES.some(key => key === name))
    ).toBe(true);
  });

  it('interprets malformed values through generic contract rules, never field branches', () => {
    const malformed = {
      version: 'future',
      github: { apiUrl: 123, graphqlEnabled: 'yes' },
      local: {
        allowedPaths: [42],
        workspaceRoot: 'relative/path',
      },
      tools: { enabled: 'ghSearch' },
      network: { timeout: Number.NaN, maxRetries: 'many' },
      lsp: { configPath: 10 },
      output: { format: 1 },
    } as unknown as OctocodeConfig;

    const resolved = resolveConfigFields(malformed, {
      GITHUB_API_URL: 'not a URL',
      ENABLE_LOCAL: 'sometimes',
      ALLOWED_PATHS: 'relative/path',
      REQUEST_TIMEOUT: 'fast',
      OCTOCODE_OUTPUT_FORMAT: 'xml',
    });

    expect(resolved).toEqual(
      expect.objectContaining({
        version: 1,
        github: { apiUrl: 'https://api.github.com', graphqlEnabled: true },
        tools: { enabled: null, disabled: null },
        network: expect.objectContaining({ timeout: 30_000 }),
        output: expect.objectContaining({ format: 'yaml' }),
      })
    );
  });

  it('accepts contract-valid URL, path, enum, and numeric values from each source', () => {
    const resolved = resolveConfigFields(
      {
        version: 1,
        github: { apiUrl: 'http://ghe.example.test/api/v3' },
        local: { workspaceRoot: 'C:\\workspace', allowedPaths: ['~/src'] },
        output: { format: 'json' },
      },
      {
        REQUEST_TIMEOUT: '60000ms',
        OCTOCODE_LSP_CONFIG: ' /tmp/lsp.json ',
      }
    );

    expect(resolved.github.apiUrl).toBe('http://ghe.example.test/api/v3');
    expect(resolved.local.workspaceRoot).toBe('C:\\workspace');
    expect(resolved.network.timeout).toBe(60_000);
    expect(resolved.lsp.configPath).toBe('/tmp/lsp.json');
    expect(resolved.output.format).toBe('json');
  });
});
