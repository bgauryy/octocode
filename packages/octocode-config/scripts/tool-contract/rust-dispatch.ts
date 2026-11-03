// Typed continuation calls for native: one `ToolQuery` variant per tool and
// one channel enum per tool, both generated from the contract, so a call
// that names a tool with another tool's query, or a channel the tool does not
// declare, fails to compile instead of failing at publication.
import { pascal } from './json.ts';
import { typeName } from './variant-names.ts';

/** One continuation channel as core declares it (`continuationChannels`). */
export type ChannelDefinition =
  | 'linear'
  | 'branch'
  | 'numbered'
  | { readonly kind: 'branch'; readonly owner: readonly string[] };

export type ErrorChannels = Readonly<Record<string, ChannelDefinition>>;

export type ChannelRegistry = Readonly<
  Record<
    string,
    {
      readonly row: Readonly<Record<string, ChannelDefinition>>;
      readonly collections: readonly string[];
    }
  >
>;

/** The names of the contract's tool records, each checked as text. */
export function toolNames(records: readonly Readonly<Record<string, unknown>>[]): string[] {
  return records.map(({ name }) => {
    if (typeof name !== 'string') throw new TypeError('A contract tool has no name');
    return name;
  });
}

/** The generated query type of a tool: `localSearch` → `LocalSearchQuery`. */
export const queryTypeName = (tool: string): string => `${pascal(tool)}Query`;

/** The generated success-row data type of a tool: `localSearch` → `LocalSearchData`. */
export const dataTypeName = (tool: string): string => `${pascal(tool)}Data`;

/** The generated error-row data type of a tool, where core registers one: `ghCloneRepo` → `GhCloneRepoErrorData`. */
export const errorDataTypeName = (tool: string): string => `${pascal(tool)}ErrorData`;

/** One named branch of a tool's query union: `LocalFetchQuerySymbols` of `localFetch`. */
export type QueryBranch = { readonly tool: string; readonly type: string };

/**
 * The branch types typify converts into each tool's query union
 * (`impl From<Branch> for <Tool>Query`), read from its output.
 */
export function queryBranches(generated: string, tools: readonly string[]): QueryBranch[] {
  const byQuery = new Map(tools.map(tool => [queryTypeName(tool), tool]));
  const branches: QueryBranch[] = [];
  for (const [, type, query] of generated.matchAll(/impl\s+::std::convert::From<(\w+)>\s+for\s+(\w+)\s*\{/g)) {
    const tool = query === undefined ? undefined : byQuery.get(query);
    if (tool !== undefined && type !== undefined) branches.push({ tool, type });
  }
  return branches;
}

const variant = (key: string): string => typeName(key.replace(/([a-z0-9])([A-Z])/g, '$1-$2'));
const str = (value: string): string => JSON.stringify(value);

/**
 * One channel enum over `row`'s keys. `ownersOf` gives the tools whose query
 * a call under a key carries.
 */
function channelEnum(
  row: Readonly<Record<string, ChannelDefinition>>,
  name: string,
  doc: string,
  ownersOf: (definition: ChannelDefinition) => readonly string[]
): string {
  const keys = Object.keys(row);
  const definitionOf = (key: string): ChannelDefinition => row[key] ?? 'linear';
  const kind = (key: string): string => {
    const definition = definitionOf(key);
    return pascal(typeof definition === 'object' ? definition.kind : definition);
  };
  const arms = (body: (key: string) => string): string =>
    keys.map(key => `            Self::${variant(key)} => ${body(key)},`).join('\n');
  return `${doc}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ${name} {
${keys.map(key => `    ${variant(key)},`).join('\n')}
}
impl ${name} {
    pub const ALL: &'static [Self] = &[${keys.map(key => `Self::${variant(key)}`).join(', ')}];
    pub const fn key(self) -> &'static str {
        match self {
${arms(key => str(key))}
        }
    }
    pub const fn kind(self) -> ChannelKind {
        match self {
${arms(key => `ChannelKind::${kind(key)}`)}
        }
    }
    /// The tools whose query a call under this key carries.
    pub const fn owners(self) -> &'static [&'static str] {
        match self {
${arms(key => `&[${ownersOf(definitionOf(key)).map(str).join(', ')}]`)}
        }
    }
}
impl Channel for ${name} {
    fn key(self) -> &'static str {
        ${name}::key(self)
    }
    fn kind(self) -> ChannelKind {
        ${name}::kind(self)
    }
    fn owners(self) -> &'static [&'static str] {
        ${name}::owners(self)
    }
}`;
}

function collectionEnum(tool: string, collections: readonly string[]): string {
  const name = `${pascal(tool)}Collection`;
  return `/// The paged lists inside a \`${tool}\` row, each with its own pagination.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ${name} {
${collections.map(key => `    ${variant(key)},`).join('\n')}
}
impl ${name} {
    pub const ALL: &'static [Self] = &[${collections.map(key => `Self::${variant(key)}`).join(', ')}];
    pub const fn key(self) -> &'static str {
        match self {
${collections.map(key => `            Self::${variant(key)} => ${str(key)},`).join('\n')}
        }
    }
}`;
}

const CHANNEL_KIND = `/// How a continuation moves: \`linear\` pages the row's one walk, \`branch\` leads into a sub-value, \`numbered\` is a zero-based branch family.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ChannelKind {
    Linear,
    Branch,
    Numbered,
}`;

const CHANNEL_TRAIT = `/// A \`next\` key, whatever tool's row it sits on: every generated channel enum implements it.
pub trait Channel: Copy + 'static {
    fn key(self) -> &'static str;
    fn kind(self) -> ChannelKind;
    /// The tools whose query a call under this key carries.
    fn owners(self) -> &'static [&'static str];
}`;

/** Each query branch type mapped to its one owning tool; throws on a registry that names unknown or missing tools. */
function branchOwnersOf(
  tools: readonly string[],
  channels: ChannelRegistry,
  branches: readonly QueryBranch[]
): Map<string, string> {
  const known = new Set(tools);
  const missing = tools.filter(tool => channels[tool] === undefined);
  if (missing.length > 0) throw new Error(`continuationChannels has no entry for ${missing.join(', ')}`);
  const stray = Object.keys(channels).filter(tool => !known.has(tool));
  if (stray.length > 0) throw new Error(`continuationChannels names unknown tools: ${stray.join(', ')}`);
  const owners = new Map<string, string>();
  for (const { tool, type } of branches) {
    if (!known.has(tool)) throw new Error(`query branch ${type} names unknown tool ${tool}`);
    const owner = owners.get(type);
    if (owner !== undefined && owner !== tool) throw new Error(`query branch ${type} is shared by ${owner} and ${tool}`);
    owners.set(type, tool);
  }
  return owners;
}

/** A `From<type> for ToolQuery` impl whose body builds the variant from `value`. */
const toolQueryFrom = (type: string, body: string): string => `impl ::std::convert::From<${type}> for ToolQuery {
    fn from(value: ${type}) -> Self {
        ${body}
    }
}`;

/** The `ToolQuery` enum with a `From` for every tool query and query branch. */
function renderToolQuery(tools: readonly string[], branchOwners: ReadonlyMap<string, string>): string {
  const queries = tools.map(tool => ({ tool, type: queryTypeName(tool), variant: pascal(tool) }));
  return `/// A call's typed query: one variant per tool, wrapping that tool's query type.
#[derive(Clone, Debug, PartialEq)]
pub enum ToolQuery {
${queries.map(({ type, variant: name }) => `    ${name}(${type}),`).join('\n')}
}
impl ToolQuery {
    pub const fn tool_name(&self) -> &'static str {
        match self {
${queries.map(({ tool, variant: name }) => `            Self::${name}(_) => ${str(tool)},`).join('\n')}
        }
    }
    pub fn to_value(&self) -> ::serde_json::Result<::serde_json::Value> {
        match self {
${queries.map(({ variant: name }) => `            Self::${name}(query) => ::serde_json::to_value(query),`).join('\n')}
        }
    }
}
${queries.map(({ type, variant: name }) => toolQueryFrom(type, `Self::${name}(value)`)).join('\n')}${[...branchOwners]
    .map(([type, tool]) => `\n${toolQueryFrom(type, `Self::${pascal(tool)}(${queryTypeName(tool)}::from(value))`)}`)
    .join('')}`;
}

type DeclaredChannels = { sections: string[]; declared: Array<{ name: string; keys: string[] }> };

/** One channel enum per tool row (and its collections), plus the error channel every tool owns. */
function renderChannels(
  tools: readonly string[],
  channels: ChannelRegistry,
  errorChannels: Readonly<Record<string, ChannelDefinition>>
): DeclaredChannels {
  const out: DeclaredChannels = { sections: [], declared: [] };
  for (const tool of tools) {
    const entry = channels[tool];
    if (!entry) continue;
    if (Object.keys(entry.row).length > 0) {
      const name = `${pascal(tool)}Channel`;
      out.declared.push({ name, keys: Object.keys(entry.row) });
      out.sections.push(
        channelEnum(
          entry.row,
          name,
          `/// The \`next\` keys a \`${tool}\` row may carry; a numbered key takes a zero-based suffix.`,
          definition => (typeof definition === 'object' ? definition.owner : [tool])
        )
      );
    }
    if (entry.collections.length > 0) out.sections.push(collectionEnum(tool, entry.collections));
  }
  if (Object.keys(errorChannels).length > 0) {
    out.declared.push({ name: 'ErrorChannel', keys: Object.keys(errorChannels) });
    // A recovery call retries the row's own tool, which no static type names:
    // every tool owns an error channel.
    out.sections.push(
      channelEnum(
        errorChannels,
        'ErrorChannel',
        '/// The `next` keys an error row may carry, on any tool.',
        definition => (typeof definition === 'object' ? definition.owner : tools)
      )
    );
  }
  return out;
}

/** `ChannelSpec` and `ALL_CHANNELS`: every declared key, for registry-wide checks. */
function renderChannelSpecs(declared: DeclaredChannels['declared']): string {
  const specs = declared.flatMap(({ name, keys }) =>
    keys.map(
      key =>
        `    ChannelSpec { key: ${name}::${variant(key)}.key(), kind: ${name}::${variant(key)}.kind(), owners: ${name}::${variant(key)}.owners() },`
    )
  );
  return `/// One declared \`next\` key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChannelSpec {
    pub key: &'static str,
    pub kind: ChannelKind,
    pub owners: &'static [&'static str],
}
/// Every key of every generated channel enum, for registry-wide checks.
pub const ALL_CHANNELS: &[ChannelSpec] = &[
${specs.join('\n')}
];`;
}

/** The Rust appended after the typify body; pure, so it is testable without cargo. */
export function renderDispatch(
  tools: readonly string[],
  channels: ChannelRegistry,
  errorChannels: Readonly<Record<string, ChannelDefinition>> = {},
  branches: readonly QueryBranch[] = []
): string {
  const branchOwners = branchOwnersOf(tools, channels, branches);
  const { sections, declared } = renderChannels(tools, channels, errorChannels);
  return [CHANNEL_KIND, CHANNEL_TRAIT, renderToolQuery(tools, branchOwners), ...sections, renderChannelSpecs(declared)].join('\n');
}
