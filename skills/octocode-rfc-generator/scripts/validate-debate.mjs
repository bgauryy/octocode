import { readFileSync, realpathSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { isDeepStrictEqual } from 'node:util';
import assert from 'node:assert/strict';
import { pathToFileURL } from 'node:url';

const object = value => value !== null && typeof value === 'object' && !Array.isArray(value);
const text = value => typeof value === 'string' && value.trim().length > 0;
const evidenceId = /^E[1-9]\d*$/;
const entry = value => value === null || typeof value === 'string' || object(value) || Array.isArray(value);
const only = (value, keys) => object(value) && Object.keys(value).every(key => keys.includes(key));
const typedQuestion = question => {
  if (!only(question, ['type', 'instructions', 'criteria']) ||
      !Object.hasOwn(question, 'instructions') || !entry(question.instructions)) return false;
  const criteria = question.criteria;
  if (question.type === 'noul') return criteria === undefined || criteria === null ||
    (only(criteria, ['true', 'false']) && Object.hasOwn(criteria, 'true') &&
      Object.hasOwn(criteria, 'false') && entry(criteria.true) && entry(criteria.false));
  if (question.type === 'choice') return object(criteria) && Object.keys(criteria).length >= 1 &&
    Object.keys(criteria).length <= 255 && Object.entries(criteria).every(([key, value]) => text(key) && entry(value));
  return question.type === 'score' && Array.isArray(criteria) && criteria.length >= 2 &&
    criteria.length <= 10 && criteria.every(entry);
};

// Structural provenance only: never establishes evidence truth or RFC readiness.
export function validateDebate(request, packet) {
  const errors = [];
  const review = packet?.review;
  if (!object(review) || !text(review.id) || !text(review.rfcRevision) ||
      !object(review.questions) || Object.keys(review.questions).length === 0 ||
      !Object.entries(review.questions).every(([id, question]) => /^Q[1-9]\d*$/.test(id) && typedQuestion(question)) ||
      !Array.isArray(review.criteria) || review.criteria.length === 0 || !review.criteria.every(text) ||
      !object(review.subject) || !['proposal', 'claim'].includes(review.subject.kind) || !text(review.subject.text)) {
    return { valid: false, errors: ['Worker packet needs review {id, rfcRevision, typed questions, criteria, subject:{kind:proposal|claim, text}}.'] };
  }
  const admission = packet?.admission;
  if (!object(admission) || admission.workersDisagree !== true ||
      !text(admission.remainingDisagreement) || !text(admission.evidenceDoesNotSettleBecause) ||
      !text(admission.directCheckUnavailableBecause) || !text(admission.currentAction) ||
      !text(admission.ifJudgeSupports) || !text(admission.ifJudgeRejects) ||
      admission.ifJudgeSupports.trim() === admission.ifJudgeRejects.trim() ||
      !object(admission.workerPositions) || !text(admission.workerPositions.A) ||
      !text(admission.workerPositions.B) ||
      admission.willChangeAction !== true || admission.directCheck?.available !== false ||
      admission.evidenceFresh !== true || admission.jevCallsAtCrossroad !== 0 ||
      admission.workerPositions.A.trim() === admission.workerPositions.B.trim()) {
    return { valid: false, errors: ['Worker packet needs an unresolved admission gate: workersDisagree:true, distinct A/B positions, remaining disagreement, why evidence/direct checks cannot settle it, current action, distinct support/reject actions, willChangeAction:true, directCheck.available:false, evidenceFresh:true, and jevCallsAtCrossroad:0.'] };
  }
  const original = packet?.evidence;
  if (!object(original) || Object.keys(original).length === 0) {
    return { valid: false, errors: ['Worker packet must contain a nonempty evidence ID map.'] };
  }
  const ids = Object.keys(original).sort();
  for (const id of ids) {
    if (!evidenceId.test(id) || !object(original[id]) ||
        !text(original[id].source) || !text(original[id].observation)) {
      errors.push(`Invalid worker evidence entry: ${id}.`);
    }
  }
  const queries = only(request, ['queries']) && Array.isArray(request.queries)
    ? request.queries : [request];
  if (queries.length === 0) {
    return { valid: false, errors: [...errors, 'Request must contain a Jev query.'] };
  }
  queries.forEach((query, index) => {
    const prefix = `Query ${index + 1}`;
    if (!only(query, ['state', 'questions']) || !object(query.state) || !object(query.questions)) {
      errors.push(`${prefix}: RFC review query must contain only state and questions; optional Jev sources require inspection before the worker snapshot.`);
      return;
    }
    const context = query.state;
    if (!isDeepStrictEqual(context.review, review)) {
      errors.push(`${prefix}: review differs from the frozen worker contract.`);
    }
    if (!isDeepStrictEqual(context.admission, admission)) {
      errors.push(`${prefix}: admission differs from the frozen worker gate.`);
    }
    if (!isDeepStrictEqual(query.questions, review.questions)) {
      errors.push(`${prefix}: typed questions differ from the frozen worker contract.`);
    }
    for (const duplicate of ['questionIds', 'rfcRevision', 'criteria', 'questions', 'proposal', 'claim']) {
      if (Object.hasOwn(context, duplicate)) {
        errors.push(`${prefix}: keep ${duplicate} inside review only; conflicting duplicate state is not supported.`);
      }
    }
    const evidence = context?.evidence;
    if (!Array.isArray(evidence)) {
      errors.push(`${prefix}: state.evidence must be an array.`);
      return;
    }
    const seen = new Set();
    for (const entry of evidence) {
      if (!object(entry) || !evidenceId.test(entry.id ?? '')) {
        errors.push(`${prefix}: invalid evidence entry.`);
        continue;
      }
      const { id, ...observation } = entry;
      if (seen.has(id)) errors.push(`${prefix}: duplicate evidence ${id}.`);
      seen.add(id);
      if (!Object.hasOwn(original, id)) errors.push(`${prefix}: unknown evidence ${id}.`);
      else if (!isDeepStrictEqual(observation, original[id])) {
        errors.push(`${prefix}: ${id} differs from the worker snapshot.`);
      }
    }
    for (const id of ids) {
      if (!seen.has(id)) errors.push(`${prefix}: missing worker evidence ${id}.`);
    }
    for (const role of ['A', 'B']) {
      for (const round of ['opening', 'rebuttal']) {
        const argument = context?.arguments?.[role]?.[round];
        if (!text(argument)) {
          errors.push(`${prefix}: missing ${role} ${round}.`);
          continue;
        }
        for (const id of new Set(argument.match(/\bE[1-9]\d*\b/g) ?? [])) {
          if (!Object.hasOwn(original, id)) errors.push(`${prefix}: ${role} ${round} cites unknown ${id}.`);
        }
      }
    }
  });
  const snapshot = JSON.stringify(ids.map(id => [id, original[id]]));
  return {
    valid: errors.length === 0,
    errors,
    evidenceHash: createHash('sha256').update(snapshot).digest('hex'),
    reviewHash: createHash('sha256').update(JSON.stringify(review)).digest('hex'),
    evidenceCount: ids.length,
  };
}

function selfTest() {
  const review = {
    id: 'review-1', rfcRevision: 'fixture-revision',
    questions: { Q1: { type: 'choice', instructions: 'Does state.evidence support advancing state.review.subject.text under state.review.criteria? Consider both state.arguments and state.missingEvidence.', criteria: { support: 'Safeguards satisfy the supplied criteria.', reject: 'Safeguards fail the supplied criteria.', insufficient: 'The evidence cannot resolve the question.', conflicting: 'Relevant evidence supports incompatible conclusions.' } } },
    criteria: ['Preserve compatibility.'], subject: { kind: 'proposal', text: 'Keep legacy support until compatibility is verified.' },
  };
  const admission = {
    workersDisagree: true,
    remainingDisagreement: 'Whether the compatibility risk justifies delaying the cutover.',
    evidenceDoesNotSettleBecause: 'The inspected receipt establishes current behavior but not the acceptable residual risk.',
    directCheckUnavailableBecause: 'The decision depends on a bounded risk tradeoff after the available compatibility checks.',
    currentAction: 'Hold the proposal in review.',
    ifJudgeSupports: 'Advance to owner review with the risk recorded.',
    ifJudgeRejects: 'Collect another compatibility safeguard before owner review.',
    workerPositions: { A: 'The existing safeguard is sufficient.', B: 'The residual risk remains too high.' },
    willChangeAction: true, directCheck: { available: false }, evidenceFresh: true, jevCallsAtCrossroad: 0,
  };
  const packet = { review, admission, evidence: { E1: { source: 'fixture:1', observation: 'Current receipt.' } } };
  const request = { state: {
    review, admission,
    evidence: [{ id: 'E1', ...packet.evidence.E1 }],
    arguments: {
      A: { opening: 'Supports E1.', rebuttal: 'Concedes limits of E1.' },
      B: { opening: 'Challenges E1.', rebuttal: 'Still uncertain about E1.' },
    },
    missingEvidence: ['Actual residual compatibility risk.'],
  }, questions: review.questions };
  assert.equal(validateDebate(request, packet).valid, true);
  assert.equal(validateDebate({ queries: [request] }, packet).valid, true);
  const mutations = [
    x => { x.state.evidence[0].observation = 'Different meaning.'; },
    x => { x.state.evidence = []; },
    x => { x.state.evidence.push({ ...x.state.evidence[0] }); },
    x => { x.state.evidence[0].id = 'E2'; },
    x => { delete x.state.arguments.B.rebuttal; },
    x => { x.state.arguments.A.opening = 'Unsupported E9.'; },
    x => { x.state.review.id = 'other-review'; },
    x => { x.state.review.rfcRevision = 'new-revision'; },
    x => { x.state.review.questions.Q1.instructions = 'A different question?'; },
    x => { x.state.review.questions = { Q2: review.questions.Q1 }; },
    x => { x.state.review.criteria = ['Ignore compatibility.']; },
    x => { x.state.review.subject.text = 'Remove legacy support immediately.'; },
    x => { x.state.proposal = 'Remove legacy support immediately.'; },
    x => { x.route = 'retired-route'; },
    x => { x.state.questionIds = ['Q99']; },
    x => { x.state.admission.ifJudgeRejects = 'A different action.'; },
    x => { x.state.admission.willChangeAction = false; },
    x => { x.state.admission.directCheck = { available: true, action: 'Run the exact check.' }; },
    x => { x.state.admission.jevCallsAtCrossroad = 1; },
    x => { x.state.admission.evidenceFresh = false; },
    x => { x.questions = { Q2: review.questions.Q1 }; },
    x => { x.questions = { Q1: { ...review.questions.Q1, instructions: 'Select the favored speaker.' } }; },
    x => { x.questions = { Q1: { ...review.questions.Q1, criteria: { support: 'Always choose this.' } } }; },
    x => { x.reasoning = 'Retired query field.'; },
    x => { x.sources = { unseen: { path: '/unreviewed/evidence.md' } }; },
  ];
  for (const mutate of mutations) {
    const broken = structuredClone(request);
    mutate(broken);
    assert.equal(validateDebate(broken, packet).valid, false);
  }
  const packetMutations = [
    x => { x.admission.workersDisagree = false; },
    x => { x.admission.ifJudgeRejects = x.admission.ifJudgeSupports; },
    x => { x.admission.workerPositions.B = x.admission.workerPositions.A; },
    x => { x.admission.willChangeAction = false; },
    x => { x.admission.directCheck.available = true; },
    x => { x.admission.evidenceFresh = false; },
    x => { x.admission.jevCallsAtCrossroad = 1; },
    x => { x.review.questions.Q1 = 'Untyped question'; },
    x => { x.review.questions.Q1.criteria = {}; },
  ];
  for (const mutate of packetMutations) {
    const broken = structuredClone(packet);
    mutate(broken);
    const matchingRequest = structuredClone(request);
    matchingRequest.state.review = broken.review;
    matchingRequest.state.admission = broken.admission;
    matchingRequest.questions = broken.review.questions;
    assert.equal(validateDebate(matchingRequest, broken).valid, false);
  }
  assert.equal(validateDebate(request, {}).valid, false);
  assert.equal(validateDebate({}, packet).valid, false);
  assert.equal(validateDebate({ queries: [] }, packet).valid, false);
  const claimPacket = structuredClone(packet);
  claimPacket.review.subject = { kind: 'claim', text: 'The current receipt establishes compatibility.' };
  claimPacket.review.questions.Q1.instructions = 'Classify support for state.review.subject.text using state.evidence, state.arguments and state.missingEvidence.';
  const claimRequest = structuredClone(request);
  claimRequest.state.review = claimPacket.review;
  claimRequest.questions = claimPacket.review.questions;
  assert.equal(validateDebate(claimRequest, claimPacket).valid, true);
  return { valid: true, selfTest: true, cases: mutations.length + packetMutations.length + 6 };
}

if (process.argv[1] && process.argv[1] !== '-' && import.meta.url === pathToFileURL(realpathSync(process.argv[1])).href) {
  try {
    const args = process.argv.slice(2);
    if (args[0] === '--help' || args.length === 0) {
      console.log('Usage: node validate-debate.mjs <request.json> <worker-packet.json>\n       node validate-debate.mjs --self-test');
      process.exitCode = args.length === 0 ? 2 : 0;
    } else if (args.length === 1 && args[0] === '--self-test') {
      console.log(JSON.stringify(selfTest()));
    } else if (args.length === 2) {
      const result = validateDebate(JSON.parse(readFileSync(args[0], 'utf8')), JSON.parse(readFileSync(args[1], 'utf8')));
      console.log(JSON.stringify(result, null, 2));
      process.exitCode = result.valid ? 0 : 1;
    } else {
      throw new Error('Use --help for supported arguments.');
    }
  } catch (error) {
    console.error(JSON.stringify({ valid: false, error: error instanceof SyntaxError ? 'Invalid JSON input.' : error.message }));
    process.exitCode = 2;
  }
}
