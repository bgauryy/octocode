/**
 * Octocode Slides — Presenter View
 *
 * Press P in the generated deck shell to open the presenter popup.
 *
 * Requires same-origin serving (npx serve works; file:// does not).
 * The default scripts/base.html template already loads and wires this file;
 * custom index.html files should copy that integration instead of inventing
 * a second keyboard handler.
 *
 * Expected call site in index.html:
 *
 *   initPresenter(
 *     () => stage.querySelector('.slide-frame[data-active]'),
 *     playable,
 *     () => current
 *   );
 */
function initPresenter(getActiveFrame, playable, getCurrentIndex) {
  // ── localStorage key — scoped to this deck (title + pathname) ───────────
  // Survives page refreshes on localhost so the timer keeps running while
  // you edit slides. Cleared on "Reset timer" or when the deck changes.
  var LS_KEY = 'octocode-slides:timer:' +
    (document.title || 'deck') + ':' + location.pathname;

  var win = null;
  var elapsed = 0;
  var timerRef = null;
  var startTime = null;

  // ── Restore timer across page refreshes ─────────────────────────────────
  (function restoreTimer() {
    try {
      var saved = localStorage.getItem(LS_KEY);
      if (!saved) return;
      var data = JSON.parse(saved);
      if (data && data.startTime) {
        startTime = data.startTime;
        elapsed = Math.floor((Date.now() - startTime) / 1000);
        if (elapsed < 0) elapsed = 0;
      }
    } catch (_) {}
  })();

  // ── Popup HTML ──────────────────────────────────────────────────────────
  var POPUP_HTML = '<!DOCTYPE html><html lang="en"><head><meta charset="UTF-8">' +
    '<title>Presenter View</title><style>' +
    'html,body{margin:0;background:#0a0a0a;color:#e0e0e0;font-family:system-ui,sans-serif;height:100%}' +
    '#wrap{display:grid;grid-template-rows:auto 1fr auto;height:100%;padding:20px;gap:12px;box-sizing:border-box}' +
    '#meta{display:flex;justify-content:space-between;align-items:center;font-size:13px;color:#777}' +
    '#slide-name{font-weight:600;color:#c8c8c8;letter-spacing:.04em}' +
    '#timer{font-variant-numeric:tabular-nums;font-size:22px;font-weight:700;color:#e0e0e0;letter-spacing:.1em}' +
    '#timer.warn{color:#f59e0b}#timer.over{color:#ef4444}' +
    '#notes{background:#141414;border:1px solid #2a2a2a;border-radius:8px;padding:18px 22px;' +
    'font-size:16px;line-height:1.7;overflow-y:auto;white-space:pre-wrap;color:#d0d0d0}' +
    '#notes.empty{color:#555;font-style:italic}' +
    '#next{font-size:12px;color:#555;border-top:1px solid #1e1e1e;padding-top:10px}' +
    '#next span{color:#888}' +
    '#actions{display:flex;gap:8px;margin-top:4px}' +
    'button{background:#1e1e1e;border:1px solid #333;border-radius:4px;color:#aaa;' +
    'font:12px system-ui;padding:4px 10px;cursor:pointer}' +
    'button:hover{background:#2a2a2a;color:#eee}' +
    '</style></head><body>' +
    '<div id="wrap">' +
    '  <div id="meta"><span id="slide-name">—</span><span id="timer">00:00</span></div>' +
    '  <div id="notes" class="empty">(no speaker notes)</div>' +
    '  <div id="next">Next: <span id="next-name">—</span>' +
    '    <div id="actions">' +
    '      <button id="btn-reset">Reset timer</button>' +
    '      <button id="btn-close">Close</button>' +
    '    </div>' +
    '  </div>' +
    '</div>' +
    '<script>' +
    'window.addEventListener("message",function(e){' +
    '  var d=e.data; if(!d||d.type!=="octocode-slides:presenter-update")return;' +
    '  var nEl=document.getElementById("notes");' +
    '  if(d.notes){nEl.textContent=d.notes;nEl.className=""}' +
    '  else{nEl.textContent="(no speaker notes)";nEl.className="empty"}' +
    '  document.getElementById("slide-name").textContent=d.name||"—";' +
    '  document.getElementById("next-name").textContent=d.next||"(last slide)";' +
    '});' +
    'document.getElementById("btn-reset").onclick=function(){window.opener&&window.opener.postMessage({type:"octocode-slides:presenter-reset"},"*")};' +
    'document.getElementById("btn-close").onclick=function(){window.close()};' +
    '<\/script></body></html>';

  // ── Timer ───────────────────────────────────────────────────────────────
  function startTimer() {
    if (!startTime) startTime = Date.now() - elapsed * 1000;
    // Persist start time so a page refresh doesn't reset the clock
    try { localStorage.setItem(LS_KEY, JSON.stringify({ startTime: startTime })); }
    catch (_) {}

    clearInterval(timerRef);
    timerRef = setInterval(function () {
      if (!win || win.closed) { clearInterval(timerRef); return; }
      elapsed = Math.floor((Date.now() - startTime) / 1000);
      var m = Math.floor(elapsed / 60);
      var s = elapsed % 60;
      var str = String(m).padStart(2, '0') + ':' + String(s).padStart(2, '0');
      var timerEl = win.document.getElementById('timer');
      if (!timerEl) return;
      timerEl.textContent = str;
      timerEl.className = elapsed > 3600 ? 'over' : elapsed > 2700 ? 'warn' : '';
    }, 1000);
  }

  // ── Read speaker notes from active iframe ───────────────────────────────
  function readNotes(frame) {
    try {
      var el = frame && frame.contentDocument &&
               frame.contentDocument.querySelector('.speaker-notes');
      if (!el) return '';
      return (el.textContent || el.innerText || '').trim();
    } catch (_) {
      return '(speaker notes unavailable — serve from same origin: npx serve .)';
    }
  }

  function frameLoaded(frame) {
    try {
      return !!(frame && frame.contentDocument &&
        frame.contentDocument.readyState === 'complete');
    } catch (_) {
      return false;
    }
  }

  // ── Push update to popup ────────────────────────────────────────────────
  function push() {
    if (!win || win.closed) return;
    var idx  = getCurrentIndex();
    var curr = playable[idx];
    var next = playable[idx + 1];
    var frame = getActiveFrame();

    // Wait for iframe content to finish loading before reading notes.
    function doSend() {
      win.postMessage({
        type: 'octocode-slides:presenter-update',
        name: curr ? curr.name : '—',
        next: next ? next.name : null,
        notes: readNotes(frame)
      }, '*');
    }

    if (!frame || frameLoaded(frame)) {
      doSend();
    } else {
      frame.addEventListener('load', doSend, { once: true });
    }
  }

  // ── Open popup ──────────────────────────────────────────────────────────
  function open() {
    if (win && !win.closed) { win.focus(); return; }
    win = window.open('', 'octocode-presenter',
      'width=560,height=420,resizable=yes,scrollbars=no,toolbar=no,menubar=no');
    if (!win) { console.warn('[presenter] Popup blocked — allow popups for this origin.'); return; }
    win.document.open();
    win.document.write(POPUP_HTML);
    win.document.close();
    // startTimer picks up startTime from localStorage restore if already running
    startTimer();
    setTimeout(push, 200); // let popup render first
  }

  // ── Listen for reset from popup ─────────────────────────────────────────
  window.addEventListener('message', function (e) {
    if (e.data && e.data.type === 'octocode-slides:presenter-reset') {
      elapsed = 0;
      startTime = Date.now();
      // Clear persisted timer so a refresh after reset starts from 00:00
      try { localStorage.removeItem(LS_KEY); } catch (_) {}
      // Re-persist the fresh start time
      try { localStorage.setItem(LS_KEY, JSON.stringify({ startTime: startTime })); }
      catch (_) {}
    }
  });

  // ── Slide change hook ───────────────────────────────────────────────────
  // Call push() whenever the slide changes.
  // index.html should call presenter.onSlideChange() after go().
  return {
    open: open,
    push: push,
    /** Call this after go() in index.html to update the popup. */
    onSlideChange: push
  };
}
