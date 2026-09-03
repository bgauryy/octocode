/**
 * human-input.mjs
 *
 * Human-like mouse, keyboard, and scroll via CDP Input domain.
 * All operations use `scheme:"raw"` through chromeDebug.
 *
 * Usage: import helpers and call with the chromeDebug raw scheme.
 */

// ── Timing helpers ─────────────────────────────────────────────────────────────

/** Random float in [min, max] */
function rand(min, max) {
  return min + Math.random() * (max - min);
}

/** Random int in [min, max] */
function randInt(min, max) {
  return Math.floor(rand(min, max + 1));
}

/** Sleep for ms milliseconds */
function sleep(ms) {
  return new Promise(r => setTimeout(r, ms));
}

// ── Bezier curve math ──────────────────────────────────────────────────────────

function bezierPoint(p0, p1, p2, p3, t) {
  const u = 1 - t;
  return {
    x: u*u*u*p0.x + 3*u*u*t*p1.x + 3*u*t*t*p2.x + t*t*t*p3.x,
    y: u*u*u*p0.y + 3*u*u*t*p1.y + 3*u*t*t*p2.y + t*t*t*p3.y,
  };
}

function easeInOut(t) {
  return t < 0.5 ? 4*t*t*t : 1 - Math.pow(-2*t+2, 3)/2;
}

function randomControlPoints(start, end) {
  const dx = end.x - start.x;
  const dy = end.y - start.y;
  const dist = Math.hypot(dx, dy) || 1;
  const px = -dy / dist;
  const py = dx / dist;
  const b1 = rand(-0.3, 0.3) * dist;
  const b2 = rand(-0.3, 0.3) * dist;
  return [
    { x: start.x + dx*0.25 + px*b1, y: start.y + dy*0.25 + py*b1 },
    { x: start.x + dx*0.75 + px*b2, y: start.y + dy*0.75 + py*b2 },
  ];
}

// ── CDP send helper ─────────────────────────────────────────────────────────────

/**
 * Send a CDP Input event via chromeDebug scheme:"raw".
 * Caller must provide a `sendRaw(method, params)` function that calls chromeDebug.
 */

// ── Mouse movement ──────────────────────────────────────────────────────────────

/**
 * Build a sequence of CDP Input.dispatchMouseEvent params for human-like movement
 * from (startX, startY) to (endX, endY).
 *
 * Returns array of {method, params} objects to execute in order via scheme:"raw".
 */
export function buildMouseMoveEvents(startX, startY, endX, endY, opts = {}) {
  const {
    minSteps = 8,
    maxSteps = 40,
    stepsDivisor = 5,
    wobbleMax = 3,
    overshootChance = 0.3,
    overshootPx = [3, 12],
    burstSize = [3, 8],
    burstPauseMs = [8, 25],
    stepDelayMs = [2, 8],
  } = opts;

  const dist = Math.hypot(endX - startX, endY - startY);
  if (dist < 1) return [];

  const steps = Math.max(minSteps, Math.min(maxSteps, Math.round(dist / stepsDivisor)));
  const start = { x: startX, y: startY };
  const end = { x: endX, y: endY };
  const [cp1, cp2] = randomControlPoints(start, end);

  const events = [];
  let burst = 0;
  const burstMax = randInt(burstSize[0], burstSize[1]);

  for (let i = 0; i <= steps; i++) {
    const t = easeInOut(i / steps);
    const pt = bezierPoint(start, cp1, cp2, end, t);
    const wobble = Math.sin(Math.PI * (i / steps)) * wobbleMax;
    const x = Math.round(pt.x + (Math.random() - 0.5) * 2 * wobble);
    const y = Math.round(pt.y + (Math.random() - 0.5) * 2 * wobble);

    events.push({
      method: 'Input.dispatchMouseEvent',
      params: { type: 'mouseMoved', x, y, button: 'none', buttons: 0 },
      delayMs: rand(stepDelayMs[0], stepDelayMs[1]),
    });

    burst++;
    if (burst >= burstMax && i < steps) {
      events[events.length - 1].delayMs += rand(burstPauseMs[0], burstPauseMs[1]);
      burst = 0;
    }
  }

  // Overshoot + correct
  if (Math.random() < overshootChance) {
    const angle = Math.atan2(endY - startY, endX - startX);
    const d = rand(overshootPx[0], overshootPx[1]);
    const ovX = Math.round(endX + Math.cos(angle) * d);
    const ovY = Math.round(endY + Math.sin(angle) * d);
    events.push({
      method: 'Input.dispatchMouseEvent',
      params: { type: 'mouseMoved', x: ovX, y: ovY, button: 'none', buttons: 0 },
      delayMs: rand(20, 50),
    });
    // Correct back
    events.push({
      method: 'Input.dispatchMouseEvent',
      params: { type: 'mouseMoved', x: Math.round(endX + (Math.random()-0.5)*4), y: Math.round(endY + (Math.random()-0.5)*4), button: 'none', buttons: 0 },
      delayMs: rand(10, 30),
    });
  }

  return events;
}

/**
 * Build click events at (x, y): mouseDown → wait → mouseUp → click.
 */
export function buildClickEvents(x, y, opts = {}) {
  const {
    isInput = false,
    aimDelayMs = isInput ? [60, 180] : [30, 100],
    holdMs = isInput ? [60, 120] : [30, 80],
    button = 'left',
  } = opts;

  return [
    {
      method: 'Input.dispatchMouseEvent',
      params: { type: 'mousePressed', x, y, button, buttons: 1, clickCount: 1, modifiers: 0 },
      delayMs: rand(aimDelayMs[0], aimDelayMs[1]),
    },
    {
      method: 'Input.dispatchMouseEvent',
      params: { type: 'mouseReleased', x, y, button, buttons: 0, clickCount: 1, modifiers: 0 },
      delayMs: rand(holdMs[0], holdMs[1]),
    },
  ];
}

// ── Typing ──────────────────────────────────────────────────────────────────────

/** Key code map for common characters */
const KEY_MAP = {
  ' ': { code: 'Space', key: ' ', keyCode: 32 },
  '\n': { code: 'Enter', key: 'Enter', keyCode: 13 },
  '\t': { code: 'Tab', key: 'Tab', keyCode: 9 },
};

/**
 * Build human-like typing events for a string.
 * Uses Input.insertText for printable chars (most natural, no char-by-char key events needed).
 * Falls back to Input.dispatchKeyEvent for special keys.
 */
export function buildTypingEvents(text, opts = {}) {
  const {
    wpmBase = 60,              // average typing speed (words per minute)
    wpmVariance = 20,          // ±WPM variance
    mistakeChance = 0.02,      // 2% chance of typo + correction per char
    burstSize = [3, 7],        // chars before a pause
    burstPauseMs = [100, 300], // pause between bursts
  } = opts;

  const msPerChar = () => {
    const wpm = wpmBase + rand(-wpmVariance, wpmVariance);
    const msPer5Chars = 60000 / wpm;
    return msPer5Chars / 5 * rand(0.7, 1.5);
  };

  const events = [];
  let burst = 0;
  const burstMax = randInt(burstSize[0], burstSize[1]);

  for (let i = 0; i < text.length; i++) {
    const ch = text[i];
    const special = KEY_MAP[ch];

    if (special) {
      events.push({
        method: 'Input.dispatchKeyEvent',
        params: { type: 'keyDown', code: special.code, key: special.key, windowsVirtualKeyCode: special.keyCode },
        delayMs: msPerChar(),
      });
      events.push({
        method: 'Input.dispatchKeyEvent',
        params: { type: 'keyUp', code: special.code, key: special.key, windowsVirtualKeyCode: special.keyCode },
        delayMs: 20,
      });
    } else {
      // Typo simulation
      if (Math.random() < mistakeChance) {
        const wrongChar = String.fromCharCode(ch.charCodeAt(0) + (Math.random() < 0.5 ? 1 : -1));
        events.push({ method: 'Input.insertText', params: { text: wrongChar }, delayMs: msPerChar() });
        // Pause then backspace
        events.push({ method: 'Input.dispatchKeyEvent', params: { type: 'keyDown', code: 'Backspace', key: 'Backspace', windowsVirtualKeyCode: 8 }, delayMs: rand(200, 600) });
        events.push({ method: 'Input.dispatchKeyEvent', params: { type: 'keyUp', code: 'Backspace', key: 'Backspace', windowsVirtualKeyCode: 8 }, delayMs: 30 });
      }
      events.push({ method: 'Input.insertText', params: { text: ch }, delayMs: msPerChar() });
    }

    burst++;
    if (burst >= burstMax) {
      events[events.length - 1].delayMs += rand(burstPauseMs[0], burstPauseMs[1]);
      burst = 0;
    }
  }

  return events;
}

// ── Scroll ──────────────────────────────────────────────────────────────────────

/**
 * Build human-like scroll events (wheel-based).
 */
export function buildScrollEvents(x, y, deltaY, opts = {}) {
  const {
    steps = Math.ceil(Math.abs(deltaY) / 100),
    stepDelayMs = [30, 80],
  } = opts;

  const events = [];
  const perStep = deltaY / steps;

  for (let i = 0; i < steps; i++) {
    const jitter = (Math.random() - 0.5) * 20;
    events.push({
      method: 'Input.dispatchMouseEvent',
      params: { type: 'mouseWheel', x, y, deltaX: 0, deltaY: perStep + jitter },
      delayMs: rand(stepDelayMs[0], stepDelayMs[1]),
    });
  }

  return events;
}

// ── chromeDebug integration ─────────────────────────────────────────────────────

/**
 * Convert a sequence of input events to chromeDebug raw scheme calls.
 *
 * The browser-agent subagent should call:
 *   for each event in sequence:
 *     chromeDebug scheme:"raw" method:event.method params:event.params port:N
 *     (then wait event.delayMs before next call)
 *
 * Example task for the subagent:
 *   "Move mouse from (100,100) to (500,300) with human-like movement, then click."
 *   The subagent generates events using these helpers and executes each as a raw CDP call.
 */
export function buildHumanClickSequence(fromX, fromY, targetX, targetY, isInput = false) {
  return [
    ...buildMouseMoveEvents(fromX, fromY, targetX, targetY),
    ...buildClickEvents(targetX, targetY, { isInput }),
  ];
}

/**
 * Build a complete "navigate to element + click" sequence.
 * Uses getBoundingClientRect from a prior Runtime.evaluate to get target coords.
 */
export function buildElementClickSequence(currentMouseX, currentMouseY, elementRect, isInput = false) {
  // Click in the middle of the element (slightly randomized)
  const targetX = Math.round(elementRect.x + elementRect.width * rand(0.35, 0.65));
  const targetY = Math.round(elementRect.y + elementRect.height * rand(0.35, 0.65));
  return buildHumanClickSequence(currentMouseX, currentMouseY, targetX, targetY, isInput);
}

// ── Export summary ──────────────────────────────────────────────────────────────

/**
 * Summary for use in browser-agent task descriptions:
 *
 * To click an element with human-like mouse movement:
 *   1. scheme:"raw" method:"Runtime.evaluate" params:{expression:"JSON.stringify(document.querySelector('button').getBoundingClientRect())",returnByValue:true}
 *      → get {x, y, width, height}
 *   2. Build events: buildHumanClickSequence(startX, startY, x + w/2, y + h/2)
 *   3. Execute each event: scheme:"raw" method:event.method params:event.params
 *      Wait event.delayMs between each call.
 *
 * To type with human-like timing:
 *   1. Click the input field (step above)
 *   2. Build events: buildTypingEvents("hello world")
 *   3. Execute each event: scheme:"raw" method:event.method params:event.params
 */
