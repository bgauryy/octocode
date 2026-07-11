/**
 * `target:"research"` adapter: runs the smart-research flow analysis
 * (analyzeResearchFlow) over a local/materialized root, packetizes it, and
 * shapes the packet/detailed-domain windows into a single research record.
 */
import { diagnostic } from '../../diagnostics.js';
import { analyzeResearchFlow } from '../../research/analyze.js';
import { buildResearchPackets } from '../../research/packets.js';
import { nativeGraphSummary, packetPage } from '../graphView.js';
import type { AdapterResult } from '../local.js';
import {
  buildDetailedDomains,
  combinePagination,
  requestedResearchMode,
} from './pagination.js';
import { records } from './rows.js';
import { params } from './shared.js';
import type { OqlQuery } from '../../types.js';

export async function executeResearch(query: OqlQuery): Promise<AdapterResult> {
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
          'target:"research" needs a complete local file universe. Use a local/materialized source, or materialize a bounded GitHub corpus first.',
          {
            backend: 'smartOqlResearch',
            repair: {
              message:
                'Run target:"materialize" for a bounded GitHub repo/subtree, then run target:"research" against the returned localPath.',
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
          'target:"research" mode:"prove" requires params.intent so the proof lane is deterministic. Use intent:"reachability"|"dependencies"|"symbols"|"general", then follow packet next.semantic/next.fetch continuations for missing proof.',
          {
            backend: 'smartOqlResearch',
            queryPath: 'params.intent',
            repair: {
              message:
                'Add params.intent. Example: params:{ mode:"prove", intent:"reachability", facets:["symbols","files","relations"] }.',
            },
          }
        ),
      ],
      provenance: [{ backend: 'smartOqlResearch', source: query.from }],
    };
  }

  let data: Awaited<ReturnType<typeof analyzeResearchFlow>>;
  try {
    data = await analyzeResearchFlow({
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
            : 'Could not analyze the requested research root.',
          { backend: 'smartOqlResearch' }
        ),
      ],
      provenance: [{ backend: 'smartOqlResearch', source: query.from }],
    };
  }

  // Plan mode returns the flow only (no scan), so there is nothing to packetize.
  const { packets, graphSummary } =
    data.mode === 'plan'
      ? { packets: [], graphSummary: undefined }
      : buildResearchPackets(data);

  const caveats = [...data.caveats];
  if (p.mode === 'prove') {
    caveats.push(
      'mode:"prove" requested on target:"research": packets are candidate-grade unless LSP proof is attached. Native AST facts are included where available, but LSP reference proof is not run here. Use target:"graph" with proof:"lsp" or follow each packet\'s next.semantic.'
    );
  }
  const pageWindow = graphSummary
    ? packetPage(query, packets.length)
    : undefined;
  const pagedPackets = pageWindow
    ? packets.slice(pageWindow.packetsStart, pageWindow.packetsEnd)
    : [];
  if (
    pageWindow &&
    packets.length > 0 &&
    pageWindow.packetsStart >= packets.length
  ) {
    caveats.push(
      `Packet page ${pageWindow.pagination.currentPage} is outside the available packet range (${pageWindow.pagination.totalPages} page(s)).`
    );
  }

  // P1: detailed view returns per-domain *windows* (sliced + paged), not whole
  // arrays — honoring `select` so a narrow projection drops unrequested domains.
  const detailed =
    query.view === 'detailed'
      ? buildDetailedDomains(query, data)
      : { fields: {} as Record<string, unknown> };

  const enriched: Record<string, unknown> = {
    kind: data.kind,
    goal: data.goal,
    intent: data.intent,
    facets: data.facets,
    mode: data.mode,
    root: data.root,
    flow: data.flow,
    summary: data.summary,
    graphCapabilities: data.graphCapabilities,
    nativeGraphSummary: nativeGraphSummary(data.graphFacts),
    caveats,
    ...(graphSummary
      ? {
          graphSummary,
          packetPage: pageWindow?.pagination,
          packets: pagedPackets,
        }
      : {}),
    ...detailed.fields,
  };

  // The envelope pagination drives `next.page`; for detailed view it must
  // advance the packet window AND every detailed domain together.
  const pagination = combinePagination(
    pageWindow?.pagination,
    detailed.pagination
  );

  return {
    results: records([enriched], 'research', query.from),
    ...(pagination ? { pagination } : {}),
    diagnostics: [],
    provenance: [{ backend: 'smartOqlResearch', source: query.from }],
  };
}
