export type PromptPlacement = 'system' | 'before-user' | 'after-user';

export interface PromptFragment {
  readonly id: string;
  readonly placement: PromptPlacement;
  readonly priority: number;
  readonly content: string;
  readonly provenance: string;
  readonly trusted: boolean;
}

export interface PromptSection {
  readonly placement: PromptPlacement;
  readonly content: string;
  readonly fragmentIds: readonly string[];
  readonly bytes: number;
}

export interface PromptSnapshot {
  readonly schemaVersion: 1;
  readonly text: string;
  readonly sections: readonly PromptSection[];
  readonly fragments: readonly {
    readonly id: string;
    readonly placement: PromptPlacement;
    readonly provenance: string;
    readonly bytes: number;
  }[];
  readonly bytes: number;
  readonly semanticDigest: string;
}

export interface PromptAssemblyOptions {
  readonly maxBytes?: number;
}

export const assemblePrompt = (
  fragments: readonly PromptFragment[],
  options: PromptAssemblyOptions = {},
): PromptSnapshot => {
  const ordered = [...fragments].sort(
    (a, b) => placementOrder(a.placement) - placementOrder(b.placement)
      || a.priority - b.priority
      || a.id.localeCompare(b.id),
  );
  const ids = new Set<string>();
  for (const fragment of ordered) {
    if (!fragment.trusted) throw new Error(`Untrusted prompt fragment: ${fragment.id}`);
    if (!fragment.id.trim()) throw new Error('Prompt fragment id is required');
    if (ids.has(fragment.id)) throw new Error(`duplicate prompt fragment id ${fragment.id}`);
    ids.add(fragment.id);
  }

  const sections = (['system', 'before-user', 'after-user'] as const).flatMap((placement) => {
    const matching = ordered.filter((fragment) => fragment.placement === placement);
    if (matching.length === 0) return [];
    const content = matching.map((fragment) => fragment.content).join('\n\n');
    return [{ placement, content, fragmentIds: matching.map((fragment) => fragment.id), bytes: byteLength(content) }];
  });
  const text = sections.map((section) => section.content).join('\n\n');
  const bytes = byteLength(text);
  if (options.maxBytes !== undefined && bytes > options.maxBytes) {
    throw new Error(`prompt exceeds byte budget ${options.maxBytes} (actual ${bytes})`);
  }
  return {
    schemaVersion: 1,
    text,
    sections,
    fragments: ordered.map(({ id, placement, provenance, content }) => ({ id, placement, provenance, bytes: byteLength(content) })),
    bytes,
    semanticDigest: fnv1a(text),
  };
};

const placementOrder = (placement: PromptPlacement): number => placement === 'system' ? 0 : placement === 'before-user' ? 1 : 2;
const byteLength = (value: string): number => new TextEncoder().encode(value).byteLength;
const fnv1a = (value: string): string => {
  let hash = 0x811c9dc5;
  for (const byte of new TextEncoder().encode(value)) {
    hash ^= byte;
    hash = Math.imul(hash, 0x01000193);
  }
  return (hash >>> 0).toString(16).padStart(8, '0');
};
