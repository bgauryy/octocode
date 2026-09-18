import { randomUUID } from 'node:crypto';
import type { PlanStep, StepInput } from './plan-types.js';

export const MAX_PLAN_STEPS = 40;
const MAX_STEP_CHARS = 160;

export function cleanContractText(value: unknown, max = 2_000): string | undefined {
  if (typeof value !== 'string') return undefined;
  const text = value.replace(/\s+/g, ' ').trim();
  return text ? text.slice(0, max) : undefined;
}

export function cleanPaths(value: unknown): string[] | undefined {
  if (!Array.isArray(value)) return undefined;
  const out = [...new Set(value.filter((item): item is string => typeof item === 'string').map((item) => item.trim()).filter(Boolean))].slice(0, 100);
  return out.length ? out : undefined;
}

function cleanDeps(deps: unknown): number[] | undefined {
  if (!Array.isArray(deps)) return undefined;
  const out = deps.filter((d): d is number => Number.isInteger(d) && d >= 1).slice(0, MAX_PLAN_STEPS);
  return out.length ? out : undefined;
}

export function cleanStepIds(ids: unknown): string[] | undefined {
  if (!Array.isArray(ids)) return undefined;
  const out = [...new Set(ids.filter((id): id is string => typeof id === 'string' && id.trim().length > 0).map((id) => id.trim()))].slice(0, MAX_PLAN_STEPS);
  return out.length ? out : undefined;
}

function cleanStepText(text: string): string {
  const oneLine = String(text ?? '').replace(/\s+/g, ' ').trim();
  return oneLine.length > MAX_STEP_CHARS ? `${oneLine.slice(0, MAX_STEP_CHARS - 1)}…` : oneLine;
}

export function cleanDecision(text: string, max = 300): string {
  const oneLine = String(text ?? '').replace(/\s+/g, ' ').trim();
  return oneLine.length > max ? `${oneLine.slice(0, max - 1)}…` : oneLine;
}

export function cleanReviewText(value: unknown, max = 8_000): string {
  return typeof value === 'string' ? value.trim().slice(0, max) : '';
}

export type NormalizedStepInput = Omit<PlanStep, 'status' | 'dependsOnStepIds' | 'awarenessTaskId'> & { dependsOn?: number[] };

export function normalizeInput(step: StepInput): NormalizedStepInput {
  if (typeof step === 'string') return { id: `step-${randomUUID()}`, text: cleanStepText(step) };
  const text = cleanStepText(step.text);
  const activeForm = step.activeForm ? cleanStepText(step.activeForm) : undefined;
  const dependsOn = cleanDeps(step.dependsOn);
  const paths = cleanPaths(step.paths);
  const reasoning = cleanContractText(step.reasoning);
  const acceptance = cleanContractText(step.acceptance);
  const checkCommand = cleanContractText(step.checkCommand);
  return {
    id: `step-${randomUUID()}`,
    text,
    ...(activeForm ? { activeForm } : {}),
    ...(dependsOn ? { dependsOn } : {}),
    ...(paths ? { paths } : {}),
    ...(reasoning ? { reasoning } : {}),
    ...(acceptance ? { acceptance } : {}),
    ...(checkCommand ? { checkCommand } : {}),
  };
}

export function dependencyIdsFromIndexes(indexes: number[] | undefined, list: Array<{ id: string }>, ownId: string): string[] | undefined {
  if (!indexes?.length) return undefined;
  const ids = [...new Set(indexes.flatMap((index) => {
    const dependency = list[index - 1];
    return dependency && dependency.id !== ownId ? [dependency.id] : [];
  }))];
  return ids.length ? ids : undefined;
}
