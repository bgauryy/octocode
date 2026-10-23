// clasify registration on the MCP surface: native resolves beta and
// provider availability; the adapter registers what the catalog reports.
import { afterEach, describe, expect, it } from 'vitest';
import { buildMcpInstructions } from '@octocodeai/config/mcp';
import {
  getDirectToolDefinitionsWithAddons,
  getNativeContractFingerprint,
  TOOL_NAMES,
} from '@octocodeai/config/schema';
import { createNativeMcp } from '../../src/native/index.js';
import {
  bindingFor,
  connectClient,
  FakeRuntime,
  tool,
} from './native-mcp-fixture.js';

afterEach(() => {
  FakeRuntime.last = undefined;
});

describe('createNativeMcp clasify discovery', () => {
  it.each([
    ['missing', undefined, false],
    ['blank', '   ', false],
  ])(
    'omits clasify from discovery with beta enabled when OCTOCODE_CLASSIFICATION_API is %s',
    async (_label, credential, nativeAvailable) => {
      // Native owns beta and credential resolution. This fixture supplies the catalog
      // availability state that the adapter must honor without reinterpreting it.
      const instance = await createNativeMcp({
        env: { OCTOCODE_BETA: '1', OCTOCODE_CLASSIFICATION_API: credential },
        binding: bindingFor(() => ({
          fingerprint: getNativeContractFingerprint(),
          tools: [
            tool('localFetch', true),
            tool(TOOL_NAMES.CLASIFY, nativeAvailable),
          ],
        })),
      });

      const client = await connectClient(instance, 'semantic-gate');

      const list = await client.listTools();
      expect(list.tools.map(t => t.name)).toEqual(['localFetch']);
      expect(list.tools.every(t => !Object.hasOwn(t, 'outputSchema'))).toBe(
        true
      );

      await client.close();
      await instance.close();
    }
  );

  it('exposes clasify when beta and OCTOCODE_CLASSIFICATION_API are enabled', async () => {
    const instance = await createNativeMcp({
      env: {
        OCTOCODE_BETA: '1',
        OCTOCODE_CLASSIFICATION_API: 'test-key',
      },
      binding: bindingFor(() => ({
        fingerprint: getNativeContractFingerprint(),
        tools: [tool('localFetch', true), tool(TOOL_NAMES.CLASIFY, true)],
      })),
    });

    const client = await connectClient(instance, 'semantic-gate');

    const list = await client.listTools();
    expect(list.tools.map(t => t.name)).toEqual([
      'localFetch',
      TOOL_NAMES.CLASIFY,
    ]);
    expect(list.tools.every(t => !Object.hasOwn(t, 'outputSchema'))).toBe(true);
    // Every core-authored hint survives the MCP registration and transport.
    const authored = getDirectToolDefinitionsWithAddons({
      availableTools: list.tools.map(listed => listed.name),
    });
    for (const listed of list.tools)
      expect(listed.annotations, listed.name).toEqual(
        authored.find(definition => definition.name === listed.name)!
          .annotations
      );

    await client.close();
    await instance.close();
  });
});

describe('createNativeMcp clasify provider recovery', () => {
  it('keeps clasify registered through provider failure and later recovery', async () => {
    const instance = await createNativeMcp({
      env: { OCTOCODE_BETA: '1', OCTOCODE_CLASSIFICATION_API: 'test-key' },
      binding: bindingFor(() => ({
        fingerprint: getNativeContractFingerprint(),
        tools: [tool('localFetch', true), tool(TOOL_NAMES.CLASIFY, true)],
      })),
    });
    const runtime = FakeRuntime.last!;
    expect(runtime.executions).toHaveLength(0);

    const client = await connectClient(instance, 'clasify-recovery');

    const list = await client.listTools();
    expect(list.tools.map(t => t.name)).toEqual([
      'localFetch',
      TOOL_NAMES.CLASIFY,
    ]);
    expect(client.getInstructions()).toBe(
      buildMcpInstructions({ availableTools: list.tools.map(t => t.name) })
    );
    const matrix = {
      mainGoal: 'Check provider recovery.',
      reasoning: 'The same host must retain the public tool.',
      resources: [{ value: 'fixture' }],
      questions: [{ type: 'yesno', ask: 'Is this a fixture?' }],
    };
    runtime.providerAvailable = false;
    const failed = await client.callTool({
      name: TOOL_NAMES.CLASIFY,
      arguments: { queries: [matrix] },
    });
    expect(failed.isError).toBe(true);
    expect(failed.structuredContent).toMatchObject({
      errorCode: 'classificationProviderError',
    });
    runtime.providerAvailable = true;
    const recovered = await client.callTool({
      name: TOOL_NAMES.CLASIFY,
      arguments: { queries: [matrix] },
    });
    expect(recovered.isError).toBe(false);
    expect(runtime.executions).toHaveLength(2);

    await client.close();
    await instance.close();
  });
});

describe('createNativeMcp clasify queries matrix', () => {
  it('accepts a clasify queries[] matrix and normalizes it before execution', async () => {
    const instance = await createNativeMcp({
      env: { OCTOCODE_BETA: '1', OCTOCODE_CLASSIFICATION_API: 'test-key' },
      binding: bindingFor(() => ({
        fingerprint: getNativeContractFingerprint(),
        tools: [tool('localFetch', true), tool(TOOL_NAMES.CLASIFY, true)],
      })),
    });
    const client = await connectClient(instance, 'clasify-queries');

    const matrix = {
      mainGoal: 'test goal',
      reasoning: 'Locate one fact without caller-authored IDs.',
      resources: [{ value: 'captured source' }],
      questions: [{ type: 'locate', ask: 'The fact to locate' }],
    };
    const response = await client.callTool({
      name: TOOL_NAMES.CLASIFY,
      arguments: { queries: [matrix] },
    });
    expect(response.isError).toBe(false);
    expect(FakeRuntime.last?.executions.at(-1)).toMatchObject({
      tool: TOOL_NAMES.CLASIFY,
      input: { queries: [matrix] },
    });
    // Text-only clients must not receive an empty content array.
    expect(response.content).toEqual([
      { type: 'text', text: JSON.stringify(response.structuredContent) },
    ]);

    await client.close();
    await instance.close();
  });
});

describe('createNativeMcp clasify semantic checks', () => {
  it.each([
    ['disabled', false],
    ['enabled', true],
  ])(
    'keeps semantic checks in clasify when provider capability is %s',
    async (_label, clasifyAvailable) => {
      const instance = await createNativeMcp({
        env: clasifyAvailable
          ? {
              OCTOCODE_BETA: '1',
              OCTOCODE_CLASSIFICATION_API: 'test-key',
              OCTOCODE_CLASSIFICATION_API_HOST:
                'https://classification.example.test',
            }
          : {
              OCTOCODE_BETA: '1',
              OCTOCODE_CLASSIFICATION_API_HOST:
                'https://classification.example.test',
            },
        binding: bindingFor(() => ({
          fingerprint: getNativeContractFingerprint(),
          tools: [
            tool('ghSearchCode', true),
            tool('localSearch', true),
            tool(TOOL_NAMES.CLASIFY, clasifyAvailable),
          ],
        })),
      });
      const client = await connectClient(instance, 'semantic-rerank-addon');
      const listed = await client.listTools();
      // Shared guidance belongs to initialize.instructions, never each tool.
      const sharedOpening = client.getInstructions()?.split('\n')[0];
      expect(sharedOpening).toBeTruthy();
      for (const registered of listed.tools) {
        expect(registered.description).toBeTruthy();
        expect(registered.description).not.toContain(sharedOpening!);
      }
      expect(
        listed.tools.some(candidate => candidate.name === TOOL_NAMES.CLASIFY)
      ).toBe(clasifyAvailable);
      for (const name of ['ghSearchCode', 'localSearch']) {
        const schema = JSON.stringify(
          listed.tools.find(candidate => candidate.name === name)?.inputSchema
        );
        expect(schema.includes('semanticRerank')).toBe(false);
        expect(schema).not.toContain('minScore');
        expect(schema).not.toContain('clasifyContext');
        expect(schema).not.toContain('clasifyMinScore');
      }
      await client.close();
      await instance.close();
    }
  );
});
