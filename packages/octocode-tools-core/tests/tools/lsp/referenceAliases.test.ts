import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { pathToFileURL } from 'node:url';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { LspSearchQuerySchema } from '@octocodeai/octocode-core/schema';
import { dispatchAnchoredSemantic } from '../../../src/tools/lsp/semantic_content/semanticAnchored.js';
import {
  resolveSymbolAnchor,
  type SymbolAnchor,
} from '../../../src/tools/lsp/shared/resolveSymbolAnchor.js';
import {
  failedAnchorEnvelope,
  emptyEnvelope,
} from '../../../src/tools/lsp/semantic_content/semanticEnvelopes/envelopeHelpers.js';
import { classifySemanticResult } from '../../../src/tools/lsp/semantic_content/semanticPresentation.js';
import { contextUtils } from '../../../src/utils/contextUtils.js';

const roots: string[] = [];
afterEach(async () => {
  vi.restoreAllMocks();
  await Promise.all(
    roots.splice(0).map(root => rm(root, { recursive: true, force: true }))
  );
});

async function fixture(
  consumer = 'from origin import target as alias\nresult = alias(1)\n'
) {
  const root = await mkdtemp(path.join(process.cwd(), '.reference-alias-'));
  roots.push(root);
  const origin = path.join(root, 'origin.py');
  const use = path.join(root, 'consumer.py');
  const content = 'def target(value):\n    return value\n';
  await Promise.all([writeFile(origin, content), writeFile(use, consumer)]);
  const declaration = {
    uri: pathToFileURL(origin).href,
    range: {
      start: { line: 0, character: 4 },
      end: { line: 0, character: 10 },
    },
    content: 'def target(value):',
  };
  const imported = {
    uri: pathToFileURL(use).href,
    range: {
      start: { line: 0, character: 19 },
      end: { line: 0, character: 25 },
    },
    content: consumer.split('\n')[0]!,
  };
  const importLine = consumer
    .split('\n')
    .findIndex(line => line.includes('target'));
  if (importLine >= 0) {
    const character = consumer.split('\n')[importLine]!.indexOf('target');
    imported.range = {
      start: { line: importLine, character },
      end: { line: importLine, character: character + 6 },
    };
  }
  const anchor: SymbolAnchor = {
    uri: declaration.uri,
    absolutePath: origin,
    content,
    resolvedSymbol: {
      name: 'target',
      uri: declaration.uri,
      range: declaration.range,
      position: declaration.range.start,
      foundAtLine: 1,
    },
  };
  const client = {
    hasCapability: vi.fn((_capability: string) => true),
    findReferences: vi.fn(async () => [declaration, imported]),
    gotoDefinition: vi.fn(async (_file: string) => [declaration]),
  };
  const query = {
    uri: origin,
    operation: 'references' as const,
    symbolName: 'target',
    lineHint: 1,
    workspaceRoot: root,
  };
  return { root, use, declaration, imported, anchor, client, query };
}

describe('provider-scoped reference alias follow-ups', () => {
  it('keeps references when alias identity verification fails', async () => {
    const f = await fixture();
    f.client.gotoDefinition.mockRejectedValue(
      new Error('language server disconnected')
    );
    const result = await dispatchAnchoredSemantic(
      f.query,
      f.anchor,
      f.client as never
    );
    expect(result.payload).toMatchObject({
      totalReferences: 2,
      coverage: { unverifiedAliasBindings: 1 },
    });
    expect(result.terminalLimit).toBe(true);
    expect(result.next?.searchAliasReferences0).toBeUndefined();
  });

  it('reports aliases as unverified when the provider lacks definition capability', async () => {
    const f = await fixture();
    f.client.hasCapability.mockImplementation(
      (capability: string) => capability !== 'definitionProvider'
    );
    const result = await dispatchAnchoredSemantic(
      f.query,
      f.anchor,
      f.client as never
    );
    expect(result.payload).toMatchObject({
      totalReferences: 2,
      coverage: { unverifiedAliasBindings: 1 },
    });
    expect(f.client.gotoDefinition).not.toHaveBeenCalled();
    expect(result.next?.searchAliasReferences0).toBeUndefined();
  });

  it.each(['throws', 'malformed'] as const)(
    'preserves provider references when syntax extraction %s',
    async mode => {
      const f = await fixture();
      vi.spyOn(contextUtils, 'extractGraphFacts').mockImplementation(() => {
        if (mode === 'throws') throw new Error('parser unavailable');
        return '{invalid';
      });
      const result = await dispatchAnchoredSemantic(
        f.query,
        f.anchor,
        f.client as never
      );
      expect(result.payload).toMatchObject({
        totalReferences: 2,
        coverage: { uninspectedFiles: 2 },
      });
      expect(f.client.gotoDefinition).not.toHaveBeenCalled();
      expect(result.next?.searchAliasReferences0).toBeUndefined();
    }
  );

  it('reports an empty type hierarchy with its own typed category', async () => {
    const f = await fixture();
    const client = { ...f.client, prepareTypeHierarchy: vi.fn(async () => []) };
    const result = await dispatchAnchoredSemantic(
      { ...f.query, operation: 'supertypes' },
      f.anchor,
      client as never
    );
    expect(result.payload).toMatchObject({
      kind: 'empty',
      category: 'noTypeHierarchy',
    });
  });

  it('uses explicit empty categories independently of explanation wording', async () => {
    const f = await fixture();
    const result = emptyEnvelope(
      'definition',
      f.anchor,
      'Nothing supplied',
      'unsupportedOperation',
      true
    );
    expect(result.payload).toMatchObject({
      kind: 'empty',
      category: 'unsupportedOperation',
    });
  });

  it('does not claim equivalent identity from partially overlapping definition sets', async () => {
    const f = await fixture();
    f.client.gotoDefinition.mockImplementation(async file =>
      file === f.use
        ? [
            f.declaration,
            {
              ...f.declaration,
              uri: pathToFileURL(path.join(f.root, 'other.py')).href,
            },
          ]
        : [f.declaration]
    );
    const result = await dispatchAnchoredSemantic(
      f.query,
      f.anchor,
      f.client as never
    );
    expect(result.payload).toMatchObject({
      coverage: { unverifiedAliasBindings: 1 },
    });
    expect(result.next?.searchAliasReferences0).toBeUndefined();
  });

  it.each([
    ['prefix = 1; value = alias', 'alias', 0],
    ['prefix = alias; value = alias', 'alias', 1],
    [
      'prefix = "😀"; target\u0301 = 1; value = target\u0301',
      'target\u0301',
      1,
    ],
  ] as const)(
    'binds exact position within %s',
    async (content, name, orderHint) => {
      const f = await fixture(content);
      const position = { line: 0, character: content.lastIndexOf(name) };
      const result = await resolveSymbolAnchor(
        { uri: f.use, operation: 'definition', position } as never,
        'lspSearch'
      );
      expect(result.ok).toBe(true);
      if (result.ok)
        expect(result.value.resolvedSymbol).toMatchObject({
          name,
          position,
          orderHint,
        });
    }
  );

  it('classifies anchor failure as unresolved rather than semantic absence', async () => {
    const f = await fixture();
    const result = classifySemanticResult(
      failedAnchorEnvelope(
        f.query,
        'The supplied position is outside the source.',
        'anchorFailed'
      )
    );
    expect(result).toMatchObject({
      status: 'empty',
      confidence: 'low',
      payload: { kind: 'empty', category: 'anchorFailed' },
    });
  });

  it.each(['original', 'alias'] as const)(
    'reports an empty %s definition result as unverified identity',
    async empty => {
      const f = await fixture();
      f.client.gotoDefinition.mockImplementation(async file =>
        (empty === 'alias') === (file === f.use) ? [] : [f.declaration]
      );
      const result = await dispatchAnchoredSemantic(
        f.query,
        f.anchor,
        f.client as never
      );
      expect(result.payload).toMatchObject({
        coverage: { unverifiedAliasBindings: 1 },
      });
      expect(result.next?.searchAliasReferences0).toBeUndefined();
      expect(result.incompleteResults).toBe(true);
    }
  );

  it('reports uninspected files without claiming an alias or terminal limit', async () => {
    const f = await fixture();
    vi.spyOn(contextUtils, 'extractGraphFacts').mockReturnValue(null);
    const result = await dispatchAnchoredSemantic(
      f.query,
      f.anchor,
      f.client as never
    );
    expect(result.payload).toMatchObject({
      coverage: {
        scope: 'languageServer',
        exhaustive: false,
        uninspectedFiles: 2,
      },
    });
    expect(result.incompleteResults).toBe(true);
    expect(result.terminalLimit).toBe(true);
  });

  it('does not guess an alias position when native syntax cannot provide its ranges', async () => {
    const f = await fixture();
    const facts = JSON.parse(
      contextUtils.extractGraphFacts(
        'from origin import target as alias\n',
        f.use
      )!
    );
    for (const binding of facts.imports) {
      delete binding.importedRange;
      delete binding.localRange;
    }
    vi.spyOn(contextUtils, 'extractGraphFacts').mockImplementation(
      (_content, file) => (file === f.use ? JSON.stringify(facts) : null)
    );
    const result = await dispatchAnchoredSemantic(
      f.query,
      f.anchor,
      f.client as never
    );
    expect(result.payload).toMatchObject({
      coverage: { unverifiedAliasBindings: 1 },
    });
    expect(result.terminalLimit).toBe(true);
    expect(result.next?.searchAliasReferences0).toBeUndefined();
    expect(f.client.gotoDefinition).not.toHaveBeenCalled();
  });

  it('collects verified grouped aliases before grouped pagination', async () => {
    const f = await fixture();
    const result = await dispatchAnchoredSemantic(
      { ...f.query, groupByFile: true },
      f.anchor,
      f.client as never
    );
    expect(result.next?.searchAliasReferences0).toBeUndefined();
    expect(result.payload).toMatchObject({
      coverage: { verifiedAliasBindings: 1 },
    });
  });

  it('collects a multiline import before pagination', async () => {
    const f = await fixture(
      'from origin import (\n    target as alias,\n)\nresult = alias(1)\n'
    );
    const result = await dispatchAnchoredSemantic(
      f.query,
      f.anchor,
      f.client as never
    );
    expect(result.next?.searchAliasReferences0).toBeUndefined();
    expect(result.payload).toMatchObject({
      coverage: { verifiedAliasBindings: 1 },
    });
  });

  it('verifies all import bindings on the result page beyond five aliases', async () => {
    const f = await fixture(
      Array.from(
        { length: 6 },
        (_, index) => `from origin import target as alias${index}\n`
      ).join('')
    );
    f.client.findReferences.mockResolvedValue([
      f.declaration,
      ...Array.from({ length: 6 }, (_, line) => ({
        ...f.imported,
        range: { start: { line, character: 19 }, end: { line, character: 25 } },
      })),
    ]);
    const result = await dispatchAnchoredSemantic(
      f.query,
      f.anchor,
      f.client as never
    );
    expect(f.client.gotoDefinition).toHaveBeenCalledTimes(7);
    expect(result.payload).toMatchObject({
      coverage: {
        verifiedAliasBindings: 6,
      },
    });
    expect(result.incompleteResults).toBeUndefined();
    expect(result.terminalLimit).toBeUndefined();
    expect(result.partialReasons).toBeUndefined();
    for (const continuation of Object.values(result.next ?? {})) {
      expect(LspSearchQuerySchema.safeParse(continuation.query).success).toBe(
        true
      );
    }
  });

  it('verifies same-line alias imports only on their owning reference page', async () => {
    const content =
      'from origin import target as first, target as second\nfirst(1); second(2)\n';
    const f = await fixture(content);
    const imported = [19, 36].map(character => ({
      ...f.imported,
      range: {
        start: { line: 0, character },
        end: { line: 0, character: character + 6 },
      },
    }));
    f.client.findReferences.mockResolvedValue([...imported, f.declaration]);
    const positions = [];
    for (const page of [1, 2, 3]) {
      const result = await dispatchAnchoredSemantic(
        { ...f.query, page, pageSize: 1 },
        f.anchor,
        f.client as never
      );
      expect(result.next?.searchAliasReferences0).toBeUndefined();
      positions.push(result.payload.coverage?.verifiedAliasBindings);
    }
    expect(positions).toEqual([2, 2, 2]);
    expect(f.client.gotoDefinition).toHaveBeenCalledTimes(9);
  });

  it('offers bounded ungrouped pages when a grouped file has many bindings', async () => {
    const f = await fixture(
      'from origin import target as first, target as second\n'
    );
    f.client.findReferences.mockResolvedValue(
      [19, 36].map(character => ({
        ...f.imported,
        range: {
          start: { line: 0, character },
          end: { line: 0, character: character + 6 },
        },
      }))
    );
    const result = await dispatchAnchoredSemantic(
      { ...f.query, groupByFile: true, pageSize: 1, includeDeclaration: false },
      f.anchor,
      f.client as never
    );
    expect(result.next?.nextAliasReferences).toBeUndefined();
    expect(result.incompleteResults).toBeUndefined();
    expect(result.terminalLimit).toBeUndefined();
    expect(f.client.gotoDefinition).toHaveBeenCalledTimes(3);
    expect(result.payload).toMatchObject({
      coverage: { verifiedAliasBindings: 2 },
    });
  });

  it('merges an absent alias use before pagination', async () => {
    const f = await fixture();
    const aliasUse = {
      ...f.imported,
      range: {
        start: { line: 1, character: 9 },
        end: { line: 1, character: 14 },
      },
    };
    f.client.findReferences.mockImplementation(async (_file, position) =>
      position?.character === 29 ? [aliasUse] : [f.declaration, f.imported]
    );
    const result = await dispatchAnchoredSemantic(
      f.query,
      f.anchor,
      f.client as never
    );
    expect(result.payload).toMatchObject({
      kind: 'references',
      totalReferences: 3,
      coverage: {
        scope: 'languageServer',
        exhaustive: false,
        verifiedAliasBindings: 1,
      },
    });
    expect(result.payload.locations).toContainEqual(
      expect.objectContaining({
        uri: pathToFileURL(f.use).href,
        range: aliasUse.range,
      })
    );
    expect(result.next?.searchAliasReferences0).toBeUndefined();
    // The verified alias is collected before the response is paginated, rather
    // than requiring the caller to discover it through a separate follow-up.
    expect(f.client.findReferences).toHaveBeenCalledTimes(2);
    expect(f.client.gotoDefinition).toHaveBeenCalledTimes(2);
  });

  it('executes ordinary and grouped pages across merged alias references', async () => {
    const f = await fixture();
    const aliasUse = {
      ...f.imported,
      range: {
        start: { line: 1, character: 9 },
        end: { line: 1, character: 14 },
      },
    };
    f.client.findReferences.mockImplementation(async (_file, position) =>
      position?.character === 29 ? [aliasUse] : [f.declaration, f.imported]
    );
    const first = await dispatchAnchoredSemantic(
      { ...f.query, pageSize: 1 },
      f.anchor,
      f.client as never
    );
    const second = await dispatchAnchoredSemantic(
      {
        ...f.query,
        page: first.pagination!.nextPage!,
        pageSize: 1,
        snapshot: first.pagination!.snapshot,
      } as never,
      f.anchor,
      f.client as never
    );
    const third = await dispatchAnchoredSemantic(
      {
        ...f.query,
        page: second.pagination!.nextPage!,
        pageSize: 1,
        snapshot: second.pagination!.snapshot,
      } as never,
      f.anchor,
      f.client as never
    );
    const union = [first, second, third].flatMap(
      result => result.payload.locations ?? []
    );
    expect(union).toHaveLength(3);
    expect(union).toContainEqual(
      expect.objectContaining({
        uri: pathToFileURL(f.use).href,
        range: aliasUse.range,
      })
    );
    const grouped = await dispatchAnchoredSemantic(
      { ...f.query, groupByFile: true, pageSize: 1 },
      f.anchor,
      f.client as never
    );
    const groupedNext = await dispatchAnchoredSemantic(
      {
        ...f.query,
        groupByFile: true,
        page: grouped.pagination!.nextPage!,
        pageSize: 1,
        snapshot: grouped.pagination!.snapshot,
      } as never,
      f.anchor,
      f.client as never
    );
    expect(
      [
        ...(grouped.payload.byFile ?? []),
        ...(groupedNext.payload.byFile ?? []),
      ].flatMap(group => group.lines)
    ).toContain(2);
  });

  it('caps alias verification with typed incomplete state', async () => {
    const f = await fixture(
      Array.from(
        { length: 40 },
        (_, index) => `from origin import target as alias${index}`
      ).join('\n')
    );
    f.client.findReferences.mockResolvedValue([
      f.declaration,
      ...Array.from({ length: 40 }, (_, line) => ({
        ...f.imported,
        range: { start: { line, character: 19 }, end: { line, character: 25 } },
      })),
    ]);
    const result = await dispatchAnchoredSemantic(
      f.query,
      f.anchor,
      f.client as never
    );
    expect(result.payload).toMatchObject({
      coverage: { verifiedAliasBindings: 32, unverifiedAliasBindings: 8 },
    });
    expect(result.incompleteResults).toBe(true);
    expect(result.partialReasons).toContain('aliasReferences');
    expect(result.terminalLimit).toBe(true);
  });

  it('bounds distinct alias-reference files and reports omitted files', async () => {
    const f = await fixture();
    const files = await Promise.all(Array.from({ length: 101 }, async (_, index) => {
      const file = path.join(f.root, `consumer-${index}.py`);
      await writeFile(file, 'from origin import target as alias\nalias(1)\n');
      return {
        uri: pathToFileURL(file).href,
        range: { start: { line: 0, character: 19 }, end: { line: 0, character: 25 } },
      };
    }));
    f.client.findReferences.mockResolvedValue(files);
    const extract = vi.spyOn(contextUtils, 'extractGraphFacts');
    const result = await dispatchAnchoredSemantic(f.query, f.anchor, f.client as never);
    expect(extract).toHaveBeenCalledTimes(100);
    expect(result.payload).toMatchObject({
      coverage: { verifiedAliasBindings: 32, uninspectedFiles: 1 },
    });
    expect(result.incompleteResults).toBe(true);
    expect(result.partialReasons).toContain('aliasReferences');
    expect(result.terminalLimit).toBe(true);
  });

  it('rejects an alias candidate whose semantic definition differs', async () => {
    const f = await fixture();
    f.client.gotoDefinition.mockImplementation(async file => [
      file === f.use
        ? {
            ...f.declaration,
            uri: pathToFileURL(path.join(f.root, 'other.py')).href,
          }
        : f.declaration,
    ]);
    const result = await dispatchAnchoredSemantic(
      f.query,
      f.anchor,
      f.client as never
    );
    expect(result.next?.searchAliasReferences0).toBeUndefined();
  });

  it('does not repeat alias work already represented by the provider', async () => {
    const f = await fixture();
    f.client.findReferences.mockResolvedValue([
      f.declaration,
      f.imported,
      {
        ...f.imported,
        range: {
          start: { line: 0, character: 29 },
          end: { line: 0, character: 34 },
        },
      },
    ]);
    const result = await dispatchAnchoredSemantic(
      f.query,
      f.anchor,
      f.client as never
    );
    expect(result.next?.searchAliasReferences0).toBeUndefined();
    expect(f.client.gotoDefinition).not.toHaveBeenCalled();
  });
});
