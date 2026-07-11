import path from 'node:path';
import { insertEditLog } from './audit.js';
import { activeTaskClaimForAgent } from './tasks.js';
import { endWork, startWork, touchWork } from './work.js';
import { discardUncommittedHookFiles } from './work-hook.js';
import { notifyGet, sessionCapture } from './maintenance.js';
import { registerAgent } from './agents.js';
import { endSession } from './sessions.js';
import { activeWorkRunForFiles, finalizeActivePiFallbackRuns, guardPiHarnessEdit, isAggregatedPiFallbackRun, piPeerDelta, resolvePiTargetPath, startOrAttachPiFallbackRun, workRunOrigin } from './pi-hooks-guard.js';
import { artifactFrom, defaultGetDb, ensurePiSession, extractPiWriteTargetPaths, firstString, getPiAwarenessAgentId, getPiAwarenessSessionId, notify, PiAwarenessBridgeOptions, PiLikeContext, PiToolEvent } from './pi-hooks-inputs.js';

export function createPiAwarenessBridge(options: PiAwarenessBridgeOptions = {}) {
  const pendingToolFiles = options.pendingToolFiles ?? new Map<string, string[]>();
  const pendingToolRuns = options.pendingToolRuns ?? new Map<string, string>();
  const peerFingerprints = options.peerFingerprints ?? new Map<string, string>();
  const getDb = options.getDb ?? ((ctx?: PiLikeContext) => defaultGetDb(options, ctx));
  const skillRoot = options.skillRoot ?? process.env.OCTOCODE_SKILL_ROOT ?? null;
  const latestPromptBySession = new Map<string, string>();

  return {
    pendingToolFiles,
    pendingToolRuns,
    peerFingerprints,

    async handleInput(event: Record<string, unknown> = {}, ctx?: PiLikeContext) {
      const sessionId = getPiAwarenessSessionId(ctx);
      const prompt = firstString(event.text, event.prompt, event.user_prompt, event.userPrompt);
      if (prompt) latestPromptBySession.set(sessionId, prompt.slice(0, 4_000));
      else latestPromptBySession.delete(sessionId);
      return undefined;
    },

    async handleToolCall(event: PiToolEvent, ctx?: PiLikeContext) {
      const targetFiles = extractPiWriteTargetPaths(event?.toolName, event?.input);
      if (targetFiles.length === 0) return undefined;
      // A file-set fallback cannot distinguish two parallel edits of the same
      // file, and tool_execution_end does not carry the start payload needed to
      // reconstruct that key. Block before the write when the host supplies no
      // stable id instead of creating presence that cannot be correlated safely.
      const dedupeKey = firstString(event?.toolCallId);
      if (!dedupeKey) {
        const reason = 'Octocode awareness blocked this edit: the Pi host did not provide a stable toolCallId for lifecycle correlation.';
        notify(ctx, reason, 'warning');
        return { block: true, reason };
      }
      if (pendingToolRuns.has(dedupeKey)) return undefined;
      const harnessBlockReason = guardPiHarnessEdit(targetFiles, ctx, skillRoot);
      if (harnessBlockReason) return { block: true, reason: harnessBlockReason };

      const agentId = getPiAwarenessAgentId(ctx);
      try {
        const db = getDb(ctx);
        const activeClaim = activeTaskClaimForAgent(db, {
          agentId,
          workspacePath: ctx?.cwd ?? process.cwd(),
          artifact: artifactFrom(ctx, event as Record<string, unknown>),
        });
        const workspacePath = ctx?.cwd ?? process.cwd();
        const artifact = artifactFrom(ctx, event as Record<string, unknown>);
        ensurePiSession(db, {
          agentId,
          sessionId: getPiAwarenessSessionId(ctx),
          workspacePath,
          artifact,
        });
        const explicitRunId = activeClaim ? null : activeWorkRunForFiles(db, {
          agentId,
          workspacePath,
          artifact,
          targetFiles,
        });
        const piSessionId = getPiAwarenessSessionId(ctx);
        const result = explicitRunId
          ? { ok: true as const, ...touchWork(db, {
            agentId,
            runId: explicitRunId,
            targetFiles,
            ttlMs: 10 * 60_000,
          }) }
          : activeClaim
            ? startWork(db, {
              agentId,
              workspacePath,
              artifact,
              runId: activeClaim.run_id,
              targetFiles,
              origin: 'HOOK',
              source: 'HOOK',
              ttlMs: 10 * 60_000,
            })
            : startOrAttachPiFallbackRun(db, {
              agentId,
              sessionId: piSessionId,
              workspacePath,
              artifact,
              targetFiles,
            });

        if (!result.ok) {
          const detail = (result.conflicts || [])
            .map((conflict) => `${conflict.file_path} (held by ${conflict.agent_id})`)
            .join(', ');
          return { block: true, reason: `Octocode awareness blocked this edit: ${detail || 'conflict'}` };
        }

        pendingToolFiles.set(dedupeKey, targetFiles);
        pendingToolRuns.set(dedupeKey, result.run.run_id);
        const peerContext = piPeerDelta(peerFingerprints, {
          agentId,
          workspacePath,
          targetFiles,
          peers: result.peers,
        });
        if (!peerContext) return undefined;
        notify(ctx, peerContext);
        return { additionalContext: peerContext };
      } catch (error) {
        notify(ctx, `Octocode awareness warning; continuing: ${error instanceof Error ? error.message : String(error)}`, 'warning');
        return undefined;
      }
    },

    async handleToolResult(event: PiToolEvent, ctx?: PiLikeContext) {
      const extracted = extractPiWriteTargetPaths(event?.toolName, event?.input);
      const dedupeKey = firstString(event?.toolCallId);
      if (!dedupeKey) {
        notify(ctx, 'Octocode awareness post-edit warning: missing stable toolCallId; the matching write should have been blocked before execution.', 'warning');
        return undefined;
      }
      const trackedFiles = pendingToolRuns.has(dedupeKey) ? pendingToolFiles.get(dedupeKey) : undefined;
      const runId = pendingToolRuns.get(dedupeKey);
      const fallbackFiles = trackedFiles ?? extracted;
      if (fallbackFiles.length === 0 && !runId) return undefined;

      pendingToolFiles.delete(dedupeKey);
      pendingToolRuns.delete(dedupeKey);
      try {
        const db = getDb(ctx);
        const agentId = getPiAwarenessAgentId(ctx);
        const sessionId = getPiAwarenessSessionId(ctx);
        const workspacePath = ctx?.cwd ?? process.cwd();
        const artifact = artifactFrom(ctx, event as Record<string, unknown>);
        if (!runId) {
          notify(ctx, 'Octocode awareness post-edit warning; continuing: missing correlated work run.', 'warning');
          return undefined;
        }
        if (event.isError === true) {
          if (workRunOrigin(db, runId) === 'HOOK') {
            discardUncommittedHookFiles(db, {
              agentId,
              runId,
              targetFiles: fallbackFiles,
              workspacePath,
            });
          }
          return undefined;
        }
        if (workRunOrigin(db, runId) === 'HOOK' && isAggregatedPiFallbackRun(db, runId)) {
          touchWork(db, { agentId, runId, ttlMs: 10 * 60_000 });
        } else if (workRunOrigin(db, runId) === 'HOOK') {
          endWork(db, { agentId, runId, targetFiles: fallbackFiles });
        } else {
          touchWork(db, { agentId, runId, targetFiles: fallbackFiles, ttlMs: 10 * 60_000 });
        }
        for (const file of fallbackFiles) {
          insertEditLog(db, {
            sessionId,
            runId,
            agentId,
            filePath: resolvePiTargetPath(file, workspacePath),
            operation: 'update',
            workspacePath,
            artifact,
          });
        }
      } catch (error) {
        notify(ctx, `Octocode awareness warning; continuing: ${error instanceof Error ? error.message : String(error)}`, 'warning');
      }
      return undefined;
    },

    async handleBeforeAgentStart(_event: Record<string, unknown> = {}, ctx?: PiLikeContext) {
      const awarenessSessionId = getPiAwarenessSessionId(ctx);
      const interventionQuery = firstString(
        _event.prompt,
        _event.text,
        _event.user_prompt,
        _event.userPrompt,
        latestPromptBySession.get(awarenessSessionId),
      );
      latestPromptBySession.delete(awarenessSessionId);
      // ARCH-5: Register / refresh agent identity at the start of each session.
      // Uses OCTOCODE_AGENT_NAME env (if set) or session file basename as display name.
      try {
        const db = getDb(ctx);
        const agentId = getPiAwarenessAgentId(ctx);
        const envName = process.env.OCTOCODE_AGENT_NAME ?? '';
        const sessionFile = ctx?.sessionManager?.getSessionFile?.();
        const derivedName = envName
          || (sessionFile ? path.basename(sessionFile, path.extname(sessionFile)) : '');
        registerAgent(db, { agentId, agentName: derivedName, workspacePath: ctx?.cwd ?? process.cwd(), artifact: artifactFrom(ctx, _event), context: 'pi' });
      } catch { /* fail-open: identity registration is non-critical */ }

      if (process.env.OCTOCODE_NO_NOTIFY === '1') return undefined;
      try {
        const db = getDb(ctx);
        const result = notifyGet(db, {
          agent_id: getPiAwarenessAgentId(ctx),
          session_id: awarenessSessionId,
          workspace: ctx?.cwd ?? process.cwd(),
          artifact: artifactFrom(ctx, _event),
          query: interventionQuery ?? undefined,
          format: 'hook',
        }) as { additionalContext?: string };
        if (!result.additionalContext) return undefined;
        return {
          message: {
            customType: 'octocode-awareness-briefing',
            content: result.additionalContext,
            display: false,
          },
        };
      } catch (error) {
        notify(ctx, `Octocode awareness briefing warning; continuing: ${error instanceof Error ? error.message : String(error)}`, 'warning');
        return undefined;
      }
    },

    async handleSessionStart(event: Record<string, unknown> = {}, ctx?: PiLikeContext) {
      try {
        const db = getDb(ctx);
        const agentId = getPiAwarenessAgentId(ctx);
        const workspacePath = ctx?.cwd ?? process.cwd();
        const artifact = artifactFrom(ctx, event);
        ensurePiSession(db, {
          agentId,
          sessionId: getPiAwarenessSessionId(ctx),
          workspacePath,
          artifact,
        });
        registerAgent(db, { agentId, workspacePath, artifact, context: 'pi' });
      } catch {
        // fail-open: session registration is advisory
      }
      return undefined;
    },

    async handleSessionShutdown(event: Record<string, unknown> = {}, ctx?: PiLikeContext) {
      latestPromptBySession.delete(getPiAwarenessSessionId(ctx));
      try {
        const db = getDb(ctx);
        const agentId = getPiAwarenessAgentId(ctx);
        const sessionId = getPiAwarenessSessionId(ctx);
        const workspacePath = ctx?.cwd ?? process.cwd();
        const artifact = artifactFrom(ctx, event);
        finalizeActivePiFallbackRuns(db, {
          agentId,
          sessionId,
          workspacePath,
          artifact,
        });
        endSession(db, { sessionId, agentId, workspacePath, artifact });
        if (process.env.OCTOCODE_NO_SESSION_CAPTURE === '1' || event.reason === 'new') return undefined;
        sessionCapture(db, {
          agent_id: agentId,
          workspace: workspacePath,
          artifact,
          reason: event.reason,
        });
      } catch {
        // fail-open: shutdown hooks must never wedge session replacement/quit
      }
      return undefined;
    },

    async handleSessionCompact(event: Record<string, unknown> = {}, ctx?: PiLikeContext) {
      latestPromptBySession.delete(getPiAwarenessSessionId(ctx));
      try {
        const db = getDb(ctx);
        finalizeActivePiFallbackRuns(db, {
          agentId: getPiAwarenessAgentId(ctx),
          sessionId: getPiAwarenessSessionId(ctx),
          workspacePath: ctx?.cwd ?? process.cwd(),
          artifact: artifactFrom(ctx, event),
        });
        if (process.env.OCTOCODE_NO_SESSION_CAPTURE === '1') return undefined;
        sessionCapture(db, {
          agent_id: getPiAwarenessAgentId(ctx),
          workspace: ctx?.cwd ?? process.cwd(),
          artifact: artifactFrom(ctx, event),
          reason: event.reason ?? 'compact',
        });
      } catch {
        // fail-open: compaction hooks must never wedge the host
      }
      return undefined;
    },
  };
}
