// share card: the same telegraph as the landing page, frozen at the moment that matters
(() => {
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
  hands.ord.setAttribute('transform', 'rotate(-180)');
  hands.ans.setAttribute('transform', 'rotate(-121)');
})();
