"""Builds the two animated SVG banners from Inter itself.

The wordmark is traced to outlines, so the banner carries the real typeface no
matter what the viewer has installed. The mascot is embedded as base64 so the
file is self-contained: no external request, no proxy, no broken image.

Every element's base style is its finished state and the animation runs from a
backwards fill, so a renderer that ignores CSS animation (a proxy, a converter,
a screenshot tool) still shows the complete banner.

    uv run --with fonttools --with brotli python tools/make-hero.py
"""

import base64
from pathlib import Path

from fontTools.pens.svgPathPen import SVGPathPen
from fontTools.ttLib import TTFont
from fontTools.varLib.instancer import instantiateVariableFont

ROOT = Path(__file__).resolve().parent.parent
FONT = ROOT / "src" / "assets" / "fonts" / "InterVariable.woff2"
ART = ROOT / "src" / "assets" / "mascot" / "aikowelcome.webp"
OUT = ROOT / "assets"

WORD = "KURO"
SIZE = 104
TRACKING = 0.14  # em, from the identity


def load_glyphs():
    font = TTFont(FONT)
    font = instantiateVariableFont(font, {"wght": 500, "opsz": 32})
    upem = font["head"].unitsPerEm
    cmap = font.getBestCmap()
    glyph_set = font.getGlyphSet()
    hmtx = font["hmtx"]

    glyphs = []
    for char in WORD:
        name = cmap[ord(char)]
        advance, _ = hmtx[name]
        pen = SVGPathPen(glyph_set)
        glyph_set[name].draw(pen)
        glyphs.append({"path": pen.getCommands(), "advance": advance})
    return glyphs, upem


def word_paths(glyphs, upem, x0, baseline):
    scale = SIZE / upem
    spacing = SIZE * TRACKING
    out = []
    x = x0
    for glyph in glyphs:
        out.append({"d": glyph["path"], "x": round(x, 2), "scale": scale, "baseline": baseline})
        x += glyph["advance"] * scale + spacing
    return out, x - spacing


def build(theme):
    if theme == "dark":
        page, text, muted = "#0B0D12", "#E8EAF0", "#A8B0C4"
        rule, sheen, glow, line = "#B8873C", "#E7C169", "#F4DCA4", "#FFFFFF"
    else:
        page, text, muted = "#FFFFFF", "#12141A", "#4A5163"
        rule, sheen, glow, line = "#B8873C", "#C9A253", "#8A5F1F", "#12141A"

    glyphs, upem = load_glyphs()
    letters, width = word_paths(glyphs, upem, 96, 178)

    animated = "".join(
        f'<path class="letter" style="animation-delay:{200 + i * 70}ms" '
        f'transform="translate({g["x"]} {g["baseline"]}) scale({g["scale"]} -{g["scale"]})" '
        f'd="{g["d"]}"/>'
        for i, g in enumerate(letters)
    )
    plain = "".join(
        f'<path transform="translate({g["x"]} {g["baseline"]}) '
        f'scale({g["scale"]} -{g["scale"]})" d="{g["d"]}"/>'
        for g in letters
    )

    art = base64.b64encode(ART.read_bytes()).decode("ascii")

    return f"""<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1200 320" width="1200" height="320" role="img" aria-labelledby="t">
  <title id="t">KURO, a third-party front door for Roblox</title>
  <style>
    .rule {{ animation: draw 620ms cubic-bezier(.2,.8,.2,1) 80ms backwards; transform-origin: 96px 88px; }}
    .divider {{ animation: fade 500ms cubic-bezier(.2,.8,.2,1) 700ms backwards; }}
    .letter {{ fill: {text}; animation: rise 520ms cubic-bezier(.2,.8,.2,1) backwards; }}
    .sub, .meta {{ fill: {muted}; animation: fade 560ms cubic-bezier(.2,.8,.2,1) backwards; }}
    .sub {{ animation-delay: 760ms; }}
    .meta {{ animation-delay: 900ms; }}
    .art {{ animation: arrive 760ms cubic-bezier(.2,.8,.2,1) 420ms backwards; }}
    .sweep {{ animation: sweep 1100ms cubic-bezier(.4,0,.2,1) 1250ms backwards; }}
    @keyframes draw {{ from {{ transform: scaleX(0); }} to {{ transform: scaleX(1); }} }}
    @keyframes rise {{ from {{ opacity: 0; }} to {{ opacity: 1; }} }}
    @keyframes fade {{ from {{ opacity: 0; }} to {{ opacity: 1; }} }}
    @keyframes arrive {{ from {{ opacity: 0; transform: translateX(18px); }} to {{ opacity: 1; transform: translateX(0); }} }}
    @keyframes sweep {{ from {{ transform: translateX(0); }} to {{ transform: translateX(1800px); }} }}
    @media (prefers-reduced-motion: reduce) {{
      .rule, .divider, .letter, .sub, .meta, .art, .sweep {{ animation: none; }}
    }}
  </style>

  <rect width="1200" height="320" fill="{page}"/>

  <rect class="rule" x="96" y="87" width="{round(width * 0.42, 1)}" height="2" fill="{rule}"/>

  <g>{animated}</g>

  <g mask="url(#word)">
    <rect class="sweep" x="-400" y="60" width="320" height="140" fill="url(#sheen)"/>
  </g>

  <text class="sub" x="96" y="232" font-family="Inter, Segoe UI, system-ui, sans-serif" font-size="21">A third-party front door for Roblox.</text>
  <text class="meta" x="96" y="266" font-family="Inter, Segoe UI, system-ui, sans-serif" font-size="14">Mem&#39;s Kuro Launcher</text>

  <rect class="divider" x="812" y="70" width="1" height="180" fill="{line}" fill-opacity="0.10"/>

  <circle cx="1002" cy="168" r="176" fill="url(#halo)"/>

  <image class="art" x="898" y="8" width="207" height="310" href="data:image/webp;base64,{art}"/>

  <defs>
    <mask id="word"><g fill="#fff">{plain}</g></mask>
    <radialGradient id="halo">
      <stop offset="0" stop-color="{sheen}" stop-opacity="0.13"/>
      <stop offset="0.65" stop-color="{sheen}" stop-opacity="0.04"/>
      <stop offset="1" stop-color="{sheen}" stop-opacity="0"/>
    </radialGradient>
    <linearGradient id="sheen" x1="0" x2="1">
      <stop offset="0" stop-color="{sheen}" stop-opacity="0"/>
      <stop offset="0.45" stop-color="{sheen}" stop-opacity="0.5"/>
      <stop offset="0.55" stop-color="{glow}" stop-opacity="0.7"/>
      <stop offset="1" stop-color="{sheen}" stop-opacity="0"/>
    </linearGradient>
  </defs>
</svg>
"""


def main():
    OUT.mkdir(exist_ok=True)
    for theme in ("dark", "light"):
        target = OUT / f"hero-{theme}.svg"
        target.write_text(build(theme), encoding="utf-8")
        print(f"{target.relative_to(ROOT)} {target.stat().st_size} bytes")


if __name__ == "__main__":
    main()
