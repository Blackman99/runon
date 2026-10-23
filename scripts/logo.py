"""Draw the runon logo: a Chadburn engine-order telegraph dial, front view.
Main handle at STOP, the engine-room answer pointer still in AHEAD."""
import math, sys
C = 256
def pt(r, deg):  # deg: 0 = up, clockwise
    a = math.radians(deg)
    return C + r * math.sin(a), C - r * math.cos(a)
def sector(r0, r1, a0, a1, fill):
    x0, y0 = pt(r1, a0); x1, y1 = pt(r1, a1); x2, y2 = pt(r0, a1); x3, y3 = pt(r0, a0)
    big = 1 if (a1 - a0) % 360 > 180 else 0
    return (f'<path d="M{x0:.2f} {y0:.2f}A{r1} {r1} 0 {big} 1 {x1:.2f} {y1:.2f}'
            f'L{x2:.2f} {y2:.2f}A{r0} {r0} 0 {big} 0 {x3:.2f} {y3:.2f}Z" fill="{fill}"/>')
def line(r0, r1, deg, w, col):
    x0, y0 = pt(r0, deg); x1, y1 = pt(r1, deg)
    return f'<line x1="{x0:.2f}" y1="{y0:.2f}" x2="{x1:.2f}" y2="{y1:.2f}" stroke="{col}" stroke-width="{w}" stroke-linecap="butt"/>'
def pointer(deg, r, base, tail, col):
    tip = pt(r, deg); l = pt(base / 2, deg - 90); rr = pt(base / 2, deg + 90); t = pt(-tail, deg)
    return (f'<path d="M{tip[0]:.2f} {tip[1]:.2f}L{rr[0]:.2f} {rr[1]:.2f}L{t[0]:.2f} {t[1]:.2f}'
            f'L{l[0]:.2f} {l[1]:.2f}Z" fill="{col}"/>')
def full():
    o = ['<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512" role="img" aria-label="runon">']
    o.append(f'<circle cx="{C}" cy="{C}" r="244" fill="#1e3326"/>')
    o.append(f'<circle cx="{C}" cy="{C}" r="230" fill="#2b4a36"/>')
    o.append(f'<circle cx="{C}" cy="{C}" r="214" fill="#1e3326"/>')
    for i in range(12):
        x, y = pt(222, i * 30 + 15)
        o.append(f'<circle cx="{x:.2f}" cy="{y:.2f}" r="5" fill="#6d7f72"/>')
    o.append(f'<circle cx="{C}" cy="{C}" r="204" fill="#f4f6ef"/>')
    # AHEAD (left, bottle green, 3 speeds), ASTERN (right, vermilion, 3 speeds)
    for k, (a0, a1) in enumerate([(-150, -104), (-104, -58), (-58, -14)]):
        o.append(sector(112, 192, a0, a1, "#1e3326" if k != 1 else "#2b4a36"))
    for k, (a0, a1) in enumerate([(14, 58), (58, 104), (104, 150)]):
        o.append(sector(112, 192, a0, a1, "#e2492f" if k != 1 else "#c93d25"))
    # STOP band at the top stays enamel white, edged by two ink rules
    for d in (-14, 14):
        o.append(line(104, 200, d, 3, "#0f1a13"))
    for d in [-150, -127, -104, -81, -58, -36, 36, 58, 81, 104, 127, 150]:
        o.append(line(196, 204, d, 2.5, "#6d7f72"))
    # maker's plate in the empty lower face
    o.append(f'<rect x="{C - 58}" y="{C + 126}" width="116" height="26" rx="4" fill="#1e3326"/>')
    for dx in (-48, 48):
        o.append(f'<circle cx="{C + dx}" cy="{C + 139}" r="3.5" fill="#9aa3a0"/>')
    o.append(f'<rect x="{C - 36}" y="{C + 137}" width="72" height="4" rx="2" fill="#2b4a36"/>')
    # answer pointer (thin, ochre): still says AHEAD
    o.append(pointer(-36, 186, 17, 30, "#d98e04"))
    # order handle (heavy, ink): STOP
    o.append(pointer(0, 196, 34, 46, "#0f1a13"))
    o.append(f'<circle cx="{C}" cy="{C}" r="38" fill="#1e3326"/>')
    o.append(f'<circle cx="{C}" cy="{C}" r="26" fill="#9aa3a0"/>')
    o.append(f'<circle cx="{C - 6}" cy="{C - 6}" r="9" fill="#dfe5e2"/>')
    o.append('</svg>')
    return "\n".join(o)
def fav():
    o = ['<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512">']
    o.append(f'<circle cx="{C}" cy="{C}" r="248" fill="#1e3326"/>')
    o.append(f'<circle cx="{C}" cy="{C}" r="196" fill="#f4f6ef"/>')
    o.append(sector(96, 176, -150, -16, "#2b4a36"))
    o.append(sector(96, 176, 16, 150, "#e2492f"))
    o.append(pointer(-40, 180, 30, 30, "#d98e04"))
    o.append(pointer(0, 186, 60, 56, "#0f1a13"))
    o.append(f'<circle cx="{C}" cy="{C}" r="46" fill="#9aa3a0"/>')
    o.append('</svg>')
    return "\n".join(o)
open("site/img/logo.svg", "w").write(full())
open("site/img/favicon.svg", "w").write(fav())
