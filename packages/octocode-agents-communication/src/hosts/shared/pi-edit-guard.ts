import { checkHostWrite, plainHostPath } from '../lease-check.js';
import type { HostBinding, PiContext, PiEvents } from './command.js';

/** Why a structured edit was blocked; undefined lets the call run. */
type Admission = { block: true; reason: string } | undefined;

/**
 * Lease admission for Pi's structured edit and write tools: the same check as the Claude and OpenCode guards. Shell/custom
 * tools and later extension rewrites remain outside it, not silently fenced.
 */
export function piEditGuard(currentBinding: () => Partial<HostBinding> | null, isBoundContext: (ctx: PiContext) => boolean): (event: PiEvents['tool_call'], ctx: PiContext) => Promise<Admission> {
  return async (event, ctx) => {
    if (!['write', 'edit'].includes(event.toolName)) return undefined;
    const target = currentBinding();
    if (!target || !isBoundContext(ctx)) return {block: true, reason: 'File edit requires an active communication binding and owned lease.'};
    const path = event.input?.path;
    if (!plainHostPath(path) || !plainHostPath(ctx.cwd)) return {block: true, reason: 'File edit requires a plain path without host aliases or parent traversal.'};
    try {
      const covered = await checkHostWrite(target, {vendorSession: ctx.sessionManager.getSessionId(), cwd: ctx.cwd, path});
      if (currentBinding() !== target || !isBoundContext(ctx) || event.input?.path !== path) return {block: true, reason: 'Session or file path changed during lease validation; retry in the current session.'};
      if (!covered) return {block: true, reason: 'File edit blocked: acquire or renew your own covering lease before retrying.'};
    } catch {
      return {block: true, reason: 'File edit blocked because live lease ownership could not be verified.'};
    }
    return undefined;
  };
}
