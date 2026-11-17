"""Builds the two animated SVG banners from Inter itself.

Two hard constraints found by watching the file on GitHub, not locally:

1. An SVG served as a repo image is sandboxed (CSP `default-src 'none'`), so an
   embedded image, even a base64 one, is blocked. The banner is therefore type
   and rules only.
2. CSS animations inside an SVG used as an image did not advance on GitHub, so
   every element whose fill or opacity came from a stylesheet stayed invisible.
   The animation is SMIL instead, and every colour is an attribute.

The wordmark is traced to outlines, so it carries the real typeface no matter
what the viewer has installed. Animate elements start from their finished state
and SMIL moves them backwards, so a renderer without SMIL shows the whole banner.

    uv run --with fonttools --with brotli python tools/make-hero.py
"""

from pathlib import Path

from fontTools.pens.svgPathPen import SVGPathPen
from fontTools.ttLib import TTFont
from fontTools.varLib.instancer import instantiateVariableFont

ROOT = Path(__file__).resolve().parent.parent
FONT = ROOT / "src" / "assets" / "fonts" / "InterVariable.woff2"
OUT = ROOT / "assets"

WORD = "KURO"
SIZE = 104
TRACKING = 0.14  # em, from the identity

FACTS = ["WINDOWS 10 AND 11", "TAURI 2 · RUST · REACT 19", "MIT LICENCE"]


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


def build(theme):
    if theme == "dark":
        page, text, muted = "#0B0D12", "#E8EAF0", "#A8B0C4"
        rule, sheen, glow, line = "#B8873C", "#E7C169", "#F4DCA4", "#FFFFFF"
    else:
        page, text, muted = "#FFFFFF", "#12141A", "#4A5163"
        rule, sheen, glow, line = "#B8873C", "#C9A253", "#8A5F1F", "#12141A"

    glyphs, upem = load_glyphs()
    scale = SIZE / upem
    spacing = SIZE * TRACKING

    letters = []
    x = 96
    for index, glyph in enumerate(glyphs):
        begin = round(0.2 + index * 0.07, 3)
        letters.append(
            f'''<g transform="translate({round(x, 2)} 178) scale({round(scale, 5)} -{round(scale, 5)})">
    <g>
      <animateTransform attributeName="transform" type="translate" values="0 14;0 0" dur="0.5s" begin="{begin}s" fill="freeze"/>
      <path d="{glyph["path"]}" fill="{text}" opacity="1">
        <animate attributeName="opacity" values="0;1" dur="0.5s" begin="{begin}s" fill="freeze"/>
      </path>
    </g>
  </g>'''
        )
        x += glyph["advance"] * scale + spacing
    word_width = x - spacing - 96

    facts = "".join(
        f'<text x="1104" y="{150 + index * 28}" text-anchor="end" font-family="Inter, Segoe UI, system-ui, sans-serif" '
        f'font-size="12" letter-spacing="0.08em" fill="{muted}">{fact}</text>'
        for index, fact in enumerate(FACTS)
    )

    return f"""<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1200 320" width="1200" height="320" role="img" aria-labelledby="t">
  <title id="t">KURO, a third-party front door for Roblox</title>

  <rect width="1200" height="320" fill="{page}"/>

  <rect x="96" y="87" height="2" fill="{rule}">
    <animate attributeName="width" values="0;{round(word_width * 0.42, 1)}" dur="0.62s" begin="0.08s" fill="freeze"/>
  </rect>

  {chr(10).join(letters)}

  <g mask="url(#word)">
    <rect x="-400" y="40" width="320" height="170" fill="url(#sheen)">
      <animateTransform attributeName="transform" type="translate" values="0 0;1800 0" dur="1.1s" begin="1.25s" fill="freeze"/>
    </rect>
  </g>

  <text x="96" y="232" font-family="Inter, Segoe UI, system-ui, sans-serif" font-size="21" fill="{muted}">A third-party front door for Roblox.<animate attributeName="opacity" values="0;1" dur="0.55s" begin="0.75s" fill="freeze"/></text>
  <text x="96" y="266" font-family="Inter, Segoe UI, system-ui, sans-serif" font-size="14" fill="{muted}">Mem&#39;s Kuro Launcher<animate attributeName="opacity" values="0;1" dur="0.55s" begin="0.9s" fill="freeze"/></text>

  <rect x="880" y="108" width="1" height="130" fill="{line}" fill-opacity="0.10"/>

  {facts}

  <defs>
    <mask id="word">
      <g transform="translate(0 0)">{''.join(f'<path transform="translate({round(96 + sum(g["advance"] * scale + spacing for g in glyphs[:i]) if i else 96, 2)} 178) scale({round(scale, 5)} -{round(scale, 5)})" d="{g["path"]}"/>' for i, g in enumerate(glyphs))}</g>
    </mask>
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
