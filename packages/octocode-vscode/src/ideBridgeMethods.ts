import * as fs from 'fs';
import * as path from 'path';
import * as vscode from 'vscode';

import { HttpError } from './ideBridgeHttp';
import type { Family, Json, Obj, Remember } from './ideBridgeTypes';
import {
  isObject,
  toDiagnostic,
  toHierarchyItem,
  toHover,
  toLocation,
  toRange,
  toSymbol,
} from './lspJson';

/** Which request fields a method reads. */
export type Shape = 'position' | 'document' | 'workspace' | 'item';

export type MethodSpec = {
  shape: Shape;
  /** Built-in VS Code command; absent for `diagnostics` (an API call). */
  command?: string;
  /** Hierarchy family: prepare* hands out items, the others consume them. */
  family?: Family;
  convert: (raw: unknown, remember: Remember) => Json;
};

const list = (raw: unknown): unknown[] => (Array.isArray(raw) ? raw : []);
const locations = (raw: unknown): Json => list(raw).map(toLocation);
const symbols = (raw: unknown): Json => list(raw).map(toSymbol);

function items(family: Family): MethodSpec['convert'] {
  return (raw, remember) =>
    list(raw).map(item => toHierarchyItem(item, remember(family, item)));
}

function calls(end: 'from' | 'to'): MethodSpec['convert'] {
  return (raw, remember) =>
    list(raw).map(call => {
      const { fromRanges, [end]: item } = call as Obj;
      return {
        [end]: toHierarchyItem(item, remember('call', item)),
        fromRanges: (fromRanges as unknown[]).map(toRange),
      };
    });
}

function providerSpec(
  shape: Shape,
  command: string,
  convert: MethodSpec['convert'],
  family?: Family
): MethodSpec {
  return { shape, command, convert, ...(family ? { family } : {}) };
}

const METHOD_SPECS: Record<string, MethodSpec> = {
  definition: providerSpec(
    'position',
    'vscode.executeDefinitionProvider',
    locations
  ),
  typeDefinition: providerSpec(
    'position',
    'vscode.executeTypeDefinitionProvider',
    locations
  ),
  implementation: providerSpec(
    'position',
    'vscode.executeImplementationProvider',
    locations
  ),
  references: providerSpec(
    'position',
    'vscode.executeReferenceProvider',
    locations
  ),
  hover: providerSpec('position', 'vscode.executeHoverProvider', raw =>
    toHover(list(raw))
  ),
  documentSymbols: providerSpec(
    'document',
    'vscode.executeDocumentSymbolProvider',
    symbols
  ),
  workspaceSymbol: providerSpec(
    'workspace',
    'vscode.executeWorkspaceSymbolProvider',
    symbols
  ),
  prepareCallHierarchy: providerSpec(
    'position',
    'vscode.prepareCallHierarchy',
    items('call'),
    'call'
  ),
  incomingCalls: providerSpec(
    'item',
    'vscode.provideIncomingCalls',
    calls('from'),
    'call'
  ),
  outgoingCalls: providerSpec(
    'item',
    'vscode.provideOutgoingCalls',
    calls('to'),
    'call'
  ),
  prepareTypeHierarchy: providerSpec(
    'position',
    'vscode.prepareTypeHierarchy',
    items('type'),
    'type'
  ),
  supertypes: providerSpec(
    'item',
    'vscode.provideSupertypes',
    items('type'),
    'type'
  ),
  subtypes: providerSpec(
    'item',
    'vscode.provideSubtypes',
    items('type'),
    'type'
  ),
  diagnostics: {
    shape: 'document',
    convert: raw => list(raw).map(toDiagnostic),
  },
};

export const IDE_BRIDGE_METHODS = Object.keys(METHOD_SPECS);

export function methodSpec(method: string): MethodSpec | undefined {
  return Object.hasOwn(METHOD_SPECS, method) ? METHOD_SPECS[method] : undefined;
}

export function requireString(body: Obj, key: string): string {
  const value = body[key];
  if (typeof value !== 'string') {
    throw new HttpError(400, `"${key}" must be a string`);
  }
  return value;
}

function requireAbsolutePath(body: Obj): string {
  const value = requireString(body, 'path');
  if (!path.isAbsolute(value)) {
    throw new HttpError(400, '"path" must be an absolute file path');
  }
  return value;
}

function requireOffset(body: Obj, key: 'line' | 'character'): number {
  const value = body[key];
  if (typeof value !== 'number' || !Number.isInteger(value) || value < 0) {
    throw new HttpError(400, `"${key}" must be a non-negative integer`);
  }
  return value;
}

/** The validated request fields a method reads. */
export type LspInput = {
  path?: string;
  position?: vscode.Position;
  query?: string;
  item?: { data: string };
};

function parseItem(body: Obj): { data: string } {
  const item = body.item;
  if (!isObject(item) || typeof item.data !== 'string') {
    throw new HttpError(
      400,
      '"item" must be an item returned by the matching prepare call, with its "data" handle'
    );
  }
  return { data: item.data };
}

export function parseInput(spec: MethodSpec, body: Obj): LspInput {
  if (spec.shape === 'item') return { item: parseItem(body) };
  if (spec.shape === 'workspace') {
    return {
      query: requireString(body, 'query'),
      ...(body.path === undefined ? {} : { path: requireAbsolutePath(body) }),
    };
  }
  const file = requireAbsolutePath(body);
  if (spec.shape === 'document') return { path: file };
  return {
    path: file,
    position: new vscode.Position(
      requireOffset(body, 'line'),
      requireOffset(body, 'character')
    ),
  };
}

async function realPathOr(file: string): Promise<string> {
  try {
    return await fs.promises.realpath(file);
  } catch {
    // Not on disk (yet): the lexical path is all there is to compare.
    return path.resolve(file);
  }
}

/**
 * Path-bearing requests are served only inside a workspace folder. Paths are
 * compared by real path, because the native client sends canonical,
 * symlink-resolved paths while the folder may be open under a symlinked
 * spelling. Returns the file under the folder's own spelling, which is the URI
 * VS Code knows the document by.
 */
export async function requireInWorkspace(file: string): Promise<vscode.Uri> {
  const realFile = await realPathOr(file);
  for (const folder of vscode.workspace.workspaceFolders ?? []) {
    if (folder.uri.scheme !== 'file') continue;
    const relative = path.relative(
      await realPathOr(folder.uri.fsPath),
      realFile
    );
    if (relative === '..' || relative.startsWith(`..${path.sep}`)) continue;
    if (path.isAbsolute(relative)) continue;
    return vscode.Uri.file(path.join(folder.uri.fsPath, relative));
  }
  throw new HttpError(
    403,
    'path is outside every workspace folder',
    `${file} is not inside a folder open in this editor`
  );
}

/**
 * A workspace-symbol scope may be a directory, which has no document to open:
 * the provider then runs on the query alone.
 */
export async function isOpenable(
  spec: MethodSpec,
  file: string
): Promise<boolean> {
  if (spec.shape !== 'workspace') return true;
  try {
    return !(await fs.promises.stat(file)).isDirectory();
  } catch {
    return true;
  }
}
