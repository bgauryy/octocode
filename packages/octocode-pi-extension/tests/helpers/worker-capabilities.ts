import assert from 'node:assert/strict';
import path from 'node:path';
import { afterEach, vi } from 'vitest';
import type { CapabilitySnapshot } from '@octocodeai/agent-contracts/capabilities';
import * as workerCapabilities from '../../src/tools/worker-capabilities.js';
import { discoverSkills } from '../../src/tools/skill-discovery.js';
import { getAssetPaths } from '../../src/assets.js';
import { createWorkerBrokerClient, createWorkerMcpBroker, type WorkerMcpBroker } from '../../src/tools/mcp/broker.js';

const fixtureBrokers: WorkerMcpBroker[] = [];
afterEach(async () => { await Promise.all(fixtureBrokers.splice(0).map(broker => broker.dispose())); });

export async function initializePackageWorkerCapabilities(nativeTools: string[]): Promise<void> {
  const skills = discoverSkills('/repo');
  await workerCapabilities.initializeWorkerCapabilityRuntime({
    snapshot: {
      schemaVersion: 1, revision: 'package-worker-fixture', nativeTools,
      skills: skills.map(skill => ({ id: skill.sourceId ?? skill.path!, name: skill.name, path: skill.path!, description: skill.description })),
      mcpTools: [{ server: 'octocode', tool: 'localSearch' }],
    },
    dispatchMcp: async () => ({ content: [{ type: 'text', text: 'package worker fixture transport' }] }),
  });
}

export function assertFocusedWorkerSkills(skillArgs: string[]): void {
  const names = skillArgs.map(skillPath => path.basename(path.dirname(skillPath)));
  assert.ok(names.every(name => ['octocode-research', 'octocode-awareness', 'octocode-rfc-generator', 'octocode-architect'].includes(name)), 'workers receive only focused enabled role skills');
  assert.ok(!names.includes('octocode-brainstorming'), 'unrelated enabled skills stay outside the worker grant');
}

/** Real authenticated parent/client transport, adapted into the in-process host harness. */
export async function installAuthenticatedWorkerCapabilityView(nativeTools: string[], includeSkill = true): Promise<WorkerMcpBroker> {
  const snapshot: CapabilitySnapshot = {
    schemaVersion: 1, revision: 'worker-projection-fixture', nativeTools,
    skills: includeSkill ? [{ id: 'research-fixture', name: 'octocode-research', path: path.join(getAssetPaths().skillsDir, 'octocode-research', 'SKILL.md'), description: 'Evidence-first research.' }] : [],
    mcpTools: [{ server: 'octocode', tool: 'localSearch', description: 'Granted local search.', inputSchema: { type: 'object' } }],
  };
  const broker = await createWorkerMcpBroker({ snapshot, dispatchMcp: async () => ({ content: [{ type: 'text', text: 'authenticated parent fixture' }] }) });
  fixtureBrokers.push(broker);
  const client = createWorkerBrokerClient(broker.registerWorker('prompt-worker', { nativeTools, skills: snapshot.skills.map(skill => skill.id), mcpTools: snapshot.mcpTools.map(({ server, tool }) => ({ server, tool })) }));
  let view = await client.readCapabilities(true);
  vi.spyOn(workerCapabilities, 'refreshCurrentWorkerCapabilities').mockImplementation(async options => {
    view = await client.readCapabilities(options?.beginTurn, options?.signal);
    return view;
  });
  vi.spyOn(workerCapabilities, 'getCurrentWorkerCapabilities').mockImplementation(() => view);
  return broker;
}

export async function assertRevokedWorkerPrompt(
  broker: WorkerMcpBroker,
  previousPrompt: string,
  beforeStart: (event: unknown, ctx: unknown) => unknown | Promise<unknown>,
  ctx: unknown,
  activeTools: () => string[],
): Promise<void> {
  broker.configureWorker('prompt-worker', { snapshotRevision: broker.getSnapshot().revision, selection: { nativeTools: [], skills: [], mcpTools: [] } });
  const revised = await beforeStart({ systemPrompt: previousPrompt }, ctx) as { systemPrompt?: string } | undefined;
  assert.deepEqual(activeTools(), [], 'revoked native tools leave the active Pi palette');
  assert.doesNotMatch(revised?.systemPrompt ?? '', /<available_skills>|<mcp_catalog_index>|<awareness>|<awareness_cli_runtime>/, 'the next worker turn removes revoked descriptions and skills');
}
