/**
 * `target:"graph"` adapter: runs the smart-research flow analysis, builds
 * evidence packets, optionally escalates them with LSP proof, and shapes the
 * result into the graph view (nodes/edges/facts) for a single graph record.
 */
import { diagnostic } from '../../diagnostics.js';
import { analyzeResearchFlow } from '../../research/analyze.js';
import { buildResearchPackets } from '../../research/packets.js';
import {
  buildGraphView,
  graphFilters,
  nativeGraphSummary,
  summarizePacketGraph,
} from '../graphView.js';
import {
  escalateGraphPacketsWithLsp,
  graphProofLimit,
  shouldRunLspProof,
} from '../graphProof.js';
import type { AdapterResult } from '../local.js';
import { requestedResearchMode } from './pagination.js';
import { records } from './rows.js';
import { params } from './shared.js';
import type { OqlGraphData, OqlQuery } from '../../types.js';

export async function executeGraph(query: OqlQuery): Promise<AdapterResult> {
  const p = params(query);
  const root =
    query.from?.kind === 'local'
      ? query.from.path
      : query.from?.kind === 'materialized'
        ? query.from.localPath
        : undefined;

  if (!root) {
    return {
      results: [],
      diagnostics: [
        diagnostic(
          'requiresMaterialization',
          'target:"graph" needs a complete local file universe. Use a local/materialized source, or materialize a bounded GitHub corpus first.',
          {
            backend: 'smartOqlGraph',
            repair: {
              message:
                'Run target:"materialize" for a bounded GitHub repo/subtree, then run target:"graph" against the returned localPath.',
            },
          }
        ),
      ],
      provenance: [],
    };
  }

  const facets = Array.isArray(p.facets)
    ? p.facets.filter((facet): facet is string => typeof facet === 'string')
    : undefined;
  const mode = requestedResearchMode(p.mode);
  if (mode === 'prove' && typeof p.intent !== 'string') {
    return {
      results: [],
      diagnostics: [
        diagnostic(
          'invalidQuery',
          'target:"graph" mode:"prove" requires params.intent so the proof lane is deterministic. Use intent:"reachability"|"dependencies"|"symbols"|"general", then follow graph packet next.semantic/next.fetch continuations for missing proof.',
          {
            backend: 'smartOqlGraph',
            queryPath: 'params.intent',
            repair: {
              message:
                'Add params.intent. Example: params:{ mode:"prove", intent:"reachability", direction:"incoming" }.',
            },
          }
        ),
      ],
      provenance: [{ backend: 'smartOqlGraph', source: query.from }],
    };
  }

  let analysis: Awaited<ReturnType<typeof analyzeResearchFlow>>;
  try {
    analysis = await analyzeResearchFlow({
      root,
      goal: typeof p.goal === 'string' ? p.goal : undefined,
      intent: typeof p.intent === 'string' ? p.intent : undefined,
      facets,
      mode,
      maxFiles: typeof p.maxFiles === 'number' ? p.maxFiles : undefined,
    });
  } catch (err) {
    return {
      results: [],
      diagnostics: [
        diagnostic(
          'invalidQuery',
          err instanceof Error
            ? err.message
            : 'Could not analyze the requested graph root.',
          { backend: 'smartOqlGraph' }
        ),
      ],
      provenance: [{ backend: 'smartOqlGraph', source: query.from }],
    };
  }

  const bundle =
    analysis.mode === 'plan' ? undefined : buildResearchPackets(analysis);
  const filters = graphFilters(p);
  const packets = bundle?.packets ?? [];
  const proofDiagnostics = shouldRunLspProof(analysis.mode, p)
    ? await escalateGraphPacketsWithLsp(
        root,
        query,
        packets,
        filters,
        graphProofLimit(query, p)
      )
    : [];
  const graphSummary = summarizePacketGraph(packets);
  const view = buildGraphView(
    query,
    packets,
    graphSummary,
    filters,
    analysis.graphFacts,
    root
  );

  const caveats = [
    ...(view.data.caveats ?? []),
    ...analysis.caveats,
    ...(analysis.mode === 'plan'
      ? ['mode:"plan" requested: graph packets were not built.']
      : []),
    ...(p.mode === 'prove'
      ? [
          shouldRunLspProof(analysis.mode, p)
            ? 'mode:"prove" requested: LSP proof escalation ran for the current graph page only. Follow next.page and next.semantic for remaining/open proof.'
            : 'mode:"prove" requested: graph rows are candidate-grade only. Follow packet next.semantic to confirm references.',
        ]
      : []),
  ];

  const enriched: OqlGraphData = {
    ...view.data,
    goal: analysis.goal,
    intent: analysis.intent,
    facets: analysis.facets,
    mode: analysis.mode,
    root: analysis.root,
    flow: analysis.flow,
    graphCapabilities: analysis.graphCapabilities,
    nativeGraphSummary: nativeGraphSummary(analysis.graphFacts),
    caveats,
  };

  return {
    results: records([enriched], 'graph', query.from),
    pagination: view.pagination,
    diagnostics: proofDiagnostics,
    provenance: [{ backend: 'smartOqlGraph', source: query.from }],
  };
}
