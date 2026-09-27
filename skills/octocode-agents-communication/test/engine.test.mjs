// COPPER — contract tests for the Snake engine.
import test from 'node:test';
import assert from 'node:assert/strict';
import { createGame, turn, step } from './engine.mjs';

const state = (overrides = {}) => ({
  width: 6, height: 6,
  snake: [{ x: 2, y: 2 }, { x: 1, y: 2 }, { x: 0, y: 2 }],
  direction: 'right', food: { x: 5, y: 5 }, score: 0, status: 'playing',
  ...overrides,
});
const same = (a, b) => assert.deepEqual(a, b);

test('createGame starts with a valid three-cell snake and non-overlapping food', () => {
  const game = createGame({ width: 8, height: 6, rng: () => 0 });
  assert.equal(game.snake.length, 3);
  assert.equal(game.status, 'playing');
  assert.ok(game.food);
  assert.ok(game.food.x >= 0 && game.food.x < game.width);
  assert.ok(game.food.y >= 0 && game.food.y < game.height);
  assert.ok(!game.snake.some(p => p.x === game.food.x && p.y === game.food.y));
});

test('step moves one cell and does not mutate its input', () => {
  const before = state();
  const snapshot = structuredClone(before);
  const next = step(before, () => 0);
  assert.deepEqual(next.snake[0], { x: 3, y: 2 });
  assert.equal(next.snake.length, before.snake.length);
  same(before, snapshot);
});

test('eating food grows the snake and increments score', () => {
  const before = state({ food: { x: 3, y: 2 } });
  const next = step(before, () => 0);
  assert.deepEqual(next.snake[0], { x: 3, y: 2 });
  assert.equal(next.snake.length, before.snake.length + 1);
  assert.equal(next.score, 1);
  assert.equal(next.status, 'playing');
  assert.ok(next.food);
  assert.ok(!next.snake.some(p => p.x === next.food.x && p.y === next.food.y));
});

test('turn rejects direct reversal without mutating the state', () => {
  const before = state();
  const snapshot = structuredClone(before);
  const next = turn(before, 'left');
  assert.equal(next.direction, 'right');
  same(before, snapshot);
});

test('step loses on a wall collision', () => {
  const before = state({ width: 3, snake: [{ x: 2, y: 2 }, { x: 1, y: 2 }], food: { x: 0, y: 0 } });
  const next = step(before);
  assert.equal(next.status, 'lost');
});

test('step loses on a non-vacating body collision', () => {
  const before = state({
    snake: [{ x: 2, y: 2 }, { x: 2, y: 1 }, { x: 1, y: 1 }, { x: 1, y: 2 }, { x: 0, y: 2 }],
    direction: 'left',
  });
  assert.equal(step(before).status, 'lost');
});

test('moving into the vacating tail cell is legal when not growing', () => {
  const before = state({
    snake: [{ x: 1, y: 1 }, { x: 1, y: 2 }, { x: 2, y: 2 }, { x: 2, y: 1 }],
    direction: 'right', food: { x: 5, y: 5 },
  });
  const next = step(before);
  assert.equal(next.status, 'playing');
  assert.deepEqual(next.snake[0], { x: 2, y: 1 });
  assert.equal(next.snake.length, before.snake.length);
});

test('eating the final empty cell wins on a full board', () => {
  const before = state({
    width: 2, height: 2,
    snake: [{ x: 0, y: 0 }, { x: 0, y: 1 }, { x: 1, y: 1 }],
    direction: 'right', food: { x: 1, y: 0 },
  });
  const next = step(before);
  assert.equal(next.status, 'won');
  assert.equal(next.food, null);
  assert.equal(next.score, 1);
  assert.equal(next.snake.length, 4);
});


test('unknown and inherited direction names do not corrupt state', () => {
  const before = state();
  for (const direction of ['toString', '__proto__', 'diagonal', null]) {
    assert.equal(turn(before, direction), before);
  }
});

test('tiny boards remain in bounds and a full one-cell board starts won', () => {
  for (const [width, height] of [[1,1],[1,3],[2,2],[3,1]]) {
    const game = createGame({width,height,rng:()=>0});
    assert.ok(game.snake.every(p=>p.x>=0 && p.x<width && p.y>=0 && p.y<height));
    assert.equal(new Set(game.snake.map(p=>`${p.x},${p.y}`)).size,game.snake.length);
  }
  assert.equal(createGame({width:1,height:1}).status,'won');
});

test('dimensions are validated and injected RNG gives repeatable food', () => {
  for (const width of [0,-1,1.5,NaN]) assert.throws(()=>createGame({width}),RangeError);
  assert.deepEqual(createGame({rng:()=>0.25}),createGame({rng:()=>0.25}));
});
