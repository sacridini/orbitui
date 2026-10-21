"""The orbitui logo: the name in ASCII (figlet "colossal"), coloured across the
visible spectrum, from violet to red, on a dark terminal-like panel.

    pip install pyfiglet
    python tools/build_logo.py [figlet font] > assets/logo.svg

The text stays plain ASCII; the colour is an SVG gradient, since Markdown
cannot colour code blocks.
"""
from xml.sax.saxutils import escape

import pyfiglet

FONT_SIZE = 20
CHAR_W = FONT_SIZE * 0.6  # monospace advance
LINE_H = FONT_SIZE * 1.15
PAD_X, PAD_Y = 36, 30

# Visible spectrum, short to long wavelengths (nm, colour).
SPECTRUM = [
    (380, "#7f00ff"),
    (440, "#2a3cff"),
    (480, "#00a8ff"),
    (500, "#00e5c8"),
    (530, "#28e83c"),
    (570, "#d8f000"),
    (590, "#ffc400"),
    (620, "#ff6a00"),
    (700, "#ff1f3d"),
]


def main(font="colossal"):
    lines = [l.rstrip() for l in pyfiglet.figlet_format("orbitui", font=font).splitlines() if l.strip()]
    cols = max(len(l) for l in lines)
    text_w, text_h = cols * CHAR_W, len(lines) * LINE_H
    width, height = round(text_w + 2 * PAD_X), round(text_h + 2 * PAD_Y)
    lo, hi = SPECTRUM[0][0], SPECTRUM[-1][0]
    stops = "\n".join(
        f'      <stop offset="{(nm - lo) / (hi - lo):.3f}" stop-color="{color}"/>' for nm, color in SPECTRUM
    )
    rows = "\n".join(
        # No-break spaces: renderers collapse runs of plain ones.
        # A fixed length per line keeps the columns aligned in any monospace font.
        f'    <text x="{PAD_X}" y="{PAD_Y + (i + 0.8) * LINE_H:.1f}" textLength="{len(line) * CHAR_W:.1f}" '
        f'lengthAdjust="spacingAndGlyphs">{escape(line).replace(" ", "&#160;")}</text>'
        for i, line in enumerate(lines)
    )
    print(f'''<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}" role="img" aria-label="orbitui">
  <defs>
    <linearGradient id="spectrum" gradientUnits="userSpaceOnUse" x1="{PAD_X}" y1="0" x2="{PAD_X + text_w:.0f}" y2="0">
{stops}
    </linearGradient>
  </defs>
  <rect width="{width}" height="{height}" rx="12" fill="#0d1117"/>
  <g font-family="ui-monospace, SFMono-Regular, Menlo, Consolas, 'Liberation Mono', 'DejaVu Sans Mono', monospace"
     font-size="{FONT_SIZE}" font-weight="700" fill="url(#spectrum)">
{rows}
  </g>
</svg>''')


if __name__ == "__main__":
    import sys

    main(*sys.argv[1:2])
