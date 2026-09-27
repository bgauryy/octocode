// COPPER: deterministic RNG injection keeps the pure engine testable.
const DELTA = Object.freeze({
  up: [0, -1], down: [0, 1], left: [-1, 0], right: [1, 0],
});
const OPPOSITE = Object.freeze({
  up: "down", down: "up", left: "right", right: "left",
});

function randomCell(cells, rng) {
  if (!cells.length) return null;
  const value = Number(rng());
  const unit = Number.isFinite(value) ? Math.max(0, Math.min(value, 1 - Number.EPSILON)) : 0;
  return cells[Math.floor(unit * cells.length)];
}

function emptyCells(state, snake = state.snake) {
  const occupied = new Set(snake.map(({ x, y }) => y * state.width + x));
  const cells = [];
  for (let y = 0; y < state.height; y++) {
    for (let x = 0; x < state.width; x++) {
      if (!occupied.has(y * state.width + x)) cells.push({ x, y });
    }
  }
  return cells;
}

export function createGame({ width = 20, height = 20, rng = Math.random } = {}) {
  if (!Number.isInteger(width) || width <= 0 || !Number.isInteger(height) || height <= 0) {
    throw new RangeError("width and height must be positive integers");
  }
  if (typeof rng !== "function") throw new TypeError("rng must be a function");
  const length = Math.min(3, width);
  const y = Math.floor(height / 2);
  const headX = Math.floor((width - 1) / 2);
  const startX = Math.max(0, headX - length + 1);
  const snake = Array.from({ length }, (_, i) => ({ x: startX + length - 1 - i, y }));
  const state = { width, height, snake, direction: "right", food: null, score: 0, status: "playing" };
  state.food = randomCell(emptyCells(state), rng);
  if (!state.food) state.status = "won";
  return state;
}

export function turn(state, direction) {
  if (!(Object.hasOwn(DELTA, direction)) || state.status !== "playing" || direction === OPPOSITE[state.direction]) return state;
  if (direction === state.direction) return state;
  return { ...state, direction };
}

export function step(state, rng = Math.random) {
  if (state.status !== "playing") return state;
  const [dx, dy] = DELTA[state.direction] || [0, 0];
  const head = state.snake[0];
  const next = { x: head.x + dx, y: head.y + dy };
  const grows = !!state.food && next.x === state.food.x && next.y === state.food.y;
  const tail = state.snake[state.snake.length - 1];
  const hitBody = state.snake.some(({ x, y }) => x === next.x && y === next.y)
    && (grows || next.x !== tail.x || next.y !== tail.y);
  if (next.x < 0 || next.x >= state.width || next.y < 0 || next.y >= state.height || hitBody) {
    return { ...state, status: "lost" };
  }

  const snake = [next, ...state.snake.slice(0, grows ? undefined : -1)];
  const base = { ...state, snake, score: state.score + (grows ? 1 : 0) };
  if (!grows) return base;
  const food = randomCell(emptyCells(base), rng);
  return { ...base, food, status: food ? "playing" : "won" };
}
