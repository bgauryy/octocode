// tools/list and published-schema checks for stdio.acceptance.mjs: the MCP
// catalog matches core, stays within budget, and agrees with the CLI schemas.
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import path from 'node:path';
import {
  acceptanceCwd,
  acceptanceEnv,
  invoke,
  receipt,
  state,
  values,
  byCodeUnit,
} from './stdio-harness.mjs';

const {
  DIRECT_TOOL_DEFINITIONS,
  TOOL_NAMES,
  getDirectToolDefinitionsWithAddons,
  getPublicToolCatalogWithAddons,
  isCliOnlyTool,
} = await import('@octocodeai/config/schema');
const { publishedInputSchema } = await import('@octocodeai/config/mcp');
const canonicalTools = DIRECT_TOOL_DEFINITIONS.map(tool => tool.name);

const listsCanonicalTools = () =>
  assert.deepEqual(
    state.expectedTools
      .filter(name => name !== TOOL_NAMES.CLASIFY)
      .toSorted(byCodeUnit),
    canonicalTools
      .filter(name => name !== TOOL_NAMES.CLASIFY && !isCliOnlyTool(name))
      .toSorted(byCodeUnit)
  );

async function rejectsLegacyClasifyShape() {
  const response = await invoke(TOOL_NAMES.CLASIFY, {
    queries: [
      {
        id: 'legacy-shape-rejected',
        resources: [{ id: 'evidence', value: 'Supplied evidence.' }],
        questions: [
          { id: 'bounded', type: 'yesno', ask: 'Is evidence supplied?' },
        ],
      },
    ],
  });
  assert.equal(response.isError, true);
  // `resources` belongs to the `sections` form, so the first repair
  // offers either fix; the other issues stay listed in `details`.
  assert.deepEqual(response.structuredContent, {
    error: `'resources' in queries[0] applies only with operation:"sections": set it, or remove 'resources'.`,
    details: [
      'queries.0.questions: Expected object',
      'queries.0: Send exactly one of: state, source.',
    ],
    errorCode: 'invalidInput',
  });
}

const declaresSelection = schema => {
  if (!schema || typeof schema !== 'object') return false;
  if (Object.hasOwn(schema.properties ?? {}, 'clasifyContext')) return true;
  return Object.values(schema).some(declaresSelection);
};
const cliSchema = (name, ...args) =>
  JSON.parse(
    execFileSync(
      values.node,
      [path.resolve(values.cli), 'schema', name, ...args],
      {
        encoding: 'utf8',
        timeout: 10_000,
        cwd: acceptanceCwd,
        env: acceptanceEnv,
      }
    )
  );

function cliSchemasAgree() {
  // The CLI lists every tool it can run; MCP clasify may delegate only to the
  // tools MCP exposes, so its published schema is built for that tool set.
  const selection = state.list.tools.some(tool =>
    declaresSelection(tool.inputSchema)
  );
  const cliCatalog = new Map(
    getPublicToolCatalogWithAddons({ selection }).tools.map(tool => [
      tool.name,
      tool,
    ])
  );
  const cliDefinitions = new Map(
    getDirectToolDefinitionsWithAddons({ selection }).map(definition => [
      definition.name,
      definition,
    ])
  );
  const mcpDefinitions = new Map(
    getDirectToolDefinitionsWithAddons({
      availableTools: state.expectedTools,
      selection,
    }).map(definition => [definition.name, definition])
  );
  for (const tool of state.list.tools) {
    const cli = cliSchema(tool.name);
    assert.equal(cli.name, tool.name);
    assert.equal(cli.availability.enabled, true);
    // CLI discovery shows one compact query row by default; the full view
    // retains the canonical validation contract for complete inspection.
    assert.equal(Object.hasOwn(cli, 'inputSchema'), false);
    assert.equal(Object.hasOwn(cli, 'outputSchema'), false);
    const cliDefinition = cliDefinitions.get(tool.name);
    const mcpDefinition = mcpDefinitions.get(tool.name);
    assert.ok(
      cliDefinition && mcpDefinition,
      `${tool.name} has no canonical contract`
    );
    const { $schema, ...canonicalRow } = cliCatalog.get(tool.name).querySchema;
    const full = cliSchema(tool.name, '--view', 'full');
    assert.deepEqual(
      full.querySchema,
      canonicalRow,
      `${tool.name} CLI full schema differs from the canonical contract`
    );
    assert.deepEqual(
      cli.querySchema,
      publishedInputSchema(cliDefinition.inputSchema, 'row'),
      `${tool.name} CLI default differs from the published row schema`
    );
    assert.deepEqual(
      cliSchema(tool.name, '--view', 'query').querySchema,
      cli.querySchema,
      `${tool.name} CLI explicit query view differs from the default`
    );
    assert.deepEqual(
      tool.inputSchema,
      publishedInputSchema(mcpDefinition.inputSchema),
      `${tool.name} MCP input schema differs from the published contract`
    );
  }
}

/** The catalog checks, in run order, for the listed tools. */
export const catalogChecks = () => [
  [
    'initialize and list every available canonical direct tool',
    listsCanonicalTools,
  ],
  [
    'MCP tool catalog stays below the production transport budget',
    () =>
      assert.ok(
        receipt.catalogBytes <= 200_000,
        `serialized MCP catalog is ${receipt.catalogBytes} bytes`
      ),
  ],
  [
    'MCP tool catalog omits output schemas',
    () =>
      assert.ok(
        state.list.tools.every(tool => !Object.hasOwn(tool, 'outputSchema')),
        'tools/list exposed an outputSchema'
      ),
  ],
  ...(state.expectedTools.includes(TOOL_NAMES.CLASIFY)
    ? [
        [
          'clasify rejects the removed resources/presets shape before provider access',
          rejectsLegacyClasifyShape,
        ],
      ]
    : []),
  [
    'CLI full contracts and compact queries agree with MCP published schemas',
    cliSchemasAgree,
  ],
];
