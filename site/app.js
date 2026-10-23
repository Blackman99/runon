// runon landing: an engine-order telegraph and one cylinder per process.
// Crank-slider kinematics: piston height x = r·cosθ + √(l² − r²·sin²θ).
(() => {
  'use strict';
  const NS = 'http://www.w3.org/2000/svg';
  const reduce = matchMedia('(prefers-reduced-motion: reduce)').matches;
  const $ = (id) => document.getElementById(id);
  const el = (tag, attrs = {}, parent) => {
    const n = document.createElementNS(NS, tag);
    for (const k in attrs) n.setAttribute(k, attrs[k]);
    if (parent) parent.appendChild(n);
    return n;
  };
  const esc = (s) => String(s).replace(/[&<>"]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c]));
  const short = (s) => (s || '').slice(0, 8);

  // ── the telegraph ────────────────────────────────────────────
  const dial = $('dial');
  const pt = (r, deg) => { const a = deg * Math.PI / 180; return [r * Math.sin(a), -r * Math.cos(a)]; };
  const arc = (r0, r1, a0, a1, fill, parent) => {
    const [x0, y0] = pt(r1, a0), [x1, y1] = pt(r1, a1), [x2, y2] = pt(r0, a1), [x3, y3] = pt(r0, a0);
    const big = (a1 - a0) > 180 ? 1 : 0;
    return el('path', { d: `M${x0} ${y0}A${r1} ${r1} 0 ${big} 1 ${x1} ${y1}L${x2} ${y2}A${r0} ${r0} 0 ${big} 0 ${x3} ${y3}Z`, fill }, parent);
  };
  const label = (txt, r, deg, size, fill, parent) => {
    const [x, y] = pt(r, deg);
    let rot = deg; if (deg > 90 || deg < -90) rot = deg + 180;
    const t = el('text', { x, y, 'font-size': size, fill, 'text-anchor': 'middle', 'dominant-baseline': 'central', transform: `rotate(${rot} ${x} ${y})` }, parent);
    t.textContent = txt; return t;
  };
  function drawDial() {
    el('circle', { r: 252, fill: '#1e3326' }, dial);
    el('circle', { r: 238, fill: '#2b4a36' }, dial);
    el('circle', { r: 222, fill: '#1e3326' }, dial);
    for (let i = 0; i < 16; i++) { const [x, y] = pt(230, i * 22.5 + 11.25); el('circle', { cx: x, cy: y, r: 4.5, fill: '#6d7f72' }, dial); }
    el('circle', { r: 212, fill: '#f4f6ef' }, dial);
    const A = [['FULL', -140, -98, '#1e3326'], ['HALF', -98, -56, '#2b4a36'], ['SLOW', -56, -14, '#1e3326']];
    const S = [['SLOW', 14, 56, '#e2492f'], ['HALF', 56, 98, '#c93d25'], ['FULL', 98, 140, '#e2492f']];
    for (const [w, a0, a1, f] of [...A, ...S]) { arc(118, 200, a0, a1, f, dial); label(w, 172, (a0 + a1) / 2, 30, '#f4f6ef', dial); }
    label('AHEAD', 98, -77, 27, '#1e3326', dial);
    label('ASTERN', 98, 77, 27, '#c93d25', dial);
    label('STOP', 166, 0, 34, '#0f1a13', dial);
    const fw = el('text', { x: 0, y: 150, 'font-size': 26, fill: '#0f1a13', 'text-anchor': 'middle' }, dial);
    fw.innerHTML = '<tspan x="0" dy="0">FINISHED WITH</tspan><tspan x="0" dy="27">ENGINES</tspan>';
    for (const d of [-140, -14, 14, 140]) { const [x0, y0] = pt(110, d), [x1, y1] = pt(206, d); el('line', { x1: x0, y1: y0, x2: x1, y2: y1, stroke: '#0f1a13', 'stroke-width': 3 }, dial); }
    for (let d = -140; d <= 140; d += 7) { if (Math.abs(d) < 14) continue; const [x0, y0] = pt(203, d), [x1, y1] = pt(210, d); el('line', { x1: x0, y1: y0, x2: x1, y2: y1, stroke: '#6d7f72', 'stroke-width': 1.6 }, dial); }
    const maker = el('text', { x: 0, y: -56, 'font-size': 17, fill: '#6d7f72', 'text-anchor': 'middle', 'letter-spacing': '0.2em' }, dial);
    maker.textContent = 'RUNON · AGENT ROOM';
    // order: a short heavy shaft plus a wedge riding the bezel, so it never covers the words
    const ord = el('g', {}, dial);
    el('path', { d: 'M0 -100 L15 -8 L0 34 L-15 -8 Z', fill: '#0f1a13' }, ord);
    el('path', { d: 'M0 -203 L15 -246 L-15 -246 Z', fill: '#0f1a13', stroke: '#f4f6ef', 'stroke-width': 3, 'stroke-linejoin': 'round' }, ord);
    // answer: a thin needle, inside the words
    const ans = el('g', {}, dial);
    el('path', { d: 'M0 -114 L7 0 L0 26 L-7 0 Z', fill: '#d98e04' }, ans);
    el('circle', { r: 40, fill: '#1e3326' }, dial);
    el('circle', { r: 27, fill: '#9aa3a0' }, dial);
    el('circle', { cx: -7, cy: -7, r: 9, fill: '#dfe5e2' }, dial);
    return { ans, ord };
  }
  const hands = drawDial();
  const FULL_AHEAD = -119, FWE = -180;
  let ordAngle = FULL_AHEAD, ansAngle = FULL_AHEAD, ansTarget = FULL_AHEAD, ansJitter = 0;
  const setHands = () => {
    hands.ord.setAttribute('transform', `rotate(${ordAngle})`);
    hands.ans.setAttribute('transform', `rotate(${ansAngle + ansJitter})`);
  };
  // the order handle: thrown, overshoots, settles (damped spring)
  function throwHandle(to, done) {
    if (reduce) { ordAngle = to; setHands(); done && done(); return; }
    let v = 0, t0 = performance.now(), last = t0;
    const step = (now) => {
      const dt = Math.max(0, Math.min(0.033, (now - last) / 1000)); last = Math.max(last, now);
      const a = 140 * (to - ordAngle) - 11 * v; v += a * dt; ordAngle += v * dt; setHands();
      if (Math.abs(to - ordAngle) > 0.08 || Math.abs(v) > 0.5) requestAnimationFrame(step); else { ordAngle = to; setHands(); done && done(); }
    };
    requestAnimationFrame(step);
  }
  const ring = () => { if (reduce) return; const t = $('telegraph'); t.classList.remove('ringing'); void t.offsetWidth; t.classList.add('ringing'); };

  // ── cylinders ────────────────────────────────────────────────
  const units = $('units');
  const R = 21, L = 70, CX = 80, CY = 200;
  let cyl = [];
  function cylinderSVG() {
    const s = el('svg', { viewBox: '0 0 160 240', 'aria-hidden': 'true' });
    el('rect', { x: 10, y: 0, width: 140, height: 42, rx: 4, fill: '#2b4a36' }, s);           // head
    el('rect', { x: 26, y: -8, width: 108, height: 12, rx: 3, fill: '#1e3326' }, s);          // rocker cover
    for (const x of [22, 58, 102, 138]) el('circle', { cx: x, cy: 21, r: 4, fill: '#6d7f72' }, s);
    el('rect', { x: 14, y: 40, width: 132, height: 130, fill: '#1e3326' }, s);                // block
    el('rect', { x: 14, y: 40, width: 12, height: 130, fill: '#2b4a36' }, s);                 // lit face
    el('rect', { x: 40, y: 42, width: 80, height: 124, fill: '#0f1a13' }, s);                 // bore cutaway
    el('rect', { x: 40, y: 42, width: 3, height: 124, fill: '#6d7f72' }, s);
    el('rect', { x: 117, y: 42, width: 3, height: 124, fill: '#6d7f72' }, s);
    el('rect', { x: 6, y: 170, width: 148, height: 70, rx: 3, fill: '#2b4a36' }, s);          // crankcase
    el('circle', { cx: CX, cy: CY, r: 34, fill: '#1e3326' }, s);                               // main bearing housing
    const rod = el('line', { stroke: '#dfe5e2', 'stroke-width': 11, 'stroke-linecap': 'round' }, s);
    const web = el('g', {}, s);
    el('circle', { r: 30, fill: '#9aa3a0' }, web);
    el('path', { d: 'M-30 0 A30 30 0 0 0 30 0 Z', fill: '#6d7f72' }, web);                    // counterweight
    el('circle', { cy: -R, r: 8, fill: '#dfe5e2' }, web);                                      // crank pin
    el('circle', { cx: CX, cy: CY, r: 7, fill: '#0f1a13' }, s);
    const piston = el('g', {}, s);
    el('rect', { x: 44, y: -26, width: 72, height: 38, rx: 2, fill: '#9aa3a0' }, piston);
    el('rect', { x: 44, y: -26, width: 72, height: 6, fill: '#dfe5e2' }, piston);
    el('rect', { x: 44, y: -16, width: 72, height: 2.5, fill: '#6d7f72' }, piston);
    el('rect', { x: 44, y: -10, width: 72, height: 2.5, fill: '#6d7f72' }, piston);
    el('circle', { cx: CX, r: 6, fill: '#0f1a13' }, piston);                                           // wrist pin
    s.insertBefore(rod, web); s.appendChild(piston); s.appendChild(rod);
    // keep the rod above the piston skirt but under the pin head
    return { s, rod, web, piston };
  }
  function pose(c) {
    const th = c.theta, sx = R * Math.sin(th), cy = R * Math.cos(th);
    const x = cy + Math.sqrt(L * L - sx * sx);
    const wy = CY - x, px = CX + sx, py = CY - cy;
    c.web.setAttribute('transform', `translate(${CX} ${CY}) rotate(${th * 180 / Math.PI})`);
    c.piston.setAttribute('transform', `translate(0 ${wy})`);
    c.rod.setAttribute('x1', CX); c.rod.setAttribute('y1', wy); c.rod.setAttribute('x2', px); c.rod.setAttribute('y2', py);
  }
  const revs = (cpu) => 0.45 + Math.min(100, cpu) / 100 * 1.8; // turns per second

  const STATE = { out: ['OUTLIVED', 'out'], ret: ['RETURNED', 'ret'], live: ['LIVE', 'live'] };
  function plateHTML(c, phase) {
    const g = c.g;
    if (phase === 'run') return `<p class="st">RUNNING <i>${g.cpu.toFixed(0)}% cpu</i></p><p class="cmd">${esc(g.command)}</p><p class="why">${esc(g.agent)} ${short(g.session)} · ${g.ordinary ? 'tool call in progress' : 'pid ' + g.pids[0]}</p>`;
    if (phase === 'ended') return `<p class="st">EXITED <i>with its call</i></p><p class="cmd">${esc(g.command)}</p><p class="why">returned and ended, nothing to do</p>`;
    if (phase === 'stopped') return `<p class="st">STOPPED <i>TERM</i></p><p class="cmd">${esc(g.command)}</p><p class="why">pid ${g.pids.join(' ')} · gone</p>`;
    const [word] = STATE[c.kind];
    return `<p class="st">${word} <i>${g.cpu.toFixed(0)}% cpu</i></p><p class="cmd">${esc(g.command)}</p><p class="why">${esc(c.why)}</p>`;
  }
  function why(g) {
    if (g.verdict === 'RETURNED') return `codex ${short(g.session)}: the tool call returned, this kept running`;
    if (g.verdict === 'LIVE') return `attached to ${g.agent} pid ${g.harness_pid}`;
    if (g.session_harness_pid) return `${g.agent} ${short(g.session)} still open, but it no longer holds this`;
    return `${g.agent} ${short(g.session)}: session is over`;
  }
  const kindOf = (v) => v === 'OUTLIVED' ? 'out' : v === 'RETURNED' ? 'ret' : 'live';

  function build(list) {
    units.innerHTML = '';
    units.style.setProperty('--n', Math.min(6, Math.max(3, list.length)));
    cyl = list.map((g, i) => {
      const u = document.createElement('div'); u.className = 'unit'; u.setAttribute('role', 'listitem');
      const p = document.createElement('div'); p.className = 'plate';
      const parts = cylinderSVG();
      u.append(p, parts.s); units.appendChild(u);
      const c = { g, plate: p, ...parts, theta: i * 2.1, w: revs(g.cpu) * 2 * Math.PI, target: revs(g.cpu) * 2 * Math.PI,
        kind: g.ordinary ? null : kindOf(g.verdict), why: g.ordinary ? '' : why(g), stopping: false };
      p.innerHTML = plateHTML(c, 'run'); pose(c); return c;
    });
  }
  let lastT = 0;
  function frame(now) {
    const dt = Math.max(0, Math.min(0.05, (now - (lastT || now)) / 1000)); lastT = Math.max(lastT, now);
    for (const c of cyl) {
      if (c.stopping) c.w *= Math.exp(-2.2 * dt); // friction once the fuel is cut
      if (c.w < 0.04) c.w = 0;
      c.theta += c.w * dt; pose(c);
    }
    if (ansTarget !== null) { ansAngle += (ansTarget - ansAngle) * Math.min(1, dt * 3.5); }
    const spinning = cyl.some((c) => !c.stopping && c.w > 0);
    ansJitter = spinning && Math.abs(ansAngle - FULL_AHEAD) < 3 && ordAngle === FWE ? Math.sin(now / 37) * 1.6 : 0;
    setHands();
    requestAnimationFrame(frame);
  }

  // ── counter drum ────────────────────────────────────────────
  function setCount(n, text) {
    const d = $('drum'), cur = d.firstElementChild;
    $('count-label').textContent = text;
    if (cur && cur.textContent === String(n)) return;
    if (reduce || !cur) { d.innerHTML = `<span>${n}</span>`; return; }
    const nx = document.createElement('span'); nx.textContent = n; d.appendChild(nx);
    requestAnimationFrame(() => { cur.style.transform = 'translateY(-100%)'; nx.style.transform = 'translateY(-100%)'; });
    setTimeout(() => { d.innerHTML = `<span>${n}</span>`; }, 560);
  }
  const strays = () => cyl.filter((c) => c.kind && c.kind !== 'live' && !c.done);
  const later = (ms, f) => { timers.push(setTimeout(f, reduce ? 0 : ms)); };
  let timers = [];

  // ── the story ───────────────────────────────────────────────
  const lever = $('lever'), cmd = $('lever-cmd');
  let stage = 0, simulated = false;
  function story(list, fromFile) {
    timers.forEach(clearTimeout); timers = [];
    simulated = fromFile; stage = 0;
    build(list);
    ordAngle = FULL_AHEAD; ansAngle = FULL_AHEAD; ansTarget = FULL_AHEAD; setHands();
    lever.disabled = true; lever.className = 'lever'; cmd.textContent = 'runon stop';
    setCount(cyl.length, 'running, the agent is busy');
    later(1500, () => {
      ring();
      throwHandle(FWE, () => {
        for (const c of cyl) if (!c.kind) { c.stopping = true; c.plate.classList.add('ended'); c.plate.innerHTML = plateHTML(c, 'ended'); }
        cyl.filter((c) => c.kind).forEach((c, i) => later(500 + i * 260, () => {
          c.plate.classList.add(STATE[c.kind][1], 'flip'); c.plate.innerHTML = plateHTML(c, 'k');
          later(380, () => c.plate.classList.remove('flip'));
        }));
        later(700, () => {
          const n = strays().length;
          setCount(n, n === 1 ? 'still running after its agent let go' : 'still running after their agent let go');
          if (n) { lever.disabled = false; stage = 1; } else { ansTarget = FWE; }
        });
      });
    });
  }
  function cut(list) {
    list.forEach((c, i) => later(i * 320, () => {
      c.stopping = true; c.done = true;
      later(900, () => { c.plate.className = 'plate stopped'; c.plate.innerHTML = plateHTML(c, 'stopped'); });
    }));
  }
  lever.addEventListener('click', () => {
    if (stage === 1) {
      lever.classList.add('pulled'); lever.disabled = true;
      const out = strays().filter((c) => c.kind === 'out');
      cut(out);
      later(out.length * 320 + 1000, () => {
        const left = strays();
        lever.classList.remove('pulled');
        setCount(left.length, left.length ? 'left alone: RETURNED needs --returned' : 'still running after their agent let go');
        if (left.length) { stage = 2; cmd.textContent = 'runon stop --returned'; lever.classList.add('ret'); lever.disabled = false; }
        else { ansTarget = FWE; stage = 3; cmd.textContent = 'nothing left'; }
      });
    } else if (stage === 2) {
      lever.classList.add('pulled'); lever.disabled = true;
      const r = strays(); cut(r);
      later(r.length * 320 + 1000, () => {
        ansTarget = FWE; stage = 3; ring();
        lever.classList.remove('pulled', 'ret'); cmd.textContent = 'nothing left';
        setCount(0, simulated ? 'stopped here on the page only: run runon stop for real' : 'still running. The answer pointer agrees now.');
      });
    }
  });

  // ── data ────────────────────────────────────────────────────
  const ORDINARY = [
    { ordinary: true, command: 'npm test', agent: 'claude', session: '', pids: [0], cpu: 18 },
    { ordinary: true, command: 'git diff --stat', agent: 'claude', session: '', pids: [0], cpu: 6 },
    { ordinary: true, command: 'rg -n TODO src/', agent: 'codex', session: '', pids: [0], cpu: 32 },
  ];
  let demo = null;
  const mix = (groups) => {
    const g = groups.slice(0, 3), o = ORDINARY.map((x, i) => ({ ...x, session: (g[i % g.length] || {}).session || '', pids: [] }));
    return [o[0], g[0], o[1], g[1], o[2], g[2]].filter(Boolean);
  };
  fetch('demo.json').then((r) => r.json()).then((v) => { demo = v; story(mix(v.groups), false); }).catch(() => {});
  requestAnimationFrame(frame);
  $('replay').addEventListener('click', () => {
    if (demo) { $('note').dataset.mode = ''; story(mix(demo.groups), false); }
  });

  // drop your own `runon --json`
  const drop = $('drop'), file = $('file');
  const load = (f) => f && f.text().then((t) => {
    let v; try { v = JSON.parse(t); } catch { $('note').textContent = 'That file is not JSON. Save it with: runon --all --json > runon.json'; return; }
    const gs = (v.groups || []).filter((g) => g && g.command);
    if (!gs.length) { $('note').textContent = 'No agent-started processes in that file. Nice and quiet.'; return; }
    $('note').textContent = `Your ${gs.length} tree${gs.length > 1 ? 's' : ''} from runon ${esc(v.runon || '')}. Nothing leaves this page, and the lever here cannot signal anything.`;
    story(gs.slice(0, 12).map((g) => ({ ...g, cpu: +g.cpu || 0, pids: g.pids || [g.root_pid] })), true);
  });
  file.addEventListener('change', () => load(file.files[0]));
  for (const ev of ['dragenter', 'dragover']) document.addEventListener(ev, (e) => { e.preventDefault(); drop.classList.add('over'); });
  document.addEventListener('dragleave', (e) => { if (!e.relatedTarget) drop.classList.remove('over'); });
  document.addEventListener('drop', (e) => { e.preventDefault(); drop.classList.remove('over'); load(e.dataTransfer.files[0]); });

  // ── terminal capture, coloured like the real thing ──────────
  fetch('demo.txt').then((r) => r.text()).then((t) => {
    const out = t.replace(/\n+$/, '').split('\n').map((line) => {
      let h = esc(line);
      h = h.replace(/^ (OUTLIVED|RETURNED|LIVE) (\s*)/, (m, w, sp) => `<span class="${w === 'OUTLIVED' ? 'bo' : w === 'RETURNED' ? 'br' : 'bl'}"> ${w} </span>${sp}`);
      h = h.replace(/^runon(\s)/, '<span class="w">runon</span>$1');
      if (/^\s+(scanned|up |Bash|exec_command|these are|they exit|\+ \d)/.test(line)) h = `<span class="d">${h}</span>`;
      h = h.replace(/^(\s{11})(claude|codex|opencode) ([0-9a-f]{8})/, '$1<span class="w">$2 $3</span>');
      return h;
    });
    $('term').innerHTML = out.join('\n');
  }).catch(() => {});
})();
