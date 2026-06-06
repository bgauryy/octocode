function initPresenter(getActiveFrame, playable, getCurrentIndex) {
  var LS_KEY = 'octocode-slides:timer:' +
    (document.title || 'deck') + ':' + location.pathname;

  var win       = null;
  var elapsed   = 0;
  var timerRef  = null;
  var startTime = null;

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

  var POPUP_HTML =
    '<!DOCTYPE html><html lang="en"><head><meta charset="UTF-8">' +
    '<title>Presenter View</title><style>' +
    '*{box-sizing:border-box;margin:0;padding:0}' +
    'html,body{height:100%;background:#0a0a0a;color:#e0e0e0;' +
      'font-family:system-ui,sans-serif;overflow:hidden}' +
    '#wrap{display:grid;grid-template-rows:auto auto 1fr auto;' +
      'height:100vh;padding:14px 18px;gap:10px}' +

    /* ── Top bar ── */
    '#topbar{display:flex;justify-content:space-between;align-items:center;gap:8px}' +
    '#slide-name{font-size:13px;font-weight:600;color:#c8c8c8;letter-spacing:.04em;' +
      'overflow:hidden;text-overflow:ellipsis;white-space:nowrap;flex:1}' +
    '#timer{font-variant-numeric:tabular-nums;font-size:20px;font-weight:700;' +
      'color:#e0e0e0;letter-spacing:.1em;white-space:nowrap}' +
    '#timer.warn{color:#f59e0b}#timer.over{color:#ef4444}' +

    /* ── Previews row ── */
    '#previews{display:grid;grid-template-columns:1fr 1fr;gap:12px}' +
    '.pv-col{display:flex;flex-direction:column;gap:5px}' +
    '.pv-label{font-size:10px;color:#555;letter-spacing:.08em;text-transform:uppercase}' +
    '.pv-box{position:relative;width:100%;overflow:hidden;border-radius:5px;' +
      'background:#000;border:1px solid #1e1e1e}' +
    '.pv-box iframe{position:absolute;top:0;left:0;width:1280px;height:720px;' +
      'border:none;pointer-events:none;transform-origin:top left}' +
    '.pv-empty{display:grid;place-items:center;font-size:11px;' +
      'color:#444;font-style:italic}' +

    /* ── Notes area ── */
    '#notes-area{display:flex;flex-direction:column;gap:5px;min-height:0}' +
    '#notes{background:#141414;border:1px solid #2a2a2a;border-radius:6px;' +
      'padding:12px 16px;font-size:14px;line-height:1.65;overflow-y:auto;' +
      'white-space:pre-wrap;color:#d0d0d0;flex:1;min-height:0}' +
    '#notes.empty{color:#555;font-style:italic}' +

    /* ── Bottom bar ── */
    '#bottom{display:flex;align-items:center;gap:8px;flex-wrap:wrap;' +
      'border-top:1px solid #1e1e1e;padding-top:8px}' +
    '#jump{display:flex;align-items:center;gap:5px;font-size:12px;color:#666}' +
    '#jump-input{width:44px;background:#1e1e1e;border:1px solid #333;' +
      'border-radius:4px;color:#ccc;font:13px system-ui;padding:3px 6px;' +
      'text-align:center;-moz-appearance:textfield}' +
    '#jump-input::-webkit-inner-spin-button{-webkit-appearance:none}' +
    '#jump-total{color:#444;font-size:11px}' +
    '#spacer{flex:1}' +
    'button{background:#1e1e1e;border:1px solid #333;border-radius:4px;' +
      'color:#aaa;font:12px system-ui;padding:4px 10px;cursor:pointer}' +
    'button:hover{background:#2a2a2a;color:#eee}' +
    '</style></head><body>' +

    '<div id="wrap">' +
    '  <div id="topbar">' +
    '    <span id="slide-name">\u2014</span><span id="timer">00:00</span>' +
    '  </div>' +
    '  <div id="previews">' +
    '    <div class="pv-col">' +
    '      <div class="pv-label">Now showing</div>' +
    '      <div class="pv-box" id="curr-box"><div class="pv-empty" style="height:140px">\u2014</div></div>' +
    '    </div>' +
    '    <div class="pv-col">' +
    '      <div class="pv-label">Up next</div>' +
    '      <div class="pv-box" id="next-box"><div class="pv-empty" style="height:140px">(last slide)</div></div>' +
    '    </div>' +
    '  </div>' +
    '  <div id="notes-area">' +
    '    <div class="pv-label">Speaker notes</div>' +
    '    <div id="notes" class="empty">(no speaker notes)</div>' +
    '  </div>' +
    '  <div id="bottom">' +
    '    <div id="jump">Slide <input id="jump-input" type="number" min="1" value="1">' +
    '      <span id="jump-total">/ \u2014</span></div>' +
    '    <span id="spacer"></span>' +
    '    <button id="btn-reset">Reset timer</button>' +
    '    <button id="btn-close">Close</button>' +
    '  </div>' +
    '</div>' +

    '<script>' +
    'function updateScale(){' +
    '  ["curr","next"].forEach(function(id){' +
    '    var box=document.getElementById(id+"-box");' +
    '    var f=box&&box.querySelector("iframe");' +
    '    if(!box||!f)return;' +
    '    var w=box.clientWidth;var scale=w/1280;' +
    '    box.style.height=Math.round(720*scale)+"px";' +
    '    f.style.transform="scale("+scale+")";' +
    '  });' +
    '}' +
    'window.addEventListener("resize",updateScale);' +

    'window.addEventListener("message",function(e){' +
    '  var d=e.data;if(!d||d.type!=="octocode-slides:presenter-update")return;' +

    '  var nEl=document.getElementById("notes");' +
    '  if(d.notes){nEl.textContent=d.notes;nEl.className=""}' +
    '  else{nEl.textContent="(no speaker notes)";nEl.className="empty"}' +

    '  document.getElementById("slide-name").textContent=d.name||"\u2014";' +

    '  if(typeof d.totalSlides==="number"){' +
    '    document.getElementById("jump-total").textContent="/ "+d.totalSlides;' +
    '    document.getElementById("jump-input").max=d.totalSlides;' +
    '    document.getElementById("jump-input").value=(d.currentIndex||0)+1;' +
    '  }' +

    '  var cBox=document.getElementById("curr-box");' +
    '  if(d.currUrl){' +
    '    var cf=cBox.querySelector("iframe");' +
    '    if(!cf||cf.src!==d.currUrl){' +
    '      cBox.innerHTML=\'<iframe src="\'+d.currUrl+\'"></iframe>\';' +
    '      updateScale();' +
    '    }' +
    '  }' +

    '  var nBox=document.getElementById("next-box");' +
    '  if(d.nextUrl){' +
    '    var nf=nBox.querySelector("iframe");' +
    '    if(!nf||nf.src!==d.nextUrl){' +
    '      nBox.innerHTML=\'<iframe src="\'+d.nextUrl+\'"></iframe>\';' +
    '      updateScale();' +
    '    }' +
    '  }else{' +
    '    nBox.innerHTML=\'<div class="pv-empty" style="height:140px">(last slide)</div>\';' +
    '  }' +
    '});' +

    'document.getElementById("jump-input").addEventListener("change",function(){' +
    '  var n=parseInt(this.value,10);if(!isFinite(n))return;' +
    '  window.opener&&window.opener.postMessage(' +
    '    {type:"octocode-slides:presenter-goto",index:n-1},"*");' +
    '});' +

    'document.getElementById("btn-reset").onclick=function(){' +
    '  window.opener&&window.opener.postMessage(' +
    '    {type:"octocode-slides:presenter-reset"},"*")};' +
    'document.getElementById("btn-close").onclick=function(){window.close()};' +
    '<\/script></body></html>';

  function startTimer() {
    if (!startTime) startTime = Date.now() - elapsed * 1000;
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

  function push() {
    if (!win || win.closed) return;
    var idx      = getCurrentIndex();
    var curr     = playable[idx];
    var next     = playable[idx + 1];
    var frame    = getActiveFrame();
    var deckBase = location.href.replace(/[^\/]*$/, '');

    function doSend() {
      win.postMessage({
        type:         'octocode-slides:presenter-update',
        name:         curr ? curr.name : '\u2014',
        next:         next ? next.name : null,
        notes:        readNotes(frame),
        currUrl:      curr ? deckBase + curr.path : null,
        nextUrl:      next ? deckBase + next.path : null,
        totalSlides:  playable.length,
        currentIndex: idx
      }, '*');
    }

    if (!frame || frameLoaded(frame)) {
      doSend();
    } else {
      frame.addEventListener('load', doSend, { once: true });
    }
  }

  function open() {
    if (win && !win.closed) { win.focus(); return; }
    win = window.open('', 'octocode-presenter',
      'width=720,height=520,resizable=yes,scrollbars=no,toolbar=no,menubar=no');
    if (!win) { console.warn('[presenter] Popup blocked — allow popups for this origin.'); return; }
    win.document.open();
    win.document.write(POPUP_HTML);
    win.document.close();
    startTimer();
    setTimeout(push, 200);
  }

  window.addEventListener('message', function (e) {
    if (!e.data) return;
    if (e.data.type === 'octocode-slides:presenter-reset') {
      elapsed = 0;
      startTime = Date.now();
      try { localStorage.removeItem(LS_KEY); } catch (_) {}
      try { localStorage.setItem(LS_KEY, JSON.stringify({ startTime: startTime })); }
      catch (_) {}
    }
  });

  return {
    open: open,
    push: push,
    
    onSlideChange: push
  };
}
