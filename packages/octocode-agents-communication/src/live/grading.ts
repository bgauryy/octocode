// Live-harness evidence: the database rows and worker events a trial collects, and the pure graders that name each check.
import type { Reply } from '../script-support.ts';

export interface MessageRow { id: number; sender: string; target: string; body: string; replyTo: number | null; replyRequired: number; wake: string }
export interface DeliveryRow { message: number; recipient: string; acknowledgedAt: number | null }
export interface DispatchRow { message: number; recipient: string; transport: string; state: string; submittedAt: number | null; error: string | null }
export interface SessionRow { id: string; name: string; vendor: string; vendorSession: string | null; expiresAt: number }
export interface LeaseRow { id: number | string | null; path: string; owner: string; acquiredAt: number }
export interface Rows { messages: MessageRow[]; deliveries: DeliveryRow[]; dispatches: DispatchRow[]; sessions: SessionRow[]; leases: LeaseRow[] }

/** The checks a grader names, each true or false. */
export type Checks = Record<string, boolean>;
/** One JSON line of a `run --trace` worker; only the fields a check reads are looked at. */
export type WorkerEvent = Reply;

/** A request the soak controller sent and the answer it expects. */
export interface SoakRequest { id: number; recipient: string; expected: string; sentAt: number }

type GradedMessage = Pick<MessageRow, 'id' | 'sender' | 'target' | 'body' | 'replyTo'>;
export interface Verdict { passed: boolean; checks: Checks }

export function gradeCollaboration({ messages, deliveries, request, notice, sender, receiver, expected, passiveTurns, initialMessages = [] }: {
  messages: GradedMessage[]; deliveries: DeliveryRow[]; request: number; notice: number; sender: string; receiver: string; expected: string; passiveTurns: number; initialMessages?: number[];
}): Verdict & { reply?: number } {
  const replies = messages.filter(row => row.replyTo === request && row.sender === receiver && row.target === sender);
  const acknowledged = (id: number): boolean => deliveries.some(row => row.message === id && row.recipient === receiver && row.acknowledgedAt !== null);
  const checks = {
    requestAcknowledged: acknowledged(request),
    noticeAcknowledged: acknowledged(notice),
    correlatedAnswer: replies.length === 1 && replies[0].body.trim() === expected,
    noticeHasNoReply: !messages.some(row => row.replyTo === notice),
    noReplyLoop: messages.every(row => initialMessages.includes(row.id) || [request, notice].includes(row.id) || replies.some(reply => reply.id === row.id)),
    passiveDoesNotStartTurn: passiveTurns === 0,
  };
  return { passed: Object.values(checks).every(Boolean), checks, reply: replies[0]?.id };
}

/** Lock handoff passes when the worker asked the owner, never overlapped the owner's lease,
 * acquired the path after release, and answered the request exactly. */
export function gradeHandoff({ messages, deliveries, leases, request, owner, worker, releasedAt, expected }: {
  messages: GradedMessage[]; deliveries: DeliveryRow[]; leases: Pick<LeaseRow, 'owner' | 'path' | 'acquiredAt'>[]; request: number; owner: string; worker: string; releasedAt: number | null; expected: string;
}): Verdict {
  const asked = messages.filter(row => row.sender === worker && row.target === owner);
  const acquired = leases.filter(row => row.owner === worker && row.path.endsWith('src/api.ts'));
  const replies = messages.filter(row => row.replyTo === request && row.sender === worker);
  const checks = {
    askedOwner: asked.length >= 1,
    noOverlap: acquired.every(row => releasedAt !== null && row.acquiredAt >= releasedAt),
    acquiredAfterRelease: acquired.length >= 1,
    correlatedAnswer: replies.length === 1 && replies[0].body.trim() === expected,
    requestAcknowledged: deliveries.some(row => row.message === request && row.recipient === worker && row.acknowledgedAt !== null),
  };
  return { passed: Object.values(checks).every(Boolean), checks };
}

/** Paged read passes when the worker answers with the code that only the last page holds. */
export function gradePagedRead({ messages, deliveries, request, worker, expected }: {
  messages: GradedMessage[]; deliveries: DeliveryRow[]; request: number; worker: string; expected: string;
}): Verdict {
  const replies = messages.filter(row => row.replyTo === request && row.sender === worker);
  const checks = {
    correlatedAnswer: replies.length === 1 && replies[0].body.trim() === expected,
    requestAcknowledged: deliveries.some(row => row.message === request && row.recipient === worker && row.acknowledgedAt !== null),
  };
  return { passed: Object.values(checks).every(Boolean), checks };
}

export function gradeConcurrentWave({ messages, deliveries, dispatches, requests, sender }: {
  messages: GradedMessage[]; deliveries: DeliveryRow[]; dispatches: Pick<DispatchRow, 'message' | 'recipient' | 'state' | 'submittedAt'>[]; requests: Pick<SoakRequest, 'id' | 'recipient' | 'expected'>[]; sender: string;
}): Verdict & { answers: number[] } {
  const answers = requests.flatMap(request => messages.filter(message => message.replyTo === request.id && message.sender === request.recipient && message.target === sender));
  const checks = {
    exactCorrelatedAnswers: requests.every(request => {
      const replies = answers.filter(answer => answer.replyTo === request.id);
      return replies.length === 1 && replies[0].body.trim() === request.expected;
    }),
    requestsAcknowledged: requests.every(request => deliveries.some(delivery => delivery.message === request.id && delivery.recipient === request.recipient && delivery.acknowledgedAt !== null)),
    requestsSubmitted: requests.every(request => dispatches.some(dispatch => dispatch.message === request.id && dispatch.recipient === request.recipient && dispatch.state === 'submitted' && dispatch.submittedAt !== null)),
    noExtraMessages: messages.every(message => requests.some(request => request.id === message.id) || answers.some(answer => answer.id === message.id)),
  };
  return { passed: Object.values(checks).every(Boolean), checks, answers: answers.map(answer => answer.id) };
}
