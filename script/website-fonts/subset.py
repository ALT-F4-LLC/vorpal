"""Subset the docs site fonts into website/src/fonts/.

Run once, offline, when the fonts or the character set change; the build
only reads the committed WOFF2 files, so it needs nothing beyond Bun.

    npm pack @fontsource-variable/instrument-sans@5.3.0 @fontsource-variable/jetbrains-mono@5.3.0
    # unpack each .tgz into <src>/<name-without-.tgz>/
    python3 -m pip install fonttools brotli
    python3 script/website-fonts/subset.py <src> website/src/fonts

Keeps Basic Latin, Latin-1 and the punctuation the docs use (dashes, curly
quotes, bullet, ellipsis, arrows, minus), the wght axis only (JetBrains Mono
limited to 400-700), no hinting. Instrument Sans keeps kern, liga and ss01;
JetBrains Mono drops its ligatures (the site turns them off).
"""
import io, os, sys
from fontTools.ttLib import TTFont
from fontTools.varLib import instancer
from fontTools import subset
U = "U+0020-007E,U+00A0-00FF,U+2013-2014,U+2018-2019,U+201C-201D,U+2022,U+2026,U+2190-2193,U+2212,U+2009"
def uni(s):
    out = []
    for part in s.split(','):
        a = part[2:].split('-'); out += range(int(a[0], 16), int(a[-1], 16) + 1)
    return out
def build(src, dst, features, limit=None):
    f = TTFont(src)
    if limit:
        f = instancer.instantiateVariableFont(f, {"wght": limit})
        b = io.BytesIO(); f.flavor = None; f.save(b); b.seek(0); f = TTFont(b)
    o = subset.Options(); o.flavor = 'woff2'; o.layout_features = features
    o.name_IDs = [0, 1, 2, 3, 4, 5, 6, 13, 14]; o.notdef_outline = True; o.hinting = False; o.desubroutinize = True
    s = subset.Subsetter(o); s.populate(unicodes=uni(U)); s.subset(f)
    f.flavor = 'woff2'; f.save(dst); print(dst, os.path.getsize(dst))
src, out = sys.argv[1], sys.argv[2]
build(f"{src}/fontsource-variable-instrument-sans-5.3.0/package/files/instrument-sans-latin-wght-normal.woff2", f"{out}/instrument-sans-wght.woff2", ["kern", "liga", "ss01", "ccmp", "locl", "mark", "mkmk"])
build(f"{src}/fontsource-variable-jetbrains-mono-5.3.0/package/files/jetbrains-mono-latin-wght-normal.woff2", f"{out}/jetbrains-mono-wght.woff2", ["kern", "ccmp", "locl", "mark", "mkmk"], (400, 700))
