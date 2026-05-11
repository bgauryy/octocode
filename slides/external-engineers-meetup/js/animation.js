/*
 * animation.js — In-slide step animation engine for Octocode Slides
 *
 * Elements marked with [data-step] are "steps" — components revealed
 * sequentially with → / Space / ↓ and hidden in reverse with ← / ↑.
 *
 * HOW IT WORKS
 * ─────────────
 * Steps are revealed before the slide advances to the next slide, and
 * all steps must be hidden before the slide retreats to the previous one.
 *
 * This is achieved by intercepting keydown events in capture phase, before
 * navbridge.js forwards them to the parent (index.html). When a step is
 * consumed, stopImmediatePropagation() prevents navbridge from seeing the
 * key — so the parent does not advance the slide. Once all steps are
 * exhausted in either direction, the key passes through normally and the
 * parent navigates to the next or previous slide.
 *
 * REQUIRED LOADING ORDER (in slide HTML, before </body>):
 *   <script src="../js/animation.js"></script>   ← MUST come first
 *   <script src="../js/navbridge.js"></script>    ← always last
 *
 * USAGE
 * ─────
 * Add data-step="N" to any element (N = 1-based display order):
 *
 *   <li data-step="1">First point</li>
 *   <li data-step="2">Second point</li>
 *   <li data-step="3">Third point</li>
 *
 * If the value is absent or non-numeric, DOM order is used as the fallback.
 * Elements without [data-step] are unaffected and always visible.
 *
 * CUSTOMISING ANIMATIONS
 * ──────────────────────
 * Override the default fade-up in the slide's local <style>:
 *
 *   [data-step]          { opacity: 0; transform: scale(0.95); transition: ... }
 *   [data-step].step-visible { opacity: 1; transform: scale(1); }
 *
 * Add staggered delays using CSS nth-of-type or by setting a custom
 * transition-delay on each step element.
 *
 * OPTIONAL — step indicator dot bar
 * ───────────────────────────────────
 * Add data-step-indicator to the slide root (<div class="slide ...">)
 * to enable a small dot bar that tracks the current step count:
 *
 *   <div class="slide slide--content" data-step-indicator>
 *
 * See animation.md for full documentation.
 */
(function () {
  /* Skip when not embedded in an iframe (e.g. opened standalone for debug). */
  if (window.parent === window) return;

  /* ── Inject default step CSS ──────────────────────────────────────────── */
  var style = document.createElement('style');
  style.textContent = [
    /* Hidden state — all [data-step] elements start invisible */
    '[data-step]{',
    '  opacity:0;',
    '  transform:translateY(14px);',
    '  transition:opacity 320ms cubic-bezier(.4,0,.2,1),',
    '             transform 320ms cubic-bezier(.4,0,.2,1);',
    '  will-change:opacity,transform;',
    '}',
    /* Visible state — added by showNext() */
    '[data-step].step-visible{',
    '  opacity:1;',
    '  transform:translateY(0);',
    '}',
    /* Step indicator dot bar */
    '.step-indicator{',
    '  display:flex;gap:6px;align-items:center;',
    '  position:absolute;bottom:18px;left:50%;transform:translateX(-50%);',
    '  z-index:20;pointer-events:none;',
    '}',
    '.step-dot{',
    '  width:6px;height:6px;border-radius:50%;',
    '  background:rgba(255,255,255,0.25);',
    '  transition:background 200ms ease,transform 200ms ease;',
    '}',
    '.step-dot.is-done{background:rgba(255,255,255,0.85);transform:scale(1.2);}'
  ].join('');
  document.head.appendChild(style);

  /* ── State ──────────────────────────────────────────────────────────── */
  var steps       = [];   /* ordered [data-step] elements */
  var currentStep = 0;    /* how many steps are currently visible */
  var indicator   = null; /* dot bar element, if enabled */

  /* ── Collect and sort steps ─────────────────────────────────────────── */
  function collectSteps() {
    var els = document.querySelectorAll('[data-step]');

    steps = Array.prototype.slice.call(els).sort(function (a, b) {
      var na = parseInt(a.getAttribute('data-step'), 10);
      var nb = parseInt(b.getAttribute('data-step'), 10);
      var aNaN = isNaN(na);
      var bNaN = isNaN(nb);
      /* Both non-numeric: keep DOM order (querySelectorAll is document order) */
      if (aNaN && bNaN) return 0;
      if (aNaN) return 1;
      if (bNaN) return -1;
      return na - nb;
    });

    if (!steps.length) return;

    /* Build optional dot indicator */
    var slideRoot = document.querySelector('[data-step-indicator]');
    if (slideRoot) {
      indicator = document.createElement('div');
      indicator.className = 'step-indicator';
      steps.forEach(function () {
        var dot = document.createElement('span');
        dot.className = 'step-dot';
        indicator.appendChild(dot);
      });
      slideRoot.appendChild(indicator);
    }
  }

  /* ── Show next step ─────────────────────────────────────────────────── */
  function showNext() {
    if (currentStep >= steps.length) return false; /* all shown — pass through */
    steps[currentStep].classList.add('step-visible');
    if (indicator) {
      indicator.children[currentStep].classList.add('is-done');
    }
    currentStep++;
    return true; /* consumed */
  }

  /* ── Hide last visible step ─────────────────────────────────────────── */
  function hideLast() {
    if (currentStep <= 0) return false; /* none shown — pass through */
    currentStep--;
    steps[currentStep].classList.remove('step-visible');
    if (indicator) {
      indicator.children[currentStep].classList.remove('is-done');
    }
    return true; /* consumed */
  }

  /* ── Key sets ───────────────────────────────────────────────────────── */
  var FORWARD  = { ArrowRight: 1, ArrowDown: 1, ' ': 1 };
  var BACKWARD = { ArrowLeft: 1, ArrowUp: 1 };

  /* ── Keydown interceptor (capture phase, before navbridge) ──────────── */
  /*
   * Both animation.js and navbridge.js attach to document in capture phase.
   * Because animation.js is loaded (and therefore registered) first, its
   * handler runs before navbridge's. When a step is consumed, calling
   * stopImmediatePropagation() prevents navbridge from seeing the event,
   * so the parent (index.html) does not advance or retreat the slide.
   *
   * When no step is available (all shown going forward, none going back),
   * the key is NOT consumed — it falls through to navbridge, which forwards
   * it to the parent, and normal slide navigation occurs.
   */
  document.addEventListener('keydown', function (e) {
    if (!steps.length) return; /* this slide has no steps — no-op */

    var consumed = false;
    if (FORWARD[e.key])  consumed = showNext();
    if (BACKWARD[e.key]) consumed = hideLast();

    if (consumed) {
      e.stopImmediatePropagation();
      e.preventDefault();
    }
  }, true /* capture phase */);

  /* ── Init ────────────────────────────────────────────────────────────── */
  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', collectSteps);
  } else {
    collectSteps();
  }
})();
