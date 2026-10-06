// Trusted CDP mouse, keyboard, and scroll sequences with configurable timing.
function rand(min, max) { return min + Math.random() * (max - min); }
function randInt(min, max) { return Math.floor(rand(min, max + 1)); }

function bezierPoint(p0, p1, p2, p3, t) {
  const u = 1 - t;
  return {
    x: u*u*u*p0.x + 3*u*u*t*p1.x + 3*u*t*t*p2.x + t*t*t*p3.x,
    y: u*u*u*p0.y + 3*u*u*t*p1.y + 3*u*t*t*p2.y + t*t*t*p3.y,
  };
}

function easeInOut(t) { return t < 0.5 ? 4*t*t*t : 1 - Math.pow(-2*t+2, 3)/2; }

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

/** Bezier-curve mouse movement from (startX, startY) to (endX, endY): array of {method, params, delayMs}. */
export function buildMouseMoveEvents(startX, startY, endX, endY, opts = {}) {
  const {
    minSteps = 8, maxSteps = 40, stepsDivisor = 5, wobbleMax = 3,
    overshootChance = 0.3, overshootPx = [3, 12],
    burstSize = [3, 8], burstPauseMs = [8, 25], stepDelayMs = [2, 8],
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

    events.push({ method: 'Input.dispatchMouseEvent', params: { type: 'mouseMoved', x, y, button: 'none', buttons: 0 }, delayMs: rand(stepDelayMs[0], stepDelayMs[1]) });

    burst++;
    if (burst >= burstMax && i < steps) {
      events[events.length - 1].delayMs += rand(burstPauseMs[0], burstPauseMs[1]);
      burst = 0;
    }
  }

  if (Math.random() < overshootChance) {
    const angle = Math.atan2(endY - startY, endX - startX);
    const d = rand(overshootPx[0], overshootPx[1]);
    events.push({ method: 'Input.dispatchMouseEvent', params: { type: 'mouseMoved', x: Math.round(endX + Math.cos(angle) * d), y: Math.round(endY + Math.sin(angle) * d), button: 'none', buttons: 0 }, delayMs: rand(20, 50) });
    events.push({ method: 'Input.dispatchMouseEvent', params: { type: 'mouseMoved', x: Math.round(endX + (Math.random()-0.5)*4), y: Math.round(endY + (Math.random()-0.5)*4), button: 'none', buttons: 0 }, delayMs: rand(10, 30) });
  }

  return events;
}

/** mouseDown -> wait -> mouseUp at (x, y): array of {method, params, delayMs}. */
export function buildClickEvents(x, y, opts = {}) {
  const { isInput = false, aimDelayMs = isInput ? [60, 180] : [30, 100], holdMs = isInput ? [60, 120] : [30, 80], button = 'left' } = opts;
  return [
    { method: 'Input.dispatchMouseEvent', params: { type: 'mousePressed', x, y, button, buttons: 1, clickCount: 1, modifiers: 0 }, delayMs: rand(aimDelayMs[0], aimDelayMs[1]) },
    { method: 'Input.dispatchMouseEvent', params: { type: 'mouseReleased', x, y, button, buttons: 0, clickCount: 1, modifiers: 0 }, delayMs: rand(holdMs[0], holdMs[1]) },
  ];
}

const NAMED_KEYS = {
  Enter: [13, 'Enter', '\r'], Tab: [9, 'Tab'], Escape: [27, 'Escape'], Backspace: [8, 'Backspace'],
  Delete: [46, 'Delete'], Space: [32, 'Space', ' '], ArrowUp: [38, 'ArrowUp'], ArrowDown: [40, 'ArrowDown'],
  ArrowLeft: [37, 'ArrowLeft'], ArrowRight: [39, 'ArrowRight'], Home: [36, 'Home'], End: [35, 'End'],
  PageUp: [33, 'PageUp'], PageDown: [34, 'PageDown'],
};
const MODIFIERS = { Alt: [1, 18, 'AltLeft'], Control: [2, 17, 'ControlLeft'], Meta: [4, 91, 'MetaLeft'], Shift: [8, 16, 'ShiftLeft'] };

/** CDP key fields for a named key (Enter, ArrowDown…) or one character. */
export function keyDefinition(key) {
  const named = NAMED_KEYS[key === ' ' ? 'Space' : key === '\n' ? 'Enter' : key === '\t' ? 'Tab' : key];
  if (named) return { key: named[1] === 'Space' ? ' ' : named[1], code: named[1], windowsVirtualKeyCode: named[0], text: named[2] };
  if ([...key].length !== 1) throw new Error(`Unknown key ${JSON.stringify(key)}; use a character or one of ${Object.keys(NAMED_KEYS).join(', ')}`);
  const up = key.toUpperCase();
  const code = /[A-Z]/.test(up) && up.length === 1 ? `Key${up}` : /\d/.test(key) ? `Digit${key}` : '';
  const vk = /[A-Z0-9]/.test(up) && up.length === 1 ? up.charCodeAt(0) : 0;
  return { key, code, windowsVirtualKeyCode: vk, text: key };
}

/** Press a key or combo ("Enter", "Control+A", "Shift+Tab"): array of {method, params, delayMs}. */
export function buildKeyPressEvents(combo) {
  const parts = String(combo).split('+').filter(Boolean);
  const main = parts.pop() ?? '';
  const mods = parts.map((m) => { const d = MODIFIERS[m === 'Ctrl' ? 'Control' : m === 'Cmd' ? 'Meta' : m]; if (!d) throw new Error(`Unknown modifier ${m}`); return [m, d]; });
  const modifiers = mods.reduce((s, [, d]) => s | d[0], 0);
  const def = keyDefinition(main);
  const printable = def.text && !(modifiers & (1 | 2 | 4));
  const ev = (type, p, delayMs = 15) => ({ method: 'Input.dispatchKeyEvent', params: { type, modifiers, ...p }, delayMs });
  return [
    ...mods.map(([m, d]) => ev('rawKeyDown', { key: m, code: d[2], windowsVirtualKeyCode: d[1] })),
    ev(printable ? 'keyDown' : 'rawKeyDown', { key: def.key, code: def.code, windowsVirtualKeyCode: def.windowsVirtualKeyCode, ...(printable ? { text: def.text, unmodifiedText: def.text } : {}) }, 40),
    ev('keyUp', { key: def.key, code: def.code, windowsVirtualKeyCode: def.windowsVirtualKeyCode }),
    ...mods.reverse().map(([m, d]) => ev('keyUp', { key: m, code: d[2], windowsVirtualKeyCode: d[1] })),
  ];
}

function charKeyEvents(ch, delayMs) {
  let def;
  try { def = keyDefinition(ch); } catch { def = null; }
  // Characters with no key mapping (emoji, combining marks) still need text input.
  if (!def) return [{ method: 'Input.insertText', params: { text: ch }, delayMs }];
  return [
    { method: 'Input.dispatchKeyEvent', params: { type: 'keyDown', key: def.key, code: def.code, windowsVirtualKeyCode: def.windowsVirtualKeyCode, text: def.text, unmodifiedText: def.text }, delayMs: 10 },
    { method: 'Input.dispatchKeyEvent', params: { type: 'keyUp', key: def.key, code: def.code, windowsVirtualKeyCode: def.windowsVirtualKeyCode }, delayMs },
  ];
}

/** WPM-paced typing with occasional typo+backspace correction: array of {method, params, delayMs}. */
export function buildTypingEvents(text, opts = {}) {
  const { wpmBase = 60, wpmVariance = 20, mistakeChance = 0.02, burstSize = [3, 7], burstPauseMs = [100, 300] } = opts;

  const msPerChar = () => {
    const wpm = wpmBase + rand(-wpmVariance, wpmVariance);
    return (60000 / wpm) / 5 * rand(0.7, 1.5);
  };

  const events = [];
  let burst = 0;
  const burstMax = randInt(burstSize[0], burstSize[1]);

  for (const ch of text) {
    if (/[a-z]/i.test(ch) && Math.random() < mistakeChance) {
      const wrongChar = String.fromCharCode(ch.charCodeAt(0) + (ch.toLowerCase() === 'a' ? 1 : -1));
      events.push(...charKeyEvents(wrongChar, msPerChar()));
      events.push(...buildKeyPressEvents('Backspace').map((e) => ({ ...e, delayMs: rand(120, 300) })));
    }
    events.push(...charKeyEvents(ch, msPerChar()));

    burst++;
    if (burst >= burstMax) {
      events[events.length - 1].delayMs += rand(burstPauseMs[0], burstPauseMs[1]);
      burst = 0;
    }
  }

  return events;
}

/** Wheel-based scroll at (x, y) by deltaY: array of {method, params, delayMs}. */
export function buildScrollEvents(x, y, deltaY, opts = {}) {
  const { steps = Math.ceil(Math.abs(deltaY) / 100), stepDelayMs = [30, 80] } = opts;
  const events = [];
  const perStep = deltaY / steps;
  for (let i = 0; i < steps; i++) {
    const jitter = (Math.random() - 0.5) * 20;
    events.push({ method: 'Input.dispatchMouseEvent', params: { type: 'mouseWheel', x, y, deltaX: 0, deltaY: perStep + jitter }, delayMs: rand(stepDelayMs[0], stepDelayMs[1]) });
  }
  return events;
}

/** Move from (fromX, fromY) to (targetX, targetY) then click: combined event sequence. */
export function buildHumanClickSequence(fromX, fromY, targetX, targetY, isInput = false) {
  return [...buildMouseMoveEvents(fromX, fromY, targetX, targetY), ...buildClickEvents(targetX, targetY, { isInput })];
}

/** Click in the middle of an element's bounding rect (from a prior getBoundingClientRect() read), slightly randomized. */
export function buildElementClickSequence(currentMouseX, currentMouseY, elementRect, isInput = false) {
  const targetX = Math.round(elementRect.x + elementRect.width * rand(0.35, 0.65));
  const targetY = Math.round(elementRect.y + elementRect.height * rand(0.35, 0.65));
  return buildHumanClickSequence(currentMouseX, currentMouseY, targetX, targetY, isInput);
}

/** Execute a built event sequence against a live cdp session, awaiting each event's delay. */
export async function runEventSequence(cdp, events) {
  for (const ev of events) {
    await cdp.send(ev.method, ev.params);
    if (ev.delayMs > 0) await new Promise((r) => setTimeout(r, ev.delayMs));
  }
}
