#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""Resolve themes/*.toml the way the game will (extends chain, derived slots), check
that the koi read against every water band, and write the resolved themes into
research/style/preview.html between the THEMES markers.

    python3 research/style/build_preview.py            # check and rebuild the preview
    python3 research/style/build_preview.py ramp water=#4F9BC8 foliage=#5FA048 stone=#C8A878
                                                       # print [palette] lines from seed colors

Exits non-zero when a theme fails the koi check or has an unknown key.
"""

import json
import math
import re
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
THEMES = ROOT / "themes"
PREVIEW = Path(__file__).resolve().parent / "preview.html"

REQUIRED = ["deep", "mid", "shallow", "highlight", "shadow", "stone_light", "stone_dark", "lily_dark",
            "lily_light", "lily_flower", "koi_white", "koi_red", "koi_sumi", "ogon"]
DERIVED = ["outline", "cloud", "asagi_blue", "asagi_red", "food", "turtle_shell",
           "turtle_skin", "ui_text", "ui_dim", "ui_accent"]
TIMES = ["dawn", "morning", "noon", "afternoon", "evening", "dusk", "night"]
ROOT_THEME = "summer-garden"
# Belong to the file they are written in; never inherited.
OWN = ("name", "description", "family", "time", "credit", "hidden")


# Colors. Mixing happens in linear light, like mix3(linear(..)) in water.rs.

def to_lin(hex_):
    h = hex_.lstrip("#")
    c = [int(h[i:i + 2], 16) / 255 for i in (0, 2, 4)]
    return [x / 12.92 if x <= 0.04045 else ((x + 0.055) / 1.055) ** 2.4 for x in c]


def to_hex(lin):
    out = []
    for x in lin:
        x = min(max(x, 0.0), 1.0)
        s = 12.92 * x if x <= 0.0031308 else 1.055 * x ** (1 / 2.4) - 0.055
        out.append(round(s * 255))
    return "#{:02X}{:02X}{:02X}".format(*out)


def mix(a, b, t):
    la, lb = to_lin(a), to_lin(b)
    return to_hex([x + (y - x) * t for x, y in zip(la, lb)])


def scale(a, k):
    return to_hex([x * k for x in to_lin(a)])


def lin_to_oklab(c):
    r, g, b = c
    l = 0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b
    m = 0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b
    s = 0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b
    l, m, s = (math.copysign(abs(v) ** (1 / 3), v) for v in (l, m, s))
    return [0.2104542553 * l + 0.7936177850 * m - 0.0040720468 * s,
            1.9779984951 * l - 2.4285922050 * m + 0.4505937099 * s,
            0.0259040371 * l + 0.7827717662 * m - 0.8086757660 * s]


def oklab_to_lin(lab):
    L, a, b = lab
    l = (L + 0.3963377774 * a + 0.2158037573 * b) ** 3
    m = (L - 0.1055613458 * a - 0.0638541728 * b) ** 3
    s = (L - 0.0894841775 * a - 1.2914855480 * b) ** 3
    return [4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s,
            -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s,
            -0.0041960863 * l - 0.7034186147 * m + 1.7076147010 * s]


def oklch(hex_):
    L, a, b = lin_to_oklab(to_lin(hex_))
    return L, math.hypot(a, b), math.degrees(math.atan2(b, a)) % 360


def from_oklch(L, C, h):
    """Reduce chroma until the color fits sRGB; never clip channels."""
    L = min(max(L, 0.02), 0.98)
    for _ in range(40):
        lin = oklab_to_lin([L, C * math.cos(math.radians(h)), C * math.sin(math.radians(h))])
        if all(-1e-4 <= x <= 1 + 1e-4 for x in lin):
            return to_hex(lin)
        C *= 0.92
    return to_hex(lin)


def turn(h, target, by):
    d = (target - h + 540) % 360 - 180
    return h + max(-by, min(by, d))


def ramp(seed, shift, dark_dl, light_dl):
    """Three hue-shifted steps: darker turns toward blue-violet, lighter toward warm yellow."""
    L, C, h = oklch(seed)
    dark = from_oklch(L - dark_dl, C * 0.85, turn(h, 265, shift))
    light = from_oklch(L + light_dl, C * 0.75, turn(h, 95, shift))
    return dark, seed.upper(), light


def contrast(a, b):
    def lum(h):
        r, g, bl = to_lin(h)
        return 0.2126 * r + 0.7152 * g + 0.0722 * bl
    la, lb = sorted([lum(a), lum(b)], reverse=True)
    return (la + 0.05) / (lb + 0.05)


def delta_e(a, b):
    return math.dist(lin_to_oklab(to_lin(a)), lin_to_oklab(to_lin(b)))


def delta_l(a, b):
    return lin_to_oklab(to_lin(a))[0] - lin_to_oklab(to_lin(b))[0]


# Machado, Oliveira and Fernandes 2009, deuteranopia at severity 1.0, in linear RGB.
# https://www.inf.ufrgs.br/~oliveira/pubs_files/CVD_Simulation/CVD_Simulation.html
DEUTAN = [[0.367322, 0.860646, -0.227968], [0.280085, 0.672501, 0.047413], [-0.011820, 0.042940, 0.968881]]


def delta_e_deutan(a, b):
    sim = lambda h: lin_to_oklab([max(0.0, sum(m * x for m, x in zip(row, to_lin(h)))) for row in DEUTAN])
    return math.dist(sim(a), sim(b))


# Themes

def load(name, seen=()):
    """Merge the extends chain root first. A theme with no `extends` extends summer-garden,
    which is the root and holds every key. Tables merge key by key, lists replace whole.
    The OWN keys and derived palette slots are not inherited."""
    if name in seen:
        sys.exit(f"extends cycle: {' -> '.join(seen + (name,))}")
    raw = tomllib.loads((THEMES / f"{name}.toml").read_text())
    parent = raw.get("extends", ROOT_THEME if name != ROOT_THEME else None)
    resolved = load(parent, seen + (name,)) if parent else {"palette": {}, "light": {}, "style": {}, "scene": {}}
    resolved = json.loads(json.dumps(resolved))
    for key in OWN:
        resolved.pop(key, None)
    # A derived slot set in a parent fits the parent's colors, so a child derives it again.
    for key in DERIVED:
        resolved["palette"].pop(key, None)
    for table in ("palette", "light", "style", "scene"):
        resolved[table].update(raw.get(table, {}))
    for key in OWN:
        if key in raw:
            resolved[key] = raw[key]
    resolved.setdefault("family", name)
    resolved.setdefault("time", "noon")
    resolved.setdefault("credit", "")
    resolved["chain"] = [name] + resolved.get("chain", [])
    return resolved


def unknown_keys(name, root):
    raw = tomllib.loads((THEMES / f"{name}.toml").read_text())
    top = set(OWN) | {"extends", "palette", "light", "style", "scene"}
    bad = [k for k in raw if k not in top]
    for table in ("light", "style", "scene"):
        bad += [f"{table}.{k}" for k in raw.get(table, {}) if k not in root[table]]
    bad += [f"palette.{k}" for k in raw.get("palette", {}) if k not in REQUIRED + DERIVED + ["swatches"]]
    return bad


def derive(p):
    p.setdefault("outline", mix(p["koi_sumi"], p["shadow"], 0.35))
    p.setdefault("cloud", mix(p["koi_white"], p["highlight"], 0.4))
    p.setdefault("asagi_blue", mix(p["koi_white"], p["shadow"], 0.5))
    p.setdefault("asagi_red", mix(p["koi_red"], p["ogon"], 0.3))
    p.setdefault("food", scale(mix(p["stone_light"], p["ogon"], 0.5), 0.75))
    p.setdefault("turtle_shell", scale(mix(p["lily_dark"], p["stone_dark"], 0.5), 0.8))
    p.setdefault("turtle_skin", mix(p["lily_light"], p["stone_dark"], 0.5))
    p.setdefault("ui_text", p["koi_white"])
    p.setdefault("ui_dim", mix(p["shallow"], p["highlight"], 0.5))
    p.setdefault("ui_accent", p["ogon"])
    if not p.get("swatches"):
        p["swatches"] = sorted({p[k].upper() for k in REQUIRED + DERIVED})
    p["swatches"] = [s.upper() for s in p["swatches"]]
    for k in REQUIRED + DERIVED:
        p[k] = p[k].upper()


BANDS = ("shallow", "mid", "deep")
MIN_DL = 0.06        # OKLab lightness gap that reads on its own
MIN_OUTLINE = 2.5    # WCAG contrast of the drawn edge against the water


def edge_color(p, style, koi, band):
    """The color the preview actually draws on the koi edge, per outline mode."""
    mode = style["outline"]
    if mode == "none":
        return None
    if mode == "soft":
        return mix(mix(p[band], p[koi], 0.3), p["outline"], 0.8)
    if mode == "rim":
        return mix(p["outline"], p[koi], 0.2)   # the light line on the sun side
    return p["outline"]                          # dark, and selout on the shade side


def readability(t):
    """Koi against each water band. Hue does not count: moving fish are tracked by
    lightness, and red on teal of equal lightness collapses for deuteranopes.
    A koi color reads when its OKLab lightness differs from the band by MIN_DL,
    or its drawn edge has WCAG contrast MIN_OUTLINE against the band."""
    p, style = t["palette"], t["style"]
    rows = {}
    for koi in ("koi_white", "koi_red", "ogon", "koi_sumi", "asagi_blue"):
        cells = {}
        for band in BANDS:
            dl = delta_l(p[koi], p[band])
            edge = edge_color(p, style, koi, band)
            o = contrast(edge, p[band]) if edge else 1.0
            ok = abs(dl) >= MIN_DL or o >= MIN_OUTLINE
            cells[band] = {"ratio": round(contrast(p[koi], p[band]), 2), "dl": round(dl, 3),
                           "deutan": round(delta_e_deutan(p[koi], p[band]), 3), "outline": round(o, 2),
                           "ok": ok, "by": "light" if abs(dl) >= MIN_DL else ("edge" if ok else "")}
        rows[koi] = cells
    return rows


def print_ramp(args):
    """Seed colors in, [palette] lines out. Tune the printed hexes by hand afterwards."""
    seeds = dict(a.split("=", 1) for a in args)
    shift = float(seeds.pop("hue_shift", 15))
    out = {}
    if "water" in seeds:
        out["deep"], out["mid"], out["shallow"] = ramp(seeds["water"], shift, 0.13, 0.13)
    if "foliage" in seeds:
        out["lily_dark"], _, out["lily_light"] = ramp(seeds["foliage"], shift, 0.12, 0.14)
    if "stone" in seeds:
        out["stone_dark"], _, out["stone_light"] = ramp(seeds["stone"], shift * 1.5, 0.28, 0.1)
    if "sun" in seeds:
        out["highlight"] = seeds["sun"].upper()
        out["shadow"] = from_oklch(0.47, 0.08, 270)
    print("[palette]")
    for k, v in out.items():
        print(f'{k} = "{v}"')


def main():
    if sys.argv[1:2] == ["ramp"]:
        return print_ramp(sys.argv[2:])
    root = load(ROOT_THEME)
    themes, problems = [], []
    for f in sorted(THEMES.glob("*.toml")):
        name = f.stem
        problems += [f"{name}: unknown key {k}" for k in unknown_keys(name, root)]
        t = load(name)
        missing = [k for k in REQUIRED if k not in t["palette"]]
        if missing:
            problems.append(f"{name}: missing palette slots {', '.join(missing)}")
            continue
        derive(t["palette"])
        if t.get("hidden"):
            continue
        t["id"] = name
        t["readability"] = readability(t)
        themes.append(t)
        problems += [f"{name}: {k} on {b} (lightness gap {c['dl']:+.3f}, edge {c['outline']})"
                     for k, row in t["readability"].items() for b, c in row.items() if not c["ok"] and k != "koi_sumi"]
        if t["palette"]["koi_sumi"] in (t["palette"]["deep"], t["palette"]["mid"], t["palette"]["shallow"]):
            problems.append(f"{name}: koi_sumi equals a water band, sumi patches become holes")

    order = {n: i for i, n in enumerate(TIMES)}
    themes.sort(key=lambda t: (t["style"]["pixel_px"] > 0, t["family"] != "garden", t["family"], order.get(t["time"], 9)))

    print("Worst band per koi color: OKLab lightness gap, then how it reads (light, edge) and the")
    print("deuteranopia OKLab distance. Sumi is a pattern patch and is shown, not judged.\n")
    print(f"{'theme':20} {'white':>16} {'red':>16} {'ogon':>16} {'asagi':>16} {'sumi':>16}")
    for t in themes:
        r = t["readability"]
        def cell(k):
            c = min(r[k].values(), key=lambda c: (c["ok"], abs(c["dl"])))
            return f"{c['dl']:+.2f} {c['by'] or 'FAIL':5} {c['deutan']:.2f}"
        print(f"{t['id']:20} " + " ".join(f"{cell(k):>16}" for k in ("koi_white", "koi_red", "ogon", "asagi_blue", "koi_sumi")))
    if problems:
        print("\nProblems:\n  " + "\n  ".join(problems))

    html = PREVIEW.read_text()
    blob = json.dumps(themes, indent=None, separators=(",", ":"))
    html, n = re.subn(r"(/\*THEMES\*/).*?(/\*END\*/)", lambda m: m.group(1) + blob + m.group(2), html, flags=re.S)
    if n != 1:
        sys.exit("preview.html is missing the /*THEMES*/ ... /*END*/ markers")
    PREVIEW.write_text(html)
    print(f"\nwrote {len(themes)} themes into {PREVIEW.relative_to(ROOT)}")
    sys.exit(1 if problems else 0)


if __name__ == "__main__":
    main()
