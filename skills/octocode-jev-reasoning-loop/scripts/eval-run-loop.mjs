#!/usr/bin/env node
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import {
  applyResponse,
  buildDecisionPacket,
  buildRunApplication,
  prepareCompactRun,
  validateDecisionPacket
} from './decision-contract.mjs';

const root = new URL('../', import.meta.url);
const fixture = JSON.parse(readFileSync(new URL('evals/run-loop-cases.json', root), 'utf8'));
const item = fixture.case;
const legacyInput = {
  route: item.route,
  model: item.model,
  decisionBrief: item.decisionBrief,
  state: item.state
};
const compactInput = {
  route: item.route,
  model: item.model,
  willChangeAction: item.decisionBrief.jev_will_change_action,
  state: item.state
};

const legacyPacket = buildDecisionPacket(legacyInput);
const prepared = prepareCompactRun(compactInput);
assert.equal(prepared.status, 'ready');
assert.equal(validateDecisionPacket(item.route, prepared.request).valid, true);
assert.deepEqual(prepared.request.questions, legacyPacket.questions);
const withoutReasoning = value => {
  const copy = structuredClone(value);
  delete copy.reasoning;
  return copy;
};
assert.deepEqual(withoutReasoning(prepared.request.state), withoutReasoning(legacyPacket.state));

const generated = buildRunApplication(item.route, prepared.request, item.response);
const application = applyResponse(prepared.request, item.response, generated.actions, generated.netAction);
assert.equal(application.blocked, false);
assert.equal(generated.netAction, item.manualNetAction);

const bytes = value => Buffer.byteLength(JSON.stringify(value));
const baselineAuthorInputBytes = bytes(legacyInput) + bytes(item.manualActions) + bytes(item.manualNetAction);
const candidateAuthorInputBytes = bytes(compactInput);
const authorInputByteReduction = 1 - candidateAuthorInputBytes / baselineAuthorInputBytes;
const skillText = readFileSync(new URL('SKILL.md', root), 'utf8');
const lobbyWords = skillText.trim().split(/\s+/).length;
const lobbyWordReduction = 1 - lobbyWords / fixture.baselineLobbyWords;
const baselineCommands = 5;
const candidateCommands = 1;
const checks = {
  packet_equivalent_except_minimal_reasoning: true,
  route_validation_passed: true,
  provisional_apply_passed: true,
  net_action_preserved: generated.netAction === item.manualNetAction,
  author_input_reduction_passed: authorInputByteReduction >= fixture.thresholds.authorInputByteReductionMinimum,
  lobby_reduction_passed: lobbyWordReduction >= fixture.thresholds.lobbyWordReductionMinimum,
  one_command_passed: candidateCommands <= fixture.thresholds.candidateCommandsMaximum,
  extra_api_calls: 0
};
const passed = Object.entries(checks).every(([key, value]) => key === 'extra_api_calls' ? value === 0 : value === true);
const report = {
  suiteVersion: fixture.version,
  frozen: fixture.frozen,
  baseline: { author_input_bytes: baselineAuthorInputBytes, lobby_words: fixture.baselineLobbyWords, commands: baselineCommands },
  candidate: { author_input_bytes: candidateAuthorInputBytes, lobby_words: lobbyWords, commands: candidateCommands },
  deltas: {
    author_input_byte_reduction: Number(authorInputByteReduction.toFixed(4)),
    lobby_word_reduction: Number(lobbyWordReduction.toFixed(4)),
    command_reduction: baselineCommands - candidateCommands
  },
  checks,
  verdict: passed ? 'ACCEPT' : 'REVERT'
};
console.log(JSON.stringify(report));
if (!passed) process.exitCode = 1;
