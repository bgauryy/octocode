import { classifyDiffLane } from '../diffLanes.js';
import type {
  OqlActiveTarget,
  OqlBackendCall,
  OqlTransformerTrace,
  QuerySource,
} from '../types.js';
import type { TransformerRegistryEntry } from './contract.js';

const ENTRIES: readonly TransformerRegistryEntry[] = [
  {
    id: 'github.code',
    target: 'code',
    sourceKinds: ['github'],
    status: 'active',
    backends: [
      { backend: 'ghSearchCode', operation: 'searchCode', exact: true },
    ],
    rowKind: 'code',
    adapterModule: 'transformers/github/code.ts',
    adapterFunctions: ['toGithubCodeSearchToolQuery'],
  },
  {
    id: 'github.files',
    target: 'files',
    sourceKinds: ['github'],
    status: 'active',
    backends: [
      { backend: 'ghSearchCode', operation: 'findFiles', exact: false },
    ],
    rowKind: 'file',
    adapterModule: 'adapters/github.ts',
    adapterFunctions: ['githubFiles'],
  },
  {
    id: 'github.content',
    target: 'content',
    sourceKinds: ['github'],
    status: 'active',
    backends: [
      { backend: 'ghGetFileContent', operation: 'getContent', exact: true },
    ],
    rowKind: 'content',
    adapterModule: 'adapters/github.ts',
    adapterFunctions: ['githubContent'],
  },
  {
    id: 'github.structure',
    target: 'structure',
    sourceKinds: ['github'],
    status: 'active',
    backends: [
      {
        backend: 'ghViewRepoStructure',
        operation: 'viewStructure',
        exact: true,
      },
    ],
    rowKind: 'tree',
    adapterModule: 'adapters/github.ts',
    adapterFunctions: ['githubStructure'],
  },
  {
    id: 'local.code.textRegex',
    target: 'code',
    sourceKinds: ['local', 'materialized'],
    status: 'active',
    backends: [
      { backend: 'localSearchCode', operation: 'searchCode', exact: true },
    ],
    rowKind: 'code',
    adapterModule: 'adapters/local.ts',
    adapterFunctions: ['executeCode'],
  },
  {
    id: 'local.code.structural',
    target: 'code',
    sourceKinds: ['local', 'materialized'],
    status: 'active',
    backends: [
      { backend: 'localSearchCode', operation: 'searchCode', exact: true },
    ],
    rowKind: 'code',
    adapterModule: 'adapters/local.ts',
    adapterFunctions: ['executeCode'],
  },
  {
    id: 'local.files',
    target: 'files',
    sourceKinds: ['local', 'materialized'],
    status: 'active',
    backends: [
      { backend: 'localFindFiles', operation: 'findFiles', exact: true },
    ],
    rowKind: 'file',
    adapterModule: 'adapters/local.ts',
    adapterFunctions: ['executeFiles'],
  },
  {
    id: 'local.content',
    target: 'content',
    sourceKinds: ['local', 'materialized'],
    status: 'active',
    backends: [
      { backend: 'localGetFileContent', operation: 'getContent', exact: true },
    ],
    rowKind: 'content',
    adapterModule: 'adapters/local.ts',
    adapterFunctions: ['executeContent'],
  },
  {
    id: 'local.structure',
    target: 'structure',
    sourceKinds: ['local', 'materialized'],
    status: 'active',
    backends: [
      {
        backend: 'localViewStructure',
        operation: 'viewStructure',
        exact: true,
      },
    ],
    rowKind: 'tree',
    adapterModule: 'adapters/local.ts',
    adapterFunctions: ['executeStructure'],
  },
  {
    id: 'local.semantics',
    target: 'semantics',
    sourceKinds: ['local', 'materialized'],
    status: 'active',
    backends: [
      { backend: 'lspGetSemantics', operation: 'getSemantics', exact: true },
    ],
    rowKind: 'record',
    recordType: 'semantics',
    adapterModule: 'adapters/researchTargets.ts',
    adapterFunctions: ['executeSemantics'],
  },
  {
    id: 'github.semantics',
    target: 'semantics',
    sourceKinds: ['github'],
    status: 'active',
    backends: [
      { backend: 'ghCloneRepo', operation: 'materialize', exact: true },
      { backend: 'lspGetSemantics', operation: 'getSemantics', exact: true },
    ],
    rowKind: 'record',
    recordType: 'semantics',
    adapterModule: 'adapters/researchTargets.ts',
    adapterFunctions: ['executeSemantics'],
  },
  {
    id: 'github.repositories',
    target: 'repositories',
    sourceKinds: ['github'],
    status: 'active',
    backends: [
      { backend: 'ghSearchRepos', operation: 'searchRepos', exact: true },
    ],
    rowKind: 'record',
    recordType: 'repository',
    adapterModule: 'adapters/researchTargets.ts',
    adapterFunctions: ['executeRepositories'],
  },
  {
    id: 'npm.packages',
    target: 'packages',
    sourceKinds: ['npm'],
    status: 'active',
    backends: [
      { backend: 'npmSearch', operation: 'searchPackages', exact: true },
    ],
    rowKind: 'record',
    recordType: 'package',
    adapterModule: 'adapters/researchTargets.ts',
    adapterFunctions: ['executePackages'],
  },
  {
    id: 'github.pullRequests',
    target: 'pullRequests',
    sourceKinds: ['github'],
    status: 'active',
    backends: [
      {
        backend: 'ghHistoryResearch',
        operation: 'searchPullRequests',
        exact: true,
      },
    ],
    rowKind: 'record',
    recordType: 'pullRequest',
    adapterModule: 'adapters/researchTargets.ts',
    adapterFunctions: ['executeHistory'],
  },
  {
    id: 'github.commits',
    target: 'commits',
    sourceKinds: ['github'],
    status: 'active',
    backends: [
      { backend: 'ghHistoryResearch', operation: 'searchCommits', exact: true },
    ],
    rowKind: 'record',
    recordType: 'commit',
    adapterModule: 'adapters/researchTargets.ts',
    adapterFunctions: ['executeHistory'],
  },
  {
    id: 'github.diff.prPatch',
    target: 'diff',
    sourceKinds: ['github'],
    status: 'active',
    backends: [
      { backend: 'ghHistoryResearch', operation: 'diff', exact: true },
    ],
    rowKind: 'record',
    recordType: 'diff',
    adapterModule: 'adapters/researchTargets.ts',
    adapterFunctions: ['executeDiff'],
  },
  {
    id: 'github.diff.directFile',
    target: 'diff',
    sourceKinds: ['github'],
    status: 'active',
    backends: [{ backend: 'ghGetFileContent', operation: 'diff', exact: true }],
    rowKind: 'record',
    recordType: 'diff',
    adapterModule: 'adapters/researchTargets.ts',
    adapterFunctions: ['executeDiff'],
  },
  {
    id: 'local.diff.directFile',
    target: 'diff',
    sourceKinds: ['local', 'materialized'],
    status: 'active',
    backends: [
      { backend: 'localGetFileContent', operation: 'diff', exact: true },
    ],
    rowKind: 'record',
    recordType: 'diff',
    adapterModule: 'adapters/researchTargets.ts',
    adapterFunctions: ['executeDiff'],
  },
  {
    id: 'local.research',
    target: 'research',
    sourceKinds: ['local', 'materialized'],
    status: 'active',
    backends: [
      {
        backend: 'smartOqlResearch',
        operation: 'runResearchFlow',
        exact: false,
      },
    ],
    rowKind: 'record',
    recordType: 'research',
    adapterModule: 'adapters/researchTargets.ts',
    adapterFunctions: ['executeResearch'],
  },
  {
    id: 'local.graph',
    target: 'graph',
    sourceKinds: ['local', 'materialized'],
    status: 'active',
    backends: [
      {
        backend: 'smartOqlGraph',
        operation: 'queryRelationshipGraph',
        exact: false,
      },
    ],
    rowKind: 'record',
    recordType: 'graph',
    adapterModule: 'adapters/researchTargets.ts',
    adapterFunctions: ['executeGraph'],
  },
  {
    id: 'github.materialize',
    target: 'materialize',
    sourceKinds: ['github'],
    status: 'active',
    backends: [
      { backend: 'ghCloneRepo', operation: 'materialize', exact: true },
    ],
    rowKind: 'record',
    recordType: 'materialized',
    adapterModule: 'adapters/materialize.ts',
    adapterFunctions: ['executeMaterializeCheckpoint'],
  },
] as const;

export function listTransformerEntries(): readonly TransformerRegistryEntry[] {
  return ENTRIES;
}

export function findTransformerEntry(args: {
  sourceKind: QuerySource['kind'];
  target: OqlActiveTarget;
  variant?: string;
}): TransformerRegistryEntry | undefined {
  if (args.target === 'diff' && args.variant) {
    const sourcePrefix =
      args.sourceKind === 'materialized' ? 'local' : args.sourceKind;
    return ENTRIES.find(
      entry => entry.id === `${sourcePrefix}.diff.${args.variant}`
    );
  }
  return ENTRIES.find(
    entry =>
      entry.target === args.target &&
      entry.sourceKinds.includes(args.sourceKind)
  );
}

export function findTransformerById(
  id: string
): TransformerRegistryEntry | undefined {
  return ENTRIES.find(entry => entry.id === id);
}

export function findTransformerForQuery(args: {
  source?: QuerySource;
  target: OqlActiveTarget;
  params?: Record<string, unknown>;
}): TransformerRegistryEntry | undefined {
  const sourceKind = sourceKindForTarget(args.source, args.target);
  if (args.target === 'diff') {
    const lane = classifyDiffLane(args.params);
    const variant =
      lane.kind === 'prPatch'
        ? 'prPatch'
        : lane.kind === 'directFile'
          ? 'directFile'
          : undefined;
    if (variant)
      return findTransformerEntry({ sourceKind, target: 'diff', variant });
  }
  return findTransformerEntry({ sourceKind, target: args.target });
}

export function backendCallsForTransformer(
  entry: TransformerRegistryEntry,
  source?: QuerySource
): OqlBackendCall[] {
  return entry.backends.map(backend => ({
    backend: backend.backend,
    operation: backend.operation,
    exact: backend.exact,
    source,
  }));
}

export function transformerTrace(
  entry: TransformerRegistryEntry
): OqlTransformerTrace {
  return {
    id: entry.id,
    status: entry.status,
    sourceKinds: entry.sourceKinds,
    target: entry.target,
    backends: entry.backends,
  };
}

function sourceKindForTarget(
  source: QuerySource | undefined,
  target: OqlActiveTarget
): QuerySource['kind'] {
  if (source?.kind) return source.kind;
  if (target === 'packages') return 'npm';
  if (target === 'repositories') return 'github';
  return 'github';
}
