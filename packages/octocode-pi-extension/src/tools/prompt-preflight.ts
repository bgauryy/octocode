import { contentDigest } from '../runtime/continuity-contracts.js';
import { type PiRuntimeObservation } from '../runtime/physiology.js';
import type { PromptMode } from '../contracts/protocols.js';
import type { BeforeAgentStartEvent, NotifyFn, PiContext, PiInstance } from '../types.js';
import type { SessionScopedState } from '../session-scoped-state.js';
import { ensureAdaptiveThinkingCompatibility } from '../model-compat.js';
import { getAssetPaths, readTextIfExists } from '../assets.js';
import {
  adaptPiResearchGuidance,
  composeSystemPrompt,
  renderSystemPromptAddendum,
  stripPiSkillsSection,
  stripProjectContext,
} from '../prompt.js';
import { projectPiSystemPromptCapabilities } from '../prompts/system-prompt.js';
import { collectPiRetainedContentDigests } from '../adapters/pi-retained-context.js';
import { refreshCapabilityAdapters } from '../adapters/pi-capability-adapters.js';
import { createPiPhysiologyAdvisory } from '../adapters/pi-physiology-regulation.js';
import { getDirectToolContractStats } from './octocode-tools.js';
import { assembleSessionPromptContext } from './session-prompt-context.js';
import { preparePromptCapabilities, renderAgentsProtocolInstructions } from './prompt-capabilities.js';
import { renderRuntimeCapabilitiesAddendum } from './image-render.js';
import { getCachedMcpCatalogAddendum, getCachedMcpCounts } from './mcp-tool.js';
import { getDynamicCapabilitiesAddendum } from './dynamic-catalog.js';
import { renderAvailableSkillsAddendum } from './skill-catalog.js';
import { activePlanScope, bumpPlanTurn } from './planning/plan-store.js';
import { getCurrentPlanReadModel, renderPlanContext } from './plan-read-model.js';
import { readSessionMemoryForContext } from './session-memory-runtime.js';
import { projectSessionMemoryUpdate, SESSION_MEMORY_MAX_BYTES } from './session-memory.js';
import { readSessionUserRequestContext, USER_REQUEST_CONTEXT_MAX_CHARS } from './user-request-context.js';
import {
  INITIAL_CONTEXT_TOKEN_BUDGET,
  assembleContextSegments,
  assertContextTokenBudget,
  estimateContextTokens,
  providerContextTokenBudget,
} from './context-segments.js';
import {
  mergeCurrentContextSources,
  registerCurrentContextSource,
} from './context-source-registry.js';
import type { CurrentRehydrationSource } from './context-source-contracts.js';
import {
  consumeValidatedRehydration,
  hasPendingRehydration,
  REHYDRATION_RECEIPT_ENTRY_TYPE,
} from './rehydration-orchestrator.js';
import { writeDiscoveryFile } from './discovery-file.js';
import { runtimeStoreFor } from './runtime-renderer.js';
import { isSubagentProcess } from './agents/registry.js';
import type { DiscoveredSkill } from './skill-discovery.js';

export interface PromptPreflightOptions {
  pi: PiInstance;
  promptMode: PromptMode;
  notify: NotifyFn;
  getSession(): SessionScopedState;
  getFallbackTools(): string[];
  readPhysiology(ctx: PiContext): PiRuntimeObservation | undefined;
}

/**
 * Owns the two-stage Pi prompt boundary without owning session lifetime.
 * `before_agent_start` prepares the complete provider context; `agent_start`
 * cancels the run when that preparation did not complete successfully.
 */
export function createPromptPreflightController(options: PromptPreflightOptions) {
  const { pi, promptMode, notify } = options;
  const pendingPreparations = new WeakMap<object, Set<string>>();
  const physiologyAdvisory = createPiPhysiologyAdvisory();
  let warnedContextDrift = false;
  let warnedSkillsDrift = false;

  const registerSelectedSkillContext = (ctx: PiContext, skill: DiscoveredSkill): void => {
    const name = skill.name.trim().toLowerCase();
    registerCurrentContextSource(ctx, {
      version: 1,
      id: `selected-skill:${name}`,
      kind: 'skill',
      origin: `skill-file:${name}`,
      authority: 'project',
      scope: 'task',
      visibility: 'inspectable',
      rehydrate: 'on-trigger',
      capture: false,
      tokenBudget: 30_000,
      readCurrent: () => readTextIfExists(skill.path),
    });
  };

  const guardAgentStart = async (_event: unknown, ctx: PiContext | undefined): Promise<void> => {
    // before_agent_start has no active abort signal and Pi catches its errors.
    // agent_start has a live run signal, before any provider request is sent.
    if (pendingPreparations.get(options.getSession())?.has(activePlanScope(ctx))) ctx?.abort?.();
  };

  const prepare = async (event: BeforeAgentStartEvent, ctx: PiContext | undefined) => {
    const session = options.getSession();
    const promptScope = activePlanScope(ctx);
    const pendingPromptScopes = pendingPreparations.get(session) ?? new Set<string>();
    pendingPreparations.set(session, pendingPromptScopes);
    pendingPromptScopes.add(promptScope);
    const worker = isSubagentProcess();

    // Declarative model/hook files are turn-scoped capabilities and must be
    // refreshed before the effective tool/skill snapshot is projected.
    refreshCapabilityAdapters(ctx);
    ensureAdaptiveThinkingCompatibility(ctx?.model);

    const noContext = Boolean(pi.getFlag?.('no-context'));
    let piPrompt = event.systemPrompt;
    if (session.managedPromptAddendum) piPrompt = piPrompt.replace(session.managedPromptAddendum, '').trim();
    if (noContext) {
      piPrompt = stripProjectContext(piPrompt);
      if (!warnedContextDrift && piPrompt.includes('<project_context>')) {
        warnedContextDrift = true;
        console.warn('[octocode-pi-extension] --no-context set but <project_context> remains after strip — Pi prompt format may have changed; update stripProjectContext.');
      }
    }

    const activeTools = await preparePromptCapabilities({
      pi,
      ctx,
      session,
      worker,
      piSkills: event.systemPromptOptions?.skills,
      fallbackTools: options.getFallbackTools(),
      notify,
    });
    const hasCapability = (name: string): boolean => activeTools.has(name);
    if (hasCapability('MCPTool')) piPrompt = adaptPiResearchGuidance(piPrompt);
    piPrompt = stripPiSkillsSection(piPrompt);
    if (!warnedSkillsDrift && piPrompt.includes('The following skills provide specialized instructions')) {
      warnedSkillsDrift = true;
      console.warn('[octocode-pi-extension] Pi skill guidance remains after host adaptation; check the supported Pi prompt format.');
    }

    const collectPromptContext = (policy: string) => assembleSessionPromptContext({
      'agents-protocol': renderAgentsProtocolInstructions(ctx, event.systemPromptOptions?.contextFiles, worker || noContext, {
        get: () => session.agentsProtocolCache,
        set: (entry) => { session.agentsProtocolCache = entry; },
      }),
      'octocode-product-policy': projectPiSystemPromptCapabilities(policy, {
        mcpTool: hasCapability('MCPTool'),
        skill: hasCapability('skill'),
      }),
      'mcp-tool-contracts': hasCapability('MCPTool') ? getCachedMcpCatalogAddendum(ctx) : '',
      // capability_revision is turn context, not frozen prompt content, so
      // capability changes do not invalidate the provider's prefix cache.
      'runtime-tool-contracts': renderRuntimeCapabilitiesAddendum(ctx),
      'dynamic-tool-contracts': worker ? '' : getDynamicCapabilitiesAddendum(
        session.latestAvailableSkills?.map(skill => skill.name),
        { tools: hasCapability('callTool'), skills: hasCapability('skill') },
      ),
      'available-skills': hasCapability('skill') ? renderAvailableSkillsAddendum(session.latestAvailableSkills) : '',
      'session-artifact-contract': session.sessionArtifactPathsContext,
    });

    const planScope = activePlanScope(ctx);
    if (!worker) bumpPlanTurn(planScope);
    const planContext = worker ? '' : renderPlanContext(getCurrentPlanReadModel(ctx, planScope));
    const currentSessionMemory = session.sessionArtifactContext
      ? readSessionMemoryForContext(ctx, session.sessionArtifactContext) ?? ''
      : '';
    const currentCapabilityRevision = session.capabilityRevision
      ? `<capability_revision>${session.capabilityRevision}</capability_revision>`
      : '';
    const recoveryPending = Boolean(ctx && hasPendingRehydration(ctx));
    const currentUserRequests = ctx && recoveryPending ? readSessionUserRequestContext(ctx) ?? '' : '';
    const retainedDigests = ctx && recoveryPending
      ? collectPiRetainedContentDigests(ctx, {
          knownSegmentContents: {
            'active-plan': planContext,
            'capability-revision': currentCapabilityRevision,
            'session-memory': currentSessionMemory,
            'user-request-history': currentUserRequests,
          },
        })
      : new Set<string>();
    const planSig = planContext;
    const planChanged = planSig !== session.deliveredPlanSignature;
    const planAlreadyRetained = recoveryPending && retainedDigests.has(contentDigest(planContext));
    const planNeedsRecovery = recoveryPending && planContext.length > 0 && !planAlreadyRetained;
    const planDeliveryContent = !planAlreadyRetained && (planChanged || planNeedsRecovery)
      ? planContext || (session.deliveredPlanSignature === undefined ? '' : 'Plan cleared; no active task breakdown remains.')
      : '';
    const livePlanContents: Record<string, string> = { 'active-plan': planContext };
    const livePlanAssembly = assembleContextSegments([
      {
        id: 'active-plan', content: planContext, kind: 'plan', origin: 'plan-domain', authority: 'user',
        scope: 'task', visibility: 'transcript', rehydrate: 'always', tokenBudget: 15_000,
      },
    ]);
    const sessionMemoryUpdate = projectSessionMemoryUpdate(
      currentSessionMemory,
      session.deliveredSessionMemorySignature,
    );
    const sessionMemoryContent = sessionMemoryUpdate.content;
    const userRequestContent = currentUserRequests && !retainedDigests.has(contentDigest(currentUserRequests))
      ? currentUserRequests
      : '';

    const currentSourcesFrom = (
      manifest: ReturnType<typeof assembleContextSegments>['manifest'],
      contents: Record<string, string>,
    ): CurrentRehydrationSource[] => manifest.map(segment => ({ segment, content: contents[segment.id] ?? '' }));
    let frozenRehydration: ReturnType<typeof consumeValidatedRehydration>;
    let recoveryPromptAssembly: ReturnType<typeof collectPromptContext> | undefined;
    if (ctx && recoveryPending) {
      const currentAssembly = collectPromptContext(session.cachedSystemPromptText ?? '');
      // When the policy cache is initialized, the exact same immutable inputs
      // feed the final assembly later in this preflight. Reuse it instead of
      // rebuilding and rehashing every segment twice during recovery.
      if (session.cachedSystemPromptText !== null) recoveryPromptAssembly = currentAssembly;
      const currentContents = currentAssembly.contents;
      for (const content of [planDeliveryContent, sessionMemoryContent, userRequestContent].filter(Boolean)) {
        retainedDigests.add(contentDigest(content));
      }
      frozenRehydration = consumeValidatedRehydration(
        ctx,
        mergeCurrentContextSources(ctx, [
          ...currentSourcesFrom(currentAssembly.manifest, currentContents),
          ...currentSourcesFrom(livePlanAssembly.manifest, livePlanContents),
        ], { totalTokenBudget: INITIAL_CONTEXT_TOKEN_BUDGET }),
        {
          allowProjection: true,
          deferConsumption: true,
          retainedContentDigests: retainedDigests,
        },
      );
    }

    const capabilityChanged = session.capabilityRevision !== session.deliveredCapabilityRevision;
    const capabilityAlreadyRetained = recoveryPending
      && currentCapabilityRevision.length > 0
      && retainedDigests.has(contentDigest(currentCapabilityRevision));
    const capabilityRevisionContent = currentCapabilityRevision
      && !capabilityAlreadyRetained
      && (capabilityChanged || recoveryPending)
      ? currentCapabilityRevision
      : '';
    const physiologyDelivery = physiologyAdvisory(ctx ? options.readPhysiology(ctx) : undefined);
    const contextAssembly = assembleContextSegments([
      { id: 'capability-revision', content: capabilityRevisionContent, kind: 'tool-result', origin: 'pi-runtime-state', authority: 'external-data', scope: 'session', visibility: 'inspectable', rehydrate: 'never', tokenBudget: 96 },
      { id: 'user-request-history', content: userRequestContent, kind: 'user-request', origin: 'session-user:history', authority: 'user', scope: 'task', visibility: 'transcript', rehydrate: 'always', tokenBudget: Math.ceil(USER_REQUEST_CONTEXT_MAX_CHARS / 4) },
      { id: 'runtime-physiology', content: physiologyDelivery.content, kind: 'tool-result', origin: 'pi-runtime-observation', authority: 'external-data', scope: 'turn', visibility: 'inspectable', rehydrate: 'never', tokenBudget: 128 },
      { id: 'active-plan', content: planDeliveryContent, kind: 'plan', origin: 'plan-domain', authority: 'user', scope: 'task', visibility: 'transcript', rehydrate: 'always', tokenBudget: 15_000 },
      { id: 'session-memory', content: sessionMemoryContent, kind: 'memory-lead', origin: 'session-memory', authority: 'external-data', scope: 'session', visibility: 'inspectable', rehydrate: 'always', tokenBudget: Math.ceil(SESSION_MEMORY_MAX_BYTES / 4) },
    ]);
    const contextMessage = contextAssembly.manifest.length > 0 || frozenRehydration?.content
      ? {
          customType: 'octocode-context-update',
          content: [contextAssembly.content, frozenRehydration?.content].filter(Boolean).join('\n\n'),
          display: false,
          details: {
            version: 1,
            estimates: contextAssembly.estimates,
            segments: [...contextAssembly.manifest, ...(frozenRehydration?.segments ?? [])],
            ...(frozenRehydration ? { rehydration: frozenRehydration.receipt } : {}),
          },
        }
      : undefined;
    if (contextMessage) {
      contextMessage.details.estimates = {
        ...contextAssembly.estimates,
        total: estimateContextTokens(contextMessage.content),
      };
      for (const kind of Object.keys(frozenRehydration?.tokensByKind ?? {}) as Array<keyof typeof contextAssembly.estimates.byKind>) {
        const byKind = contextMessage.details.estimates.byKind;
        byKind[kind] = (byKind[kind] ?? 0) + (frozenRehydration?.tokensByKind[kind] ?? 0);
      }
    }

    if (session.cachedSystemPromptText === null) {
      session.cachedSystemPromptText = readTextIfExists(getAssetPaths().systemPrompt);
    }
    const promptAssembly = recoveryPromptAssembly ?? collectPromptContext(session.cachedSystemPromptText);
    const initialContents = promptAssembly.contents;
    const mcpCatalog = initialContents['mcp-tool-contracts'];
    const runtimeCapabilities = initialContents['runtime-tool-contracts'];
    const dynamicCatalog = initialContents['dynamic-tool-contracts'];
    const availableSkills = initialContents['available-skills'];
    const prompt = promptAssembly.content;
    const resolvedPrompt = prompt.trim().length === 0
      ? piPrompt
      : composeSystemPrompt({ piSystemPrompt: piPrompt, octocodePrompt: prompt, promptMode });
    const mcpCounts = getCachedMcpCounts(ctx);
    const directToolStats = getDirectToolContractStats(activeTools);
    const turnContextChars = contextMessage?.content.length ?? 0;
    const dynamicChars = runtimeCapabilities.length + dynamicCatalog.length + availableSkills.length + turnContextChars;
    const providerSubtotalChars = resolvedPrompt.length + directToolStats.totalChars + turnContextChars;
    const estimatedProviderTokens = assertContextTokenBudget(
      'initial provider context',
      providerSubtotalChars,
      providerContextTokenBudget(ctx?.model?.contextWindow),
    );
    runtimeStoreFor(ctx)?.getState().setContext({
      status: 'ready',
      mode: 'routing',
      systemPromptChars: resolvedPrompt.length,
      mcpChars: mcpCatalog.length,
      dynamicChars,
      directToolChars: directToolStats.totalChars,
      providerSubtotalChars,
      estimatedTokens: estimatedProviderTokens,
      contextAssemblyEstimates: promptAssembly.estimates,
      mcpServers: mcpCounts.servers,
      mcpTools: mcpCounts.tools,
      skills: session.latestAvailableSkills?.length ?? 0,
    });
    void writeDiscoveryFile(ctx, {
      skills: (session.latestAvailableSkills ?? []).map(skill => ({ ...skill, enabled: true })),
      nativeTools: [...activeTools],
      overhead: {
        sysChars: piPrompt.length + (session.cachedSystemPromptText?.length ?? 0),
        mcpChars: mcpCatalog.length,
        dynamicChars,
        totalChars: resolvedPrompt.length + turnContextChars,
        contextAssemblyEstimates: promptAssembly.estimates,
        directToolChars: directToolStats.totalChars,
        mcpServers: mcpCounts.servers,
        mcpTools: mcpCounts.tools,
        skills: session.latestAvailableSkills?.length ?? 0,
        status: 'ready',
        mode: 'routing',
      },
    });
    session.managedPromptAddendum = renderSystemPromptAddendum(prompt);
    session.deliveredPlanSignature = planSig;
    session.deliveredSessionMemorySignature = sessionMemoryUpdate.signature;
    session.deliveredCapabilityRevision = session.capabilityRevision;
    if (frozenRehydration) {
      pi.appendEntry?.(REHYDRATION_RECEIPT_ENTRY_TYPE, frozenRehydration.receipt);
      frozenRehydration.commit();
    }
    physiologyDelivery.commit();
    pendingPromptScopes.delete(promptScope);
    if (resolvedPrompt === event.systemPrompt) {
      return contextMessage ? { message: contextMessage } : undefined;
    }
    return contextMessage
      ? { systemPrompt: resolvedPrompt, message: contextMessage }
      : { systemPrompt: resolvedPrompt };
  };

  return { guardAgentStart, prepare, registerSelectedSkillContext };
}
