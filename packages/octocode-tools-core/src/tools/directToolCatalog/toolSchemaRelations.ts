import {
  getToolSchemaRelations,
  getToolSchemaVariants,
} from '@octocodeai/octocode-core/schemas';
import { z } from 'zod';
import { findDirectToolDefinition } from './toolCatalogDefinitions.js';

type JsonObject = Record<string, unknown>;

function isObject(value: unknown): value is JsonObject {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function collectObjectBranches(schema: JsonObject): JsonObject[] {
  if (isObject(schema.properties)) return [schema];
  const union = Array.isArray(schema.anyOf)
    ? schema.anyOf
    : Array.isArray(schema.oneOf)
      ? schema.oneOf
      : [];
  return union.filter(isObject).flatMap(collectObjectBranches);
}

function operationFields(toolName: string): Map<string, string[]> {
  const schema = findDirectToolDefinition(toolName)?.schema;
  if (!schema) return new Map();
  const jsonSchema = z.toJSONSchema(schema) as JsonObject;
  const fieldsByOperation = new Map<string, Set<string>>();
  for (const branch of collectObjectBranches(jsonSchema)) {
    if (!isObject(branch.properties)) continue;
    const operation = branch.properties.operation;
    if (!isObject(operation) || typeof operation.const !== 'string') continue;
    const fields = fieldsByOperation.get(operation.const) ?? new Set<string>();
    for (const field of Object.keys(branch.properties)) {
      if (!['goal', 'reasoning', 'operation'].includes(field))
        fields.add(field);
    }
    fieldsByOperation.set(operation.const, fields);
  }
  return new Map(
    [...fieldsByOperation].map(([operation, fields]) => [
      operation,
      [...fields],
    ])
  );
}

export function getDirectToolSchemaRelations(toolName: string): string[] {
  if (toolName === 'ghSearch') {
    const [scope, ...relations] = getToolSchemaRelations(toolName);
    return [
      ...(scope ? [scope] : []),
      'code and repositories need at least one search term or scope filter.',
      ...relations,
    ];
  }
  if (toolName === 'localSearch') {
    return [
      'text needs searchText.',
      'structural needs exactly one of pattern or rule.',
      'Fields from different operations cannot be mixed.',
    ];
  }
  if (toolName === 'ghSearchHistory') {
    return [
      'Choose pullRequests, issues, or commits; fields cannot cross operations.',
      'issues and commits require owner+repo; pullRequests may search globally.',
      'Search/list operations reject exact identities, content, and diff pagination.',
    ];
  }
  if (toolName === 'ghGetHistoryItem') {
    return [
      'Every operation requires owner+repo.',
      'pullRequest and issue need number; commit needs ref; compare needs base+head.',
      'Fields and pagination axes cannot cross operations.',
    ];
  }
  return getToolSchemaRelations(toolName);
}

export function getDirectToolSchemaVariants(toolName: string) {
  const fieldsByOperation = operationFields(toolName);
  const historyVariants =
    toolName === 'ghSearchHistory'
      ? [
          {
            name: 'pullRequests',
            when: 'Search or list pull requests',
            requires: ['operation'],
            example: { operation: 'pullRequests', keywords: ['schema'] },
          },
          {
            name: 'issues',
            when: 'Search or list issues',
            requires: ['operation', 'owner', 'repo'],
            example: { operation: 'issues', owner: 'o', repo: 'r' },
          },
          {
            name: 'commits',
            when: 'List commit history',
            requires: ['operation', 'owner', 'repo'],
            example: { operation: 'commits', owner: 'o', repo: 'r' },
          },
        ]
      : toolName === 'ghGetHistoryItem'
        ? [
            {
              name: 'pullRequest',
              when: 'Retrieve one pull request',
              requires: ['operation', 'owner', 'repo', 'number'],
              example: {
                operation: 'pullRequest',
                owner: 'o',
                repo: 'r',
                number: 1,
              },
            },
            {
              name: 'issue',
              when: 'Retrieve one issue',
              requires: ['operation', 'owner', 'repo', 'number'],
              example: {
                operation: 'issue',
                owner: 'o',
                repo: 'r',
                number: 1,
              },
            },
            {
              name: 'commit',
              when: 'Retrieve one commit',
              requires: ['operation', 'owner', 'repo', 'ref'],
              example: {
                operation: 'commit',
                owner: 'o',
                repo: 'r',
                ref: 'main',
              },
            },
            {
              name: 'compare',
              when: 'Compare two refs',
              requires: ['operation', 'owner', 'repo', 'base', 'head'],
              example: {
                operation: 'compare',
                owner: 'o',
                repo: 'r',
                base: 'main',
                head: 'next',
              },
            },
          ]
        : getToolSchemaVariants(toolName);
  return historyVariants.map(variant => ({
    ...variant,
    ...(toolName === 'localSearch' && variant.name === 'structural'
      ? { requires: ['operation', 'path'] }
      : {}),
    ...(toolName === 'lspGetSemantics' && variant.name === 'workspace'
      ? { requires: ['type', 'symbolName'] }
      : {}),
    ...(fieldsByOperation.has(variant.name)
      ? { fields: fieldsByOperation.get(variant.name) }
      : {}),
  }));
}
