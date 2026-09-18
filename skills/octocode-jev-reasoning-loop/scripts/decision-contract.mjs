import { readFileSync } from 'node:fs';

const DEFAULT_POLICY = JSON.parse(readFileSync(new URL('../assets/default-policy.json', import.meta.url), 'utf8'));
const ROUTE_TYPES = {
  hunch_check: ['noul'],
  hypothesis_triage: ['choice', 'choice'],
  decision_review: ['noul', 'choice', 'noul'],
  reflection_delta: ['choice', 'choice', 'noul'],
  disputed_inference: ['choice', 'choice'],
  hallucination_gate: ['noul', 'choice']
};
const COST = { trivial: 0, low: 1, medium: 2, high: 3 };
const EFFECTS = new Set(['strengthen', 'weaken', 'neutral']);
const isObject = value => value !== null && typeof value === 'object' && !Array.isArray(value);
const nonempty = value => typeof value === 'string' && value.trim().length > 0;
const ownKeys = value => isObject(value) ? Object.keys(value) : [];
const unique = values => new Set(values).size === values.length;
const exactKeys = (value, keys) => isObject(value) && ownKeys(value).length === keys.length && keys.every(key => Object.hasOwn(value, key));
const criteriaText = item => item.statement || item.action || item.description || item.observation || item.id;

function question(type, instructions, criteria) {
  return { type, instructions, ...(criteria === undefined ? {} : { criteria }) };
}
function choiceCriteria(items, noneText) {
  return Object.fromEntries([...items.map(item => [item.id, criteriaText(item)]), ['none', noneText]]);
}
function evidenceIds(state) {
  const ids = Array.isArray(state?.evidence) ? state.evidence.map(item => item.id) : [];
  if (isObject(state?.newEvidence)) ids.push(state.newEvidence.id);
  return ids.filter(nonempty);
}
function referencedIds(value, pattern) {
  return [...JSON.stringify(value).matchAll(pattern)].map(match => match[0]);
}
function publicReasoning(brief) {
  const result = { observations: brief.observations, uncertainty: brief.uncertainty };
  for (const field of ['inferences', 'assumptions', 'strongest_counter', 'prediction', 'falsifier', 'discriminating_observation']) {
    if (Object.hasOwn(brief, field)) result[field] = structuredClone(brief[field]);
  }
  return result;
}

export function routeDecision(input, policy = DEFAULT_POLICY) {
  let result;
  if (input.willChangeAction === false) result = ['no_jev', [], 'act_without_jev', false];
  else if (input.exactLookupAvailable) result = ['deterministic', [], 'run_direct_check', false];
  else if (Number(input.jevCallsAtCrossroad || 0) >= policy.maxJevCallsPerCrossroad) result = ['no_jev', [], 'continue_host_research', false];
  else if (input.aboutToAssert && Number(input.evidenceCount || 0) === 0) result = ['deterministic', [], 'block_ungrounded_claim', false];
  else if (input.aboutToAssert && input.scopeCompatible === false) result = ['deterministic', [], 'narrow_or_block_claim', false];
  else if (input.evidenceFresh === false) result = ['missing_fact', [], 'refresh_evidence', false];
  else if (input.newEvidence && Number(input.previousHypothesisCount || 0) > 0) result = ['reflection_delta', ROUTE_TYPES.reflection_delta, 'update_or_reframe', false];
  else if (input.aboutToAssert) result = ['hallucination_gate', ROUTE_TYPES.hallucination_gate, 'gate_then_qualify', false];
  else if (input.evidenceCollected && input.boundedClaim) result = ['disputed_inference', ROUTE_TYPES.disputed_inference, 'verify_selected_basis', false];
  else if (input.actionCost === 'high' && policy.highCostReviewRequired || input.difficultToReverse || typeof input.judgmentGap === 'number' && input.judgmentGap < policy.softTieGap) result = ['decision_review', ROUTE_TYPES.decision_review, 'review_before_action', false];
  else if (Number(input.hypothesisCount || 0) >= 2 && input.checksDiscriminate === false) result = ['missing_fact', [], 'design_discriminating_checks', false];
  else if (Number(input.hypothesisCount || 0) >= 2) result = ['hypothesis_triage', ROUTE_TYPES.hypothesis_triage, 'test_selected_check', false];
  else if (input.weakHunch) result = ['hunch_check', ROUTE_TYPES.hunch_check, 'promote_or_drop_hunch', false];
  else result = ['missing_fact', [], 'retrieve_or_frame_evidence', false];
  return { route: result[0], questionTypes: [...result[1]], policyAction: result[2], mayAssert: result[3] };
}

function compactChecksDiscriminate(checks) {
  return Array.isArray(checks) && checks.length >= 2 && checks.every(check =>
    Array.isArray(check?.expectedOutcomes) && check.expectedOutcomes.length >= 2 &&
    check.expectedOutcomes.some(outcome => new Set(Object.values(outcome?.effect || {})).size > 1)
  );
}

function compactRoutingInput(input) {
  const state = input.state || {};
  const evidence = Array.isArray(state.evidence) ? state.evidence : [];
  const base = {
    willChangeAction: input.willChangeAction,
    exactLookupAvailable: input.directCheck?.available === true,
    jevCallsAtCrossroad: input.jevCallsAtCrossroad || 0,
    ...(Object.hasOwn(input, 'evidenceFresh') ? { evidenceFresh: input.evidenceFresh } : {})
  };
  switch (input.route) {
    case 'hunch_check': return { ...base, weakHunch: true, observationsCount: nonempty(state.basis) ? 1 : 0 };
    case 'hypothesis_triage': return { ...base, observationsCount: evidence.length, hypothesisCount: state.hypotheses?.length || 0, checksDiscriminate: compactChecksDiscriminate(state.next_checks) };
    case 'decision_review': return { ...base, actionCost: state.actionCost, difficultToReverse: state.difficultToReverse === true };
    case 'reflection_delta': return { ...base, newEvidence: isObject(state.newEvidence), previousHypothesisCount: state.hypotheses?.length || 0 };
    case 'disputed_inference': return { ...base, evidenceCollected: evidence.length > 0, boundedClaim: nonempty(state.claim) };
    case 'hallucination_gate': return {
      ...base,
      aboutToAssert: true,
      evidenceCount: evidence.length,
      scopeCompatible: !nonempty(state.claim_scope) || evidence.some(item => item?.scope === state.claim_scope)
    };
    default: return base;
  }
}

function compactObservations(route, state) {
  if (route === 'reflection_delta' && isObject(state.newEvidence)) return `New evidence: ${state.newEvidence.id} (${state.newEvidence.source}).`;
  if (Array.isArray(state.evidence) && state.evidence.length > 0) {
    return `Anchored evidence: ${state.evidence.map(item => `${item.id} (${item.source})`).join(', ')}.`;
  }
  if (route === 'hunch_check') return `Supplied basis: ${state.basis}`;
  if (route === 'decision_review') return 'The proposal, assumptions, risks, cost, and reversibility are supplied in state.';
  return 'The route-specific state contains the current bounded observation.';
}

export function prepareCompactRun(input, policy = DEFAULT_POLICY) {
  if (!isObject(input)) throw new Error('$ expected an object; received non-object input.');
  if (!Object.hasOwn(ROUTE_TYPES, input.route)) throw new Error(`$.route expected one of ${Object.keys(ROUTE_TYPES).join(', ')}; received ${JSON.stringify(input.route)}.`);
  if (typeof input.willChangeAction !== 'boolean') throw new Error(`$.willChangeAction expected boolean; received ${JSON.stringify(input.willChangeAction)}.`);
  if (!isObject(input.state)) throw new Error(`$.state expected object; received ${JSON.stringify(input.state)}.`);
  if (input.directCheck !== undefined && (!isObject(input.directCheck) || typeof input.directCheck.available !== 'boolean' || input.directCheck.available && !nonempty(input.directCheck.action))) {
    throw new Error('$.directCheck expected { available: boolean, action?: nonempty string }; received an invalid value.');
  }
  const hasActions = Object.hasOwn(input, 'actions');
  const hasNetAction = Object.hasOwn(input, 'netAction');
  if (hasActions !== hasNetAction) throw new Error('$.actions and $.netAction must be supplied together or both omitted.');
  if (hasActions && (!isObject(input.actions) || !nonempty(input.netAction) || input.netAction.trim().length < 10)) {
    throw new Error('$.actions expected an object and $.netAction expected at least 10 characters.');
  }
  const routing = routeDecision(compactRoutingInput(input), policy);
  if (routing.route !== input.route) {
    const status = ['no_jev', 'deterministic'].includes(routing.route) ? 'skipped' : routing.route === 'missing_fact' ? 'needs_evidence' : 'redirected';
    return {
      status,
      routing,
      nextAction: input.directCheck?.available ? input.directCheck.action : routing.policyAction
    };
  }
  const reasoning = isObject(input.reasoning) ? input.reasoning : {};
  const brief = {
    observations: reasoning.observations || compactObservations(input.route, input.state),
    uncertainty: reasoning.uncertainty || input.state.goal || input.state.mainGoal,
    direct_check: input.directCheck || { available: false },
    jev_will_change_action: input.willChangeAction
  };
  for (const field of ['inferences', 'assumptions', 'strongest_counter', 'prediction', 'falsifier', 'discriminating_observation']) {
    if (Object.hasOwn(reasoning, field)) brief[field] = structuredClone(reasoning[field]);
  }
  const request = buildDecisionPacket({
    route: input.route,
    model: input.model,
    decisionBrief: brief,
    state: input.state,
    jevCallsAtCrossroad: input.jevCallsAtCrossroad
  }, policy);
  return { status: 'ready', routing, route: input.route, request };
}

function buildQuestions(route, state) {
  switch (route) {
    case 'hunch_check':
      return {
        worth_pursuing: question('noul', 'Based solely on `state.basis`, is `state.hunch` a useful lead worth turning into competing falsifiable hypotheses?', { true: 'The hunch is a useful lead to test.', false: 'The supplied basis does not justify pursuing the hunch.' })
      };
    case 'hypothesis_triage':
      return {
        hypothesis: question('choice', 'Which supplied hypothesis in `state.hypotheses` best fits the observations as a provisional lead? Select none when the deck is inadequate.', choiceCriteria(state.hypotheses, 'No supplied hypothesis is a useful lead.')),
        next_check: question('choice', 'Which supplied check in `state.next_checks` is expected to reduce uncertainty between the hypotheses most effectively? Judge discrimination only; host policy handles cost. Select none when no check separates them.', choiceCriteria(state.next_checks, 'No supplied check usefully distinguishes the hypotheses.'))
      };
    case 'reflection_delta':
      return {
        effect_on_prior_lead: question('choice', 'How does `state.newEvidence` affect `state.priorLead` relative to the predictions and expected outcomes?', { strengthens: 'The new evidence matches a prediction of the prior lead.', weakens: 'The new evidence conflicts with a prediction of the prior lead.', neutral: 'The evidence does not materially distinguish the prior lead.', ambiguous: 'Its effect cannot be resolved from the supplied state.', none: 'The supplied effect labels do not fit the observation.' }),
        updated_lead: question('choice', 'After applying only `state.newEvidence`, which supplied hypothesis is the best provisional lead?', choiceCriteria(state.hypotheses, 'No supplied hypothesis adequately explains the updated evidence.')),
        reframe_needed: question('noul', 'Does `state.newEvidence` indicate that the supplied hypothesis set is no longer an adequate framing?', { true: 'The hypothesis set should be replaced or expanded.', false: 'The current set remains adequate for another check.' })
      };
    case 'decision_review':
      return {
        proposal_viable: question('noul', 'Is `state.proposal` viable enough to execute given the supplied assumptions, risks, cost, and reversibility?', { true: 'The proposal remains viable with its stated safeguards.', false: 'A supplied risk or assumption blocks the proposal.' }),
        primary_risk: question('choice', 'Which supplied risk most threatens the usefulness or safety of `state.proposal`?', choiceCriteria(state.risks, 'No supplied risk materially blocks the proposal.')),
        more_evidence_needed: question('noul', 'Should the host retrieve more evidence before executing `state.proposal`?', { true: 'Retrieve evidence before acting.', false: 'The supplied state is adequate for a provisional action.' })
      };
    case 'disputed_inference':
      return {
        claim_status: question('choice', 'Assess `state.claim` using only the supplied evidence and counterclaim.', { supported: 'Evidence establishes the claim.', contradicted: 'Evidence establishes that the claim is false.', insufficient: 'Evidence is not enough to decide.', conflicting: 'Evidence gives an unresolved contradiction.' }),
        decisive_basis: question('choice', 'Which supplied evidence basis most directly establishes or refutes `state.claim`?', choiceCriteria(state.evidence_bases, 'No supplied basis settles the claim.'))
      };
    case 'hallucination_gate': {
      const questions = {
        grounded: question('noul', 'Based solely on `state.evidence`, is `state.claim` directly grounded?', { true: 'At least one evidence item directly supports the claim.', false: 'No evidence item directly supports the claim.' }),
        evidence_anchor: question('choice', 'Which supplied evidence item most directly grounds `state.claim`? Select none when no item does.', choiceCriteria(state.evidence, 'No supplied evidence directly grounds the claim.'))
      };
      if (state.claim_scope) questions.scope_matches = question('noul', 'Does `state.claim_scope` match the scope of the supplied grounding evidence?', { true: 'Claim and evidence scopes match.', false: 'The claim is broader or otherwise incompatible.' });
      return questions;
    }
    default: throw new Error(`Route '${route}' does not use a Jev packet.`);
  }
}

function validateBrief(brief, errors) {
  const required = ['observations', 'uncertainty', 'direct_check', 'jev_will_change_action'];
  if (!isObject(brief)) return errors.push('decisionBrief must be an object.');
  for (const field of required) if (!Object.hasOwn(brief, field)) errors.push(`decisionBrief.${field} is required.`);
  for (const field of ['observations', 'uncertainty']) if (!nonempty(brief[field])) errors.push(`decisionBrief.${field} must be nonempty.`);
  for (const field of ['strongest_counter', 'prediction', 'falsifier', 'discriminating_observation']) if (Object.hasOwn(brief, field) && !nonempty(brief[field])) errors.push(`decisionBrief.${field} must be nonempty when supplied.`);
  for (const field of ['inferences', 'assumptions']) if (Object.hasOwn(brief, field) && (!Array.isArray(brief[field]) || brief[field].some(item => !nonempty(item)))) errors.push(`decisionBrief.${field} must be an array of nonempty strings when supplied.`);
  if (!isObject(brief.direct_check) || typeof brief.direct_check.available !== 'boolean') errors.push('decisionBrief.direct_check.available must be boolean.');
  if (brief.direct_check?.available && !nonempty(brief.direct_check.action)) errors.push('decisionBrief.direct_check.action is required when available.');
  if (typeof brief.jev_will_change_action !== 'boolean') errors.push('decisionBrief.jev_will_change_action must be boolean.');
}

function validateGeneric(request, errors) {
  if (!exactKeys(request, ['model', 'state', 'questions'])) errors.push('Request must contain exactly model, state, and questions.');
  if (!nonempty(request?.model)) errors.push('model must be nonempty.');
  if (!isObject(request?.state)) errors.push('state expected an object.');
  if (isObject(request?.state)) {
    if (!isObject(request.state.reasoning)) errors.push('state.reasoning expected a bounded reasoning-summary object.');
    else {
      if (!nonempty(request.state.reasoning.observations)) errors.push(`state.reasoning.observations expected a nonempty decision-context summary; received ${JSON.stringify(request.state.reasoning.observations)}.`);
      if (!nonempty(request.state.reasoning.uncertainty)) errors.push(`state.reasoning.uncertainty expected a nonempty uncertainty summary; received ${JSON.stringify(request.state.reasoning.uncertainty)}.`);
    }
  }
  if (!isObject(request?.questions) || ownKeys(request.questions).length === 0) errors.push('questions must be a nonempty object.');
  for (const [id, q] of Object.entries(request?.questions || {})) {
    if (!isObject(q) || !['choice', 'noul', 'score'].includes(q.type)) errors.push(`questions.${id}.type is invalid.`);
    if (!nonempty(q?.instructions)) errors.push(`questions.${id}.instructions must be nonempty.`);
    if (q?.type === 'choice' && (!isObject(q.criteria) || ownKeys(q.criteria).length === 0)) errors.push(`questions.${id}.criteria must be a nonempty object.`);
    if (q?.type === 'score' && (!Array.isArray(q.criteria) || q.criteria.length < 2)) errors.push(`questions.${id}.criteria must contain ordered levels.`);
  }
}

function validateEvidence(state, errors) {
  if (!Array.isArray(state.evidence) || state.evidence.length === 0) return errors.push('state.evidence must contain anchored evidence.');
  const ids = state.evidence.map(item => item?.id);
  if (!ids.every(nonempty) || !unique(ids)) errors.push('Evidence IDs must be nonempty and unique.');
  for (const item of state.evidence) {
    if (!isObject(item) || !nonempty(item.source) || !nonempty(item.scope) || !nonempty(item.content)) errors.push('Every evidence item needs source, scope, and content.');
    if (isObject(item) && !ownKeys(item).every(key => ['id', 'kind', 'source', 'scope', 'content'].includes(key))) errors.push(`Evidence ${item.id || '?'} contains unsupported fields.`);
  }
  if (nonempty(state.scope)) {
    state.evidence.forEach((item, index) => {
      if (item.scope !== state.scope) errors.push(`state.evidence[${index}].scope expected ${JSON.stringify(state.scope)}; received ${JSON.stringify(item.scope)}.`);
    });
  }
  const references = referencedIds(state.reasoning || {}, /\bE\d+\b/g);
  for (const id of references) if (!ids.includes(id)) errors.push(`Reasoning references unknown evidence ID ${id}.`);
}

function validateTriage(state, questions, errors) {
  validateEvidence(state, errors);
  if (!Array.isArray(state.hypotheses) || state.hypotheses.length < 2 || state.hypotheses.length > 5) errors.push('Supply 2–5 hypotheses.');
  const hypotheses = Array.isArray(state.hypotheses) ? state.hypotheses : [];
  const hypIds = hypotheses.map(item => item?.id);
  if (!hypIds.every(id => /^H\d+$/.test(id)) || !unique(hypIds)) errors.push('Hypothesis IDs must be unique H<number> labels.');
  for (const hypothesis of hypotheses) {
    if (!exactKeys(hypothesis, ['id', 'statement', 'assumption', 'predicts', 'weakenedBy'])) errors.push(`Hypothesis ${hypothesis?.id || '?'} contains unsupported fields.`);
    if (!nonempty(hypothesis?.statement) || !nonempty(hypothesis?.assumption) || !nonempty(hypothesis?.weakenedBy)) errors.push(`Hypothesis ${hypothesis?.id || '?'} needs statement, assumption, and weakenedBy.`);
    if (!Array.isArray(hypothesis?.predicts) || hypothesis.predicts.length === 0 || hypothesis.predicts.some(item => !nonempty(item))) errors.push(`Hypothesis ${hypothesis?.id || '?'} needs one or more predictions.`);
  }
  if (!Array.isArray(state.next_checks) || state.next_checks.length < 2) errors.push('Supply at least two next checks.');
  const checks = Array.isArray(state.next_checks) ? state.next_checks : [];
  const checkIds = checks.map(item => item?.id);
  if (!checkIds.every(id => /^C\d+$/.test(id)) || !unique(checkIds)) errors.push('Check IDs must be unique C<number> labels.');
  for (const check of checks) {
    if (!exactKeys(check, ['id', 'action', 'cost', 'expectedOutcomes'])) errors.push(`Check ${check?.id || '?'} contains unsupported fields or a pre-observed result.`);
    if (!nonempty(check?.action) || !Object.hasOwn(COST, check?.cost)) errors.push(`Check ${check?.id || '?'} needs an action and valid cost.`);
    if (!Array.isArray(check?.expectedOutcomes) || check.expectedOutcomes.length < 2) { errors.push(`Check ${check?.id || '?'} needs at least two expected outcomes.`); continue; }
    let discriminates = false;
    for (const outcome of check.expectedOutcomes) {
      if (!exactKeys(outcome, ['observation', 'effect']) || !nonempty(outcome?.observation) || !isObject(outcome?.effect)) errors.push(`Check ${check?.id || '?'} has an invalid expected outcome.`);
      const effectKeys = ownKeys(outcome?.effect);
      if (effectKeys.some(id => !hypIds.includes(id)) || Object.values(outcome?.effect || {}).some(value => !EFFECTS.has(value))) errors.push(`Check ${check?.id || '?'} references invalid hypothesis effects.`);
      if (new Set(Object.values(outcome?.effect || {})).size > 1) discriminates = true;
    }
    if (!discriminates) errors.push(`Check ${check?.id || '?'} does not distinguish hypotheses.`);
  }
  if (!exactKeys(questions?.hypothesis?.criteria, [...hypIds, 'none'])) errors.push('Hypothesis criteria must exactly match hypotheses plus none.');
  if (!exactKeys(questions?.next_check?.criteria, [...checkIds, 'none'])) errors.push('Next-check criteria must exactly match checks plus none.');
}

function validateReflection(state, questions, errors) {
  if (!nonempty(state.priorLead) || !Array.isArray(state.hypotheses) || state.hypotheses.length < 2 || !isObject(state.check) || !isObject(state.newEvidence)) errors.push('Reflection requires priorLead, hypotheses, check, and newEvidence.');
  const ids = Array.isArray(state.hypotheses) ? state.hypotheses.map(item => item.id) : [];
  if (!ids.includes(state.priorLead)) errors.push('priorLead must identify a supplied hypothesis.');
  for (const hypothesis of state.hypotheses || []) if (!Array.isArray(hypothesis.predicts) || hypothesis.predicts.length === 0) errors.push(`Reflection hypothesis ${hypothesis.id} needs predictions.`);
  if (!nonempty(state.newEvidence?.id) || !nonempty(state.newEvidence?.source) || !nonempty(state.newEvidence?.scope) || !nonempty(state.newEvidence?.content)) errors.push('newEvidence needs id, source, scope, and content.');
  if (!exactKeys(questions?.updated_lead?.criteria, [...ids, 'none'])) errors.push('updated_lead criteria must match hypotheses plus none.');
}

function validateReview(state, questions, _policy, errors) {
  if (!nonempty(state.proposal) || !Array.isArray(state.assumptions) || state.assumptions.length === 0 || !Array.isArray(state.risks) || state.risks.length === 0) errors.push('Decision review requires a proposal, assumptions, and risks.');
  const ids = Array.isArray(state.risks) ? state.risks.map(item => item.id) : [];
  if (!ids.every(id => /^R\d+$/.test(id)) || !unique(ids)) errors.push('Risk IDs must be unique R<number> labels.');
  if (!exactKeys(questions?.primary_risk?.criteria, [...ids, 'none'])) errors.push('primary_risk criteria must match risks plus none.');
}

function validateClaim(state, questions, errors) {
  validateEvidence(state, errors);
  if (!nonempty(state.claim) || !nonempty(state.counterclaim) || !Array.isArray(state.evidence_bases) || state.evidence_bases.length === 0) errors.push('Claim check requires claim, counterclaim, and evidence_bases.');
  const evidence = evidenceIds(state);
  const bases = Array.isArray(state.evidence_bases) ? state.evidence_bases : [];
  const ids = bases.map(item => item.id);
  for (const basis of bases) if (!Array.isArray(basis.evidenceIds) || basis.evidenceIds.length === 0 || basis.evidenceIds.some(id => !evidence.includes(id))) errors.push(`Evidence basis ${basis.id} references unknown evidence.`);
  if (!exactKeys(questions?.claim_status?.criteria, ['supported', 'contradicted', 'insufficient', 'conflicting'])) errors.push('claim_status criteria must use the four fixed labels.');
  if (!exactKeys(questions?.decisive_basis?.criteria, [...ids, 'none'])) errors.push('decisive_basis criteria must match bases plus none.');
}

function validateGate(state, questions, errors) {
  if (!nonempty(state.claim) || !Array.isArray(state.evidence) || state.evidence.length === 0) errors.push('Do not call the hallucination gate without evidence.');
  const ids = Array.isArray(state.evidence) ? state.evidence.map(item => item.id) : [];
  if (!exactKeys(questions?.evidence_anchor?.criteria, [...ids, 'none'])) errors.push('evidence_anchor criteria must match evidence plus none.');
  if (state.claim_scope) {
    state.evidence.forEach((item, index) => {
      if (!nonempty(item.scope)) errors.push(`state.evidence[${index}].scope expected a nonempty scope because state.claim_scope is set; received ${JSON.stringify(item.scope)}.`);
    });
    if (!state.evidence.some(item => item.scope === state.claim_scope)) {
      errors.push(`state.claim_scope expected one matching evidence scope; received ${JSON.stringify(state.claim_scope)} versus ${JSON.stringify(state.evidence.map(item => item.scope))}. Narrow or block before Jev.`);
    }
  }
}

export function validateDecisionPacket(route, request, policy = DEFAULT_POLICY) {
  const errors = [];
  validateGeneric(request, errors);
  if (!isObject(request?.state) || !isObject(request?.questions)) return { valid: false, errors };
  switch (route) {
    case 'hunch_check':
      if (!nonempty(request.state.hunch) || !nonempty(request.state.basis)) errors.push('Hunch route requires hunch and basis.');
      break;
    case 'hypothesis_triage': validateTriage(request.state, request.questions, errors); break;
    case 'reflection_delta': validateReflection(request.state, request.questions, errors); break;
    case 'decision_review': validateReview(request.state, request.questions, policy, errors); break;
    case 'disputed_inference': validateClaim(request.state, request.questions, errors); break;
    case 'hallucination_gate': validateGate(request.state, request.questions, errors); break;
    default: errors.push(`Unknown Jev route '${route}'.`);
  }
  for (const [id, q] of Object.entries(request.questions)) {
    const fixedClaimStatus = route === 'disputed_inference' && id === 'claim_status';
    if (q.type === 'choice' && !fixedClaimStatus && !Object.hasOwn(q.criteria || {}, 'none')) errors.push(`Choice question ${id} must include none.`);
  }
  return { valid: errors.length === 0, errors };
}

export function buildDecisionPacket(input, policy = DEFAULT_POLICY) {
  const errors = [];
  if (!isObject(input)) throw new Error('Builder input must be an object.');
  validateBrief(input.decisionBrief, errors);
  if (errors.length) throw new Error(errors.join(' '));
  if (input.decisionBrief.direct_check.available) throw new Error(`A direct deterministic check is available: ${input.decisionBrief.direct_check.action}`);
  if (!input.decisionBrief.jev_will_change_action) throw new Error('Do not call Jev when its answer cannot change the next action.');
  if (Number(input.jevCallsAtCrossroad || 0) >= policy.maxJevCallsPerCrossroad) throw new Error('Jev call limit reached for this crossroad; continue host research.');
  if (!ROUTE_TYPES[input.route]) throw new Error(`Route '${input.route}' does not require a Jev packet.`);
  const state = structuredClone(input.state || {});
  state.reasoning = publicReasoning(input.decisionBrief);
  const packet = {
    model: input.model || 'jev-latest',
    state,
    questions: input.questions ? structuredClone(input.questions) : buildQuestions(input.route, state)
  };
  const checked = validateDecisionPacket(input.route, packet, policy);
  if (!checked.valid) throw new Error(checked.errors.join(' '));
  return packet;
}

function selectedChoice(response, id) {
  return response?.answers?.[id]?.choice;
}
function selectedNoul(response, id) {
  return response?.answers?.[id]?.noul;
}

export function buildRunApplication(route, request, response, policy = DEFAULT_POLICY) {
  const actions = {};
  let netAction;
  switch (route) {
    case 'hunch_check': {
      const pursue = selectedNoul(response, 'worth_pursuing') >= policy.noul.leanYesMinimum;
      actions.worth_pursuing = pursue ? 'Frame competing falsifiable hypotheses before another Jev call.' : 'Drop the hunch and continue host evidence retrieval.';
      netAction = actions.worth_pursuing;
      break;
    }
    case 'hypothesis_triage': {
      const hypothesis = selectedChoice(response, 'hypothesis');
      const checkId = selectedChoice(response, 'next_check');
      const check = request.state.next_checks?.find(item => item.id === checkId);
      actions.hypothesis = hypothesis === 'none' ? 'Replace the hypothesis deck before testing.' : `Keep ${hypothesis} provisional until a real check observes one frozen branch.`;
      actions.next_check = check ? `Execute ${check.id}: ${check.action}` : 'Design a new discriminating check before continuing.';
      netAction = check?.action || actions.next_check;
      break;
    }
    case 'reflection_delta': {
      const effect = selectedChoice(response, 'effect_on_prior_lead');
      const lead = selectedChoice(response, 'updated_lead');
      const reframe = selectedNoul(response, 'reframe_needed') >= policy.noul.leanYesMinimum;
      actions.effect_on_prior_lead = `Record that new evidence ${effect || 'ambiguously affects'} ${request.state.priorLead}.`;
      actions.updated_lead = lead === 'none' ? 'Abandon the current lead and replace the deck.' : `Carry ${lead} only as the updated provisional lead.`;
      actions.reframe_needed = reframe ? 'Replace or expand the hypothesis deck before another check.' : 'Retain the current deck for the next host-owned check.';
      netAction = reframe ? actions.reframe_needed : actions.updated_lead;
      break;
    }
    case 'decision_review': {
      const viable = selectedNoul(response, 'proposal_viable') >= policy.noul.leanYesMinimum;
      const risk = selectedChoice(response, 'primary_risk');
      const retrieve = selectedNoul(response, 'more_evidence_needed') >= policy.noul.leanYesMinimum;
      actions.proposal_viable = viable ? 'Keep the proposal provisional behind its stated safeguards.' : 'Stop the proposal and redesign it before execution.';
      actions.primary_risk = risk === 'none' ? 'Record that no supplied risk was selected.' : `Mitigate supplied risk ${risk} before execution.`;
      actions.more_evidence_needed = retrieve ? 'Retrieve evidence that resolves the selected risk or assumption.' : 'Proceed only within the supplied evidence and safeguards.';
      netAction = retrieve ? actions.more_evidence_needed : viable ? actions.primary_risk : actions.proposal_viable;
      break;
    }
    case 'disputed_inference': {
      const status = selectedChoice(response, 'claim_status');
      const basis = selectedChoice(response, 'decisive_basis');
      actions.claim_status = `Treat the bounded claim as ${status || 'undecided'} and keep the judgment advisory.`;
      actions.decisive_basis = basis === 'none' ? 'Retrieve a decisive evidence basis before asserting the claim.' : `Reopen every source in evidence basis ${basis} before citation.`;
      netAction = ['supported', 'contradicted'].includes(status) ? actions.decisive_basis : 'Narrow the claim or retrieve evidence before reconsidering it.';
      break;
    }
    case 'hallucination_gate': {
      const grounded = selectedNoul(response, 'grounded') >= policy.groundedMinimum;
      const anchor = selectedChoice(response, 'evidence_anchor');
      actions.grounded = grounded ? 'Keep the assertion bounded to its grounding evidence.' : 'Block the assertion until direct grounding exists.';
      actions.evidence_anchor = anchor === 'none' ? 'Block the assertion because no anchor was selected.' : `Cite and reopen evidence anchor ${anchor} before assertion.`;
      if (Object.hasOwn(response?.answers || {}, 'scope_matches')) {
        const matches = selectedNoul(response, 'scope_matches') >= policy.groundedMinimum;
        actions.scope_matches = matches ? 'Keep the assertion inside the declared evidence scope.' : 'Narrow the assertion to a compatible evidence scope.';
      }
      netAction = grounded && anchor !== 'none' ? actions.evidence_anchor : actions.grounded;
      break;
    }
    default: throw new Error(`Route '${route}' cannot generate an application.`);
  }
  for (const id of ownKeys(request.questions)) if (!nonempty(actions[id])) actions[id] = `Inspect ${id} and choose the next host-owned action.`;
  return { actions, netAction };
}

function confidenceRead(gap, policy) {
  if (gap > policy.clearLeadGap) return 'sharp';
  if (gap >= policy.softTieGap) return 'moderate';
  return 'soft-tie';
}
function noulDirection(value, policy) {
  if (value >= policy.noul.strongYesMinimum) return 'strong-yes';
  if (value >= policy.noul.leanYesMinimum) return 'lean-yes';
  if (value > policy.noul.leanNoMaximum) return 'ambiguous';
  if (value > policy.noul.strongNoMaximum) return 'lean-no';
  return 'strong-no';
}
function requireAction(actions, id) {
  const action = actions?.[id];
  if (!nonempty(action) || action.trim().length < 10) throw new Error(`A concrete action is required for answer '${id}'.`);
  return action;
}

export function applyResponse(request, response, actions, netAction, policy = DEFAULT_POLICY) {
  if (!nonempty(response?.model) || !isObject(response?.answers)) throw new Error('Response needs model and answers.');
  if (!nonempty(netAction) || netAction.trim().length < 10) throw new Error('A concrete net action is required.');
  const requestIds = ownKeys(request?.questions);
  if (!exactKeys(response.answers, requestIds)) throw new Error('Response answer IDs differ from request questions.');
  const applied = [];
  const blockReasons = [];
  for (const id of requestIds) {
    const expected = request.questions[id];
    const answer = response.answers[id];
    const action = requireAction(actions, id);
    if (answer?.type !== expected.type) throw new Error(`Answer type differs for '${id}'.`);
    if (answer.type === 'choice') {
      const selected = answer.choice;
      if (!Object.hasOwn(expected.criteria, selected) || !isObject(answer.probabilities)) throw new Error(`Invalid Choice answer '${id}'.`);
      const probability = answer.probabilities[selected];
      const ranked = Object.entries(answer.probabilities).sort((a, b) => b[1] - a[1]);
      const runner = ranked.find(([label]) => label !== selected) || [selected, probability];
      const gap = Math.max(0, probability - runner[1]);
      const item = {
        question_id: id,
        answer_type: 'choice',
        selected_id: selected,
        selected_description: expected.criteria[selected],
        probability,
        runner_up_id: runner[0],
        runner_up_probability: runner[1],
        spread_gap: gap,
        confidence_read: confidenceRead(gap, policy),
        none_selected: selected === 'none',
        provisional: true,
        next_action: action
      };
      if (id === 'next_check' && item.confidence_read === 'soft-tie' && Array.isArray(request.state.next_checks)) {
        const close = ranked.filter(([, value]) => probability - value < policy.softTieGap).map(([label]) => request.state.next_checks.find(check => check.id === label)).filter(Boolean);
        if (close.length > 1) item.policy_preference = [...close].sort((a, b) => COST[a.cost] - COST[b.cost])[0].id;
      }
      if (selected === 'none') blockReasons.push(`${id} selected none; follow the route-specific reframe protocol.`);
      if (item.confidence_read === 'soft-tie' && !item.policy_preference) blockReasons.push(`${id} is a soft tie; widen evidence before commitment.`);
      applied.push(item);
    } else if (answer.type === 'noul') {
      if (typeof answer.noul !== 'number' || answer.noul < 0 || answer.noul > 1) throw new Error(`Invalid Noul answer '${id}'.`);
      const direction = noulDirection(answer.noul, policy);
      applied.push({ question_id: id, answer_type: 'noul', noul_value: answer.noul, direction, provisional: true, action });
      if (direction === 'ambiguous') blockReasons.push(`${id} is ambiguous; retrieve evidence instead of forcing a decision.`);
      if (id === 'grounded' && answer.noul < policy.groundedMinimum) blockReasons.push(`grounded is below policy minimum ${policy.groundedMinimum}.`);
      if (id === 'scope_matches' && answer.noul < policy.groundedMinimum) blockReasons.push('scope_matches is below policy minimum; narrow the claim.');
    } else {
      if (typeof answer.score !== 'number' || !isObject(answer.legend)) throw new Error(`Invalid Score answer '${id}'.`);
      applied.push({ question_id: id, answer_type: 'score', score_value: answer.score, level_interpretation: answer.legend[String(Math.round(answer.score))] ?? 'Between supplied levels; use the conservative adjacent level.', provisional: true, action });
    }
  }
  return {
    model_used: response.model,
    answers: applied,
    net_action: netAction,
    blocked: blockReasons.length > 0,
    block_reasons: blockReasons
  };
}

export { DEFAULT_POLICY };
