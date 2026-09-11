/* v8 ignore file -- exercised through the built CLI and isolated-package subprocess tests */
import { writeCommandPayload, writeCommandText } from '../command-output.js';
import { awarenessEntityCatalog } from './entities.js';
import {
  AWARENESS_CONCEPTS,
  getAwarenessOperationDescriptor,
  listAwarenessOperationDescriptors,
  type AwarenessOperation,
} from './operation-catalog.js';

function canonicalOperationName(value: string): AwarenessOperation | undefined {
  const normalized = value.trim().replace(/\s+/, '.');
  return getAwarenessOperationDescriptor(normalized) ? normalized as AwarenessOperation : undefined;
}

function routineDiscovery() {
  const descriptors = listAwarenessOperationDescriptors();
  return {
    concepts: Object.fromEntries(AWARENESS_CONCEPTS.map(concept => [
      concept,
      descriptors.filter(row => row.concept === concept).map(row => row.operation.slice(concept.length + 1)),
    ])),
    operations: descriptors.map(row => row.operation),
    call: '<concept> <operation> [flags]',
    schema: 'schema command <concept> <operation>',
  };
}

/** Return the exact contract for one public Awareness operation. */
export function cliCommandSchema(operationName: string): Record<string, unknown> | null {
  const operation = canonicalOperationName(operationName);
  if (!operation) return null;
  const descriptor = getAwarenessOperationDescriptor(operation)!;
  return {
    ...structuredClone(descriptor.inputSchema),
    'x-awareness-operation': operation,
    'x-cli-command': operation.replace('.', ' '),
    'x-cli-context': ['db', 'db_scope', 'workspace', 'agent_id', 'session_id', 'compact'],
    'x-awareness-effect': descriptor.effects,
    'x-awareness-approval': 'parameter-sensitive',
    'x-awareness-output-budget': descriptor.outputBudget,
  };
}

function usage(): string {
  return `Usage:
  npx @octocodeai/octocode-awareness schema commands [--compact]
  npx @octocodeai/octocode-awareness schema command <concept> <operation> [--compact]
  npx @octocodeai/octocode-awareness schema entities [--compact] [--all]`;
}

function printJsonError(payload: Record<string, unknown>, compact = false): number {
  writeCommandPayload({ ok: false, ...payload }, compact);
  return 1;
}

function rejectUnknownParams(params: Record<string, unknown>, allowed: readonly string[]): string | undefined {
  const allowedSet = new Set(allowed);
  return Object.keys(params).find(key => !allowedSet.has(key));
}

/** Narrow schema discovery for the canonical operation surface. */
export async function runSchemaCommand(command: string | undefined, params: Record<string, unknown>): Promise<number> {
  const compact = params.compact === true;
  if (!command || command === '--help' || command === '-h') {
    writeCommandText(`${usage()}\n`);
    return 0;
  }

  if (command === 'commands') {
    const unknown = rejectUnknownParams(params, ['compact']);
    if (unknown) return printJsonError({ error_code: 'UNKNOWN_FLAG', error: `Unknown flag --${unknown.replaceAll('_', '-')}` }, compact);
    writeCommandPayload({
      ok: true,
      kind: 'awareness.cli-surface',
      hint: 'Call `<concept> <operation>` directly. Use `context orient` once.',
      ...routineDiscovery(),
    }, compact);
    return 0;
  }

  if (command === 'command') {
    const unknown = rejectUnknownParams(params, ['compact', 'concept', 'operation']);
    if (unknown) return printJsonError({ error_code: 'UNKNOWN_FLAG', error: `Unknown flag --${unknown.replaceAll('_', '-')}` }, compact);
    const requested = [params.concept, params.operation].filter(Boolean).join(' ');
    const schema = cliCommandSchema(requested);
    if (!schema) return printJsonError({
      error_code: 'UNKNOWN_OPERATION',
      error: `Unknown Awareness operation: ${requested || '<missing>'}`,
      hint: 'Use `schema commands --compact` for the complete surface.',
    }, compact);
    writeCommandPayload(schema, compact);
    return 0;
  }

  if (command === 'entities') {
    const unknown = rejectUnknownParams(params, ['compact', 'all']);
    if (unknown) return printJsonError({ error_code: 'UNKNOWN_FLAG', error: `Unknown flag --${unknown.replaceAll('_', '-')}` }, compact);
    const catalog = awarenessEntityCatalog();
    if (params.all === true) writeCommandPayload({ ok: true, kind: 'awareness.entities', ...catalog }, compact);
    else {
      const families = new Map<string, string[]>();
      for (const entity of catalog.entities) {
        const names = families.get(entity.family) ?? [];
        names.push(entity.name);
        families.set(entity.family, names);
      }
      writeCommandPayload({
        ok: true,
        kind: 'awareness.entities',
        storage: catalog.storage,
        families: [...families.entries()].map(([family, entities]) => ({ family, entities })),
        hint: 'Pass --all for owner and relation kind per entity.',
      }, compact);
    }
    return 0;
  }

  return printJsonError({ error_code: 'UNKNOWN_SCHEMA_ACTION', error: `Unknown schema action: ${command}`, hint: usage() }, compact);
}
