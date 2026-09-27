# Snake

A dependency-free browser game built by six agents using the communication skill.

From this folder, run `python3 -m http.server 8765 --bind 127.0.0.1`, then open http://127.0.0.1:8765.
Run tests with `node --test *.test.mjs`.

- **Start** begins or resumes; **Pause** freezes the game; **Restart** resets and waits for Start.
- Use arrow keys, WASD, or the four direction buttons. One turn is accepted per tick; direct reversal is blocked.
- Eat food to grow and score. Walls and occupied body cells end the game. The vacating tail cell is safe. Filling the board wins.

## Files and ownership

| File | Author |
| --- | --- |
| engine.mjs | Luna 1 — immutable game rules and injectable RNG |
| engine.test.mjs | Luna 2 — engine behavior tests |
| index.html | Haiku CLI 1 — accessible page |
| style.css | Haiku CLI 2 — responsive arcade styling |
| app.mjs | Grok CLI — controls, timer and canvas |
| README.md | Pi Haiku — play guide and QA |

The parent assembled published documents and fixed reviewed integration defects. The engine exports `createGame`, `turn`, and `step`; statuses are `playing`, `lost`, `won`. The page/controller share IDs `board`, `score`, `status`, `start`, `pause`, `restart` and `data-direction` buttons. Pure engine calls are deterministic when given deterministic RNG.

## Checks

Engine tests cover movement, growth, collisions, reversal, tail vacancy, winning, tiny boards, invalid inputs and deterministic RNG. Browser/app checks cover start, pause, restart, direction buffering and control activation. Communication evidence, individual reflections and limitations are recorded in CHECKS.md.
