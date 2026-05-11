/*
 * Slide -> parent navigation bridge.
 *
 * When a slide iframe has keyboard focus (after the user clicks anywhere
 * inside it), arrow keys fire on the iframe's document, not the parent
 * window. This script forwards those keys to the parent via postMessage
 * so the navigation controller in index.html can keep working.
 *
 * Animation lifecycle
 * ───────────────────
 * All iframes load upfront (before the user sees them), so CSS @keyframes
 * animations would finish before the slide is ever displayed. To fix this:
 *
 *   1. At load time we inject a freeze style that pauses all CSS animations
 *      and pause SVG timelines.
 *   2. When the parent sends octocode-slides:activate (in go() and on
 *      iframe load), we remove the freeze and replay every animation from
 *      the start so the full sequence runs on first view.
 */
(function () {
  if (window.parent === window) return; // standalone, not embedded

  // ── Animation freeze ──────────────────────────────────────────────────────
  // Injected immediately (before DOMContentLoaded) so CSS animations never
  // get a chance to tick before the slide becomes visible.
  var freezeStyle = document.createElement('style');
  freezeStyle.textContent = '*, *::before, *::after { animation-play-state: paused !important; }';
  (document.head || document.documentElement).appendChild(freezeStyle);

  function pauseSVGs() {
    document.querySelectorAll('svg').forEach(function (svg) {
      try { svg.pauseAnimations(); } catch (_) {}
    });
  }

  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', pauseSVGs);
  } else {
    pauseSVGs();
  }

  function activateAnimations() {
    // 1. Remove the CSS animation freeze so animations are no longer paused.
    if (freezeStyle.parentNode) freezeStyle.parentNode.removeChild(freezeStyle);

    // 2. Restart every CSS animation from the beginning (handles
    //    animation-fill-mode: forwards correctly — cancel() resets to
    //    pre-animation state, play() restarts including animation-delay).
    requestAnimationFrame(function () {
      try {
        document.getAnimations().forEach(function (anim) {
          try { anim.cancel(); anim.play(); } catch (_) {}
        });
      } catch (_) {}

      // 3. Reset and unpause SVG SMIL timelines.
      document.querySelectorAll('svg').forEach(function (svg) {
        try { svg.setCurrentTime(0); svg.unpauseAnimations(); } catch (_) {}
      });
    });
  }

  // ── Navigation bridge ─────────────────────────────────────────────────────

  var NAV_KEYS = {
    ArrowLeft: 1, ArrowRight: 1, ArrowUp: 1, ArrowDown: 1,
    PageUp: 1, PageDown: 1, Home: 1, End: 1,
    ' ': 1, g: 1, G: 1, f: 1, F: 1
  };

  function isTypingTarget(el) {
    if (!el) return false;
    var tag = (el.tagName || '').toLowerCase();
    if (tag === 'input' || tag === 'textarea' || tag === 'select') return true;
    return !!el.isContentEditable;
  }

  function hasTextSelection() {
    try {
      var sel = window.getSelection && window.getSelection();
      return !!(sel && String(sel).length > 0);
    } catch (_) { return false; }
  }

  function send(key) {
    try {
      window.parent.postMessage(
        { type: 'octocode-slides:nav', key: key },
        '*'
      );
    } catch (_) { /* ignore cross-origin failures */ }
  }

  document.addEventListener('keydown', function (e) {
    if (isTypingTarget(e.target)) return;
    if (!NAV_KEYS[e.key]) return;
    if (e.key === ' ' && hasTextSelection()) return;
    send(e.key);
    e.preventDefault();
  }, true);

  // Parent forwards nav keys here when the parent window has focus (not the
  // iframe). Re-dispatch as a real keydown so animation.js (loaded before
  // navbridge in capture phase) can intercept and consume a step first.
  // If animation.js has no step to consume, it lets the event through and
  // the keydown handler above calls send() to post back to the parent.
  //
  // Also handles octocode-slides:activate — starts (or restarts) all
  // CSS animations and SVG timelines from the beginning.
  window.addEventListener('message', function (e) {
    if (!e.data || typeof e.data !== 'object') return;

    if (e.data.type === 'octocode-slides:activate') {
      activateAnimations();
      return;
    }

    if (e.data.type === 'octocode-slides:key' && e.data.key) {
      document.dispatchEvent(new KeyboardEvent('keydown', {
        key: e.data.key,
        bubbles: true,
        cancelable: true
      }));
    }
  });

  // Forward clicks anywhere on the slide as a "wake the HUD" hint so the
  // top-right status pill reappears when the user interacts with a slide.
  document.addEventListener('mousemove', function () {
    try { window.parent.postMessage({ type: 'octocode-slides:activity' }, '*'); }
    catch (_) {}
  }, { passive: true });
})();
