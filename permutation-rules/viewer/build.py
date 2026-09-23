"""Inline a replay JSON into the viewer template as one self-contained page.

    cargo run --release --example replay -- viewer/replay.json
    python3 viewer/build.py viewer/replay.json viewer/replay.html

Every non-ASCII character is escaped (\\uXXXX in scripts, &#x..; elsewhere) so
the page renders correctly whatever charset it is served with.
"""
import re
import sys
from pathlib import Path

here = Path(__file__).parent
src, dst = sys.argv[1], sys.argv[2]
template = (here / "template.html").read_text(encoding="utf-8")
data = Path(src).read_text(encoding="utf-8").replace("</", "<\\/")


def esc_html(s):
    return "".join(c if ord(c) < 128 else "&#x%X;" % ord(c) for c in s)


def esc_js(s):
    return "".join(c if ord(c) < 128 else "\\u%04x" % ord(c) for c in s)


parts = re.split(r"(<script\b[^>]*>.*?</script>)", template, flags=re.S)
page = "".join(esc_js(p) if p.startswith("<script") else esc_html(p) for p in parts)
page = page.replace("/*REPLAY_DATA*/", esc_js(data))
Path(dst).write_text(page, encoding="ascii")
print(f"wrote {dst} ({len(page)} bytes)")
