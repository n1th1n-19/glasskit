#!/usr/bin/env python3
"""glasskit grain splash: renders glasskit/{r00-r13,l00-l09,ready}.png + preview.html. Needs Pillow."""
import math, os, random
from PIL import Image, ImageChops, ImageDraw, ImageFilter, ImageFont

HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(HERE, "glasskit")
W, H = 900, 240
FG, ACCENT = (244, 245, 247), (59, 130, 246)  # text #f4f5f7, dot #3b82f6
NR, NL = 14, 10

font = ImageFont.truetype(os.path.join(HERE, "Nunito[wght].ttf"), 200)
font.set_variation_by_axes([900])  # Black

def mask(txt, x=None, y=None):
    m = Image.new("L", (W, H), 0)
    ImageDraw.Draw(m).text((x, y), txt, font=font, fill=255, anchor="ls")
    return m

# centre "n1th1n." on baseline; the dot sits right after the text
base = 190
tw = font.getlength("n1th1n"); dw = font.getlength(".")
x0 = (W - tw - dw) / 2
text_m = mask("n1th1n", x0, base)
dot_m = mask(".", x0 + tw, base)
blur = lambda m: m.filter(ImageFilter.GaussianBlur(7))
text_b, dot_b = blur(text_m), blur(dot_m)

def grain(core, soft, reveal, rng, core_at=0.6):
    """keep pixel if rand < blurred_alpha*reveal; crisp core once reveal > core_at"""
    noise = Image.frombytes("L", (W, H), rng.randbytes(W * H))
    keep = ImageChops.subtract(soft.point(lambda v: int(v * reveal)), noise).point(lambda v: 255 if v else 0)
    if reveal > core_at:
        keep = ImageChops.lighter(keep, core.point(lambda v: 255 if v > 230 else 0))
    return keep

def frame(reveal, dot_level, seed):
    rng = random.Random(seed)
    img = Image.new("RGBA", (W, H), (0, 0, 0, 0))
    img.paste(Image.new("RGBA", (W, H), FG + (255,)), mask=grain(text_m, text_b, reveal, rng))
    if dot_level > 0:  # dot is crisp above level 0.5, grainy below (the blink)
        img.paste(Image.new("RGBA", (W, H), ACCENT + (255,)), mask=grain(dot_m, dot_b, dot_level, rng, 0.5))
    return img

os.makedirs(OUT, exist_ok=True)
names = []
for i in range(NR):  # reveal: grain condenses, dot blinks
    r = (i + 1) / NR
    d = min(1, r * 1.2) * (0.35 + 0.65 * abs(math.sin(i * 0.9)))
    n = f"r{i:02d}"; frame(r, d, 100 + i).save(f"{OUT}/{n}.png"); names.append(n)
for i in range(NL):  # loop: re-seeded grain, dot pulses (sinusoid, one full cycle)
    d = 0.55 + 0.45 * math.sin(2 * math.pi * i / NL)
    n = f"l{i:02d}"; frame(1, d, 500 + i).save(f"{OUT}/{n}.png"); names.append(n)
frame(1, 1, 900).save(f"{OUT}/ready.png"); names.append("ready")

cells = "".join(f'<figure><img src="glasskit/{n}.png"><figcaption>{n}</figcaption></figure>' for n in names)
open(os.path.join(HERE, "preview.html"), "w").write(
    f"""<!doctype html><meta charset=utf-8><title>glasskit splash</title>
<style>body{{background:#0c0e12;color:#8a8f98;font:12px sans-serif;margin:16px}}
main{{display:grid;grid-template-columns:repeat(auto-fill,minmax(300px,1fr));gap:12px}}
img{{width:100%;image-rendering:pixelated;display:block}}figure{{margin:0;border:1px solid #262a33;border-radius:12px;padding:8px}}</style>
<main>{cells}</main>""")
print(len(names), "frames ->", OUT)
