import { createSessionArtifactContext, type SessionIdentityInput } from '../session-artifacts.js';
import type { ActivePlanContext } from './plan-types.js';

declare const planScopeBrand: unique symbol;
/** Opaque session/workspace key; it is not necessarily a filesystem working directory. */
export type PlanScope = string & { readonly [planScopeBrand]?: true };

interface ScopeBinding {
  identityInput: SessionIdentityInput;
}

const scopeBindings = new Map<PlanScope, ScopeBinding>();

export function activePlanScope(ctx?: ActivePlanContext): PlanScope {
  const cwd = ctx?.cwd ?? process.cwd();
  const sessionId = ctx?.sessionManager?.getSessionId?.()?.trim();
  const sessionFile = ctx?.sessionManager?.getSessionFile?.()?.trim();
  const scope = sessionId
    ? `${cwd}\0id:${sessionId}`
    : sessionFile
      ? `${cwd}\0${sessionFile}`
      : cwd;
  const planScope = scope as PlanScope;
  scopeBindings.set(planScope, { identityInput: { cwd, sessionManager: ctx?.sessionManager } });
  return planScope;
}

function bindingForScope(scope: PlanScope): ScopeBinding {
  const known = scopeBindings.get(scope);
  if (known) return known;
  const separator = scope.indexOf('\0');
  if (separator < 0) return { identityInput: { cwd: scope } };
  const cwd = scope.slice(0, separator);
  const discriminator = scope.slice(separator + 1);
  if (discriminator.startsWith('id:')) {
    const sessionId = discriminator.slice(3);
    return { identityInput: { cwd, sessionManager: { getSessionId: () => sessionId } } };
  }
  return {
    identityInput: { cwd, sessionManager: { getSessionFile: () => discriminator } },
  };
}

export function artifactContextForScope(scope: PlanScope) {
  return createSessionArtifactContext(bindingForScope(scope).identityInput);
}

export function workspaceForPlanScope(scope: PlanScope): string {
  return bindingForScope(scope).identityInput.cwd ?? scope.split('\0', 1)[0]!;
}

export function releasePlanScopeBinding(scope: PlanScope): void {
  scopeBindings.delete(scope);
}
