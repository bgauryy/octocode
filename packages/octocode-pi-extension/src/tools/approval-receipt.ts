import { randomUUID } from 'node:crypto';

import type { PiContext } from '../types.js';
import type { ApprovalClass, PermissionLevel } from '../contracts/protocols.js';
import { emitExecution } from './execution-runtime.js';
import { executionLabel } from './execution-presentation.js';

export type ApprovalResolution =
  | 'allow-policy'
  | 'allow-once'
  | 'allow-session'
  | 'deny-user'
  | 'deny-unavailable'
  | 'cancelled';

export interface ApprovalDecisionReceipt {
  requestId: string;
  requester: string;
  operation: string;
  scope: ApprovalClass;
  matchedPolicy: PermissionLevel | 'remembered';
  resolution: ApprovalResolution;
  decisionSource: 'policy' | 'user' | 'host';
  decidedAt: number;
}

export interface ApprovalOutcomeBase {
  approved: boolean;
  remembered: boolean;
  always: boolean;
  interactive: boolean;
}

export function createApprovalReceiptEmitter(
  ctx: PiContext | undefined,
  request: { actionClass: ApprovalClass; title: string },
) {
  const requestId = randomUUID();
  const requester = ctx?.sessionManager?.getSessionId?.() || 'session-unavailable';
  const operation = executionLabel(request.title);
  return {
    requested(matchedPolicy: ApprovalDecisionReceipt['matchedPolicy']): void {
      emitExecution(ctx, 'permission.requested', {
        id: requestId,
        title: operation,
        requester,
        operation,
        scope: request.actionClass,
        matchedPolicy,
      });
    },
    resolved(
      outcome: ApprovalOutcomeBase,
      resolution: ApprovalResolution,
      decisionSource: ApprovalDecisionReceipt['decisionSource'],
      matchedPolicy: ApprovalDecisionReceipt['matchedPolicy'],
    ): ApprovalOutcomeBase & { receipt: ApprovalDecisionReceipt } {
      const receipt: ApprovalDecisionReceipt = {
        requestId,
        requester,
        operation,
        scope: request.actionClass,
        matchedPolicy,
        resolution,
        decisionSource,
        decidedAt: Date.now(),
      };
      emitExecution(ctx, 'permission.resolved', {
        id: requestId,
        decision: resolution,
        requester,
        operation,
        scope: request.actionClass,
        matchedPolicy,
        decisionSource,
        decidedAt: receipt.decidedAt,
      }, 'transcript');
      return { ...outcome, receipt };
    },
  };
}
