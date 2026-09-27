// COPPER DOM bridge: canvas, controls, and one queued direction per tick.
import { createGame, step, turn } from "./engine.mjs";

const KEYS = {
  arrowup: "up",
  arrowdown: "down",
  arrowleft: "left",
  arrowright: "right",
  w: "up",
  a: "left",
  s: "down",
  d: "right",
};

const canvas = document.querySelector("#board");
const scoreEl = document.querySelector("#score");
const statusEl = document.querySelector("#status");
const ctx = canvas.getContext("2d");
canvas.width = 400;
canvas.height = 400;

let game = createGame();
let running = false;
let queued = null;
let timer = 0;

function paint() {
  const cell = canvas.width / game.width;
  ctx.fillStyle = "#111827";
  ctx.fillRect(0, 0, canvas.width, canvas.height);
  if (game.food) {
    ctx.fillStyle = "#f43f5e";
    ctx.fillRect(game.food.x * cell + 1, game.food.y * cell + 1, cell - 2, cell - 2);
  }
  ctx.fillStyle = "#22c55e";
  for (const part of game.snake) {
    ctx.fillRect(part.x * cell + 1, part.y * cell + 1, cell - 2, cell - 2);
  }
  document.querySelector("#pause").disabled = !running;
  document.querySelector("#start").disabled = running;
  statusEl.className = game.status;
  scoreEl.textContent = String(game.score);
  statusEl.textContent = running ? game.status : game.status === "playing" ? "paused" : game.status;
}

function accept(direction) {
  if (!running || queued || game.status !== "playing" || !direction) return;
  const next = turn(game, direction);
  if (next.direction !== game.direction) queued = next.direction;
}

function tick() {
  if (!running) return;
  if (queued) {
    const next = turn(game, queued);
    queued = null;
    game = next;
  }
  game = step(game);
  if (game.status !== "playing") {
    running = false;
    clearInterval(timer);
    timer = 0;
  }
  paint();
}

function play() {
  if (game.status !== "playing") game = createGame();
  queued = null;
  running = true;
  if (!timer) timer = setInterval(tick, 140);
  paint();
}

function pause() {
  running = false;
  clearInterval(timer);
  timer = 0;
  paint();
}

function restart() {
  pause();
  game = createGame();
  queued = null;
  paint();
}

document.querySelector("#start").addEventListener("click", play);
document.querySelector("#pause").addEventListener("click", pause);
document.querySelector("#restart").addEventListener("click", restart);

for (const button of document.querySelectorAll("[data-direction]")) {
  button.addEventListener("click", (event) => {
    event.preventDefault();
    accept(button.dataset.direction);
  });
}

window.addEventListener("keydown", (event) => {
  const direction = KEYS[event.key.toLowerCase()];
  if (!direction) return;
  event.preventDefault();
  accept(direction);
});

paint();
