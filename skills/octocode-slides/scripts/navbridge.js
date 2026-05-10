/*
 * Slide → parent navigation bridge.
 *
 * When a slide iframe has keyboard focus (after the user clicks anywhere
 * inside it), arrow keys fire on the iframe's document, not the parent
 * window. This script forwards those keys to the parent via postMessage
 * so the navigation controller in index.html can keep working.
 *
 * Usage: copy to js/navbridge.js at the deck root.
 * Each slide includes: <script src="../js/navbridge.js"></script>
 */
(function () {
  if (window.parent === window) return; // standalone, not embedded

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

  // Forward mouse activity so the parent HUD wakes back up
  document.addEventListener('mousemove', function () {
    try { window.parent.postMessage({ type: 'octocode-slides:activity' }, '*'); }
    catch (_) {}
  }, { passive: true });
})();
