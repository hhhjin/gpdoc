# /// script
# requires-python = ">=3.12"
# dependencies = ["pillow==13.0.0", "numpy==2.5.5"]
# ///
"""Compare the gpdoc editor demo with the bundled Notion light-mode reference."""

import argparse
import json
import re
import subprocess
import sys
import tempfile
from itertools import pairwise
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw

LABELS = (
    "Project kickoff",
    "This week, five",
    "Owner",
    "This week's goal",
    "Someone opening",
    "Discussion",
    "What do people",
    "Where do they",
    "Record the block",
    "Before the review",
    "Prepare a sample",
    "Review schedule",
)
DPR = 2
INK = 190
HERE = Path(__file__).resolve().parent
REFERENCE = HERE / "notion" / "body.png"
DEFAULT_OUTPUT = HERE.parents[2] / "target" / "notion-comparison"


def recognize(path):
    lines = json.loads(
        subprocess.check_output(["swift", str(HERE / "recognize.swift"), str(path)])
    )
    return sorted(lines, key=lambda line: line["y"])


def crop_window(path, dest):
    with Image.open(path) as image:
        if image.width != 960 * DPR:
            raise ValueError(
                "Expected a shadow-free, 1920-physical-pixel-wide gpdoc window. "
                "Use the default 960-point window size on a 2x display."
            )
        lines = recognize(path)

        def heading(text):
            matches = [line for line in lines if line["text"] == text]
            if len(matches) != 1:
                raise ValueError(
                    f"Expected one visible {text!r} heading. Scroll to the top."
                )
            return matches[0]

        first = heading("Project kickoff")
        last = heading("Review schedule")
        # Keep the reference column's left edge fixed so horizontal drift remains visible.
        left = (image.width - 704 * DPR) // 2
        with Image.open(REFERENCE) as reference:
            right = left + reference.width
        top = max(0, round(first["y"]) - 24)
        bottom = round(last["y"] + last["h"]) + 10
        if bottom <= top or bottom > image.height:
            raise ValueError("Both headings must be fully visible in the capture.")
        image.crop((left, top, right, bottom)).convert("RGB").save(dest)


def inspect(path):
    with Image.open(path) as image:
        rgb = np.array(image.convert("RGB"))
    lines = recognize(path)
    ink = rgb.max(axis=2) < INK
    anchors = {}
    for label in LABELS:
        candidates = [
            line
            for line in lines
            if re.sub(r"^[^a-zA-Z]+", "", line["text"])
            .replace("’", "'")
            .startswith(label)
        ]
        if len(candidates) != 1:
            raise ValueError(
                f"{path}: expected one OCR line starting with {label!r}, found {len(candidates)}"
            )
        line = candidates[0]
        # Refine Vision's line box using the first/last rows carrying glyphs.
        # Ignore the first 5 logical pixels (insertion caret or quote border).
        left = max(10, round(line["x"]))
        right = min(rgb.shape[1], round(line["x"] + line["w"]) + 4)
        top = max(0, int(line["y"]) - 4)
        bottom = min(len(rgb), round(line["y"] + line["h"]) + 4)
        region = ink[top:bottom, left:right]
        rows = np.flatnonzero(region.sum(axis=1) >= 4)
        if not len(rows):
            raise ValueError(f"No foreground found for {label!r}")
        # Vision can include a few pixels of the next wrapped line in a box.
        runs = np.split(rows, np.flatnonzero(np.diff(rows) > DPR * 2) + 1)
        rows = max(runs, key=lambda run: region[run].sum())
        anchors[label] = {"top": int(top + rows[0]), "bottom": int(top + rows[-1] + 1)}
    tops = [anchor["top"] for anchor in anchors.values()]
    if any(first >= second for first, second in pairwise(tops)):
        raise ValueError("Anchor order differs from the kickoff fixture")
    start = anchors[LABELS[0]]["top"]
    end = anchors[LABELS[-1]]["bottom"]
    # Estimate capture colors only on the relevant rows; JPEG capture can shift RGB.
    title = rgb[start : anchors[LABELS[0]]["bottom"], 10:]
    quote = rgb[anchors["Someone opening"]["top"] : anchors["Discussion"]["top"]]
    yellow = (quote[:, :, 0] > 210) & (quote[:, :, 0].astype(int) - quote[:, :, 2] > 20)

    def mode_color(pixels):
        if not len(pixels):
            return None
        colors, counts = np.unique(pixels.reshape(-1, 3), axis=0, return_counts=True)
        return "#" + bytes(colors[counts.argmax()]).hex().upper()

    colors = {
        "heading_ink": mode_color(title[title.max(axis=2) < 100]),
        "yellow_fill": mode_color(quote[yellow]),
        "paper": mode_color(rgb[0]),
    }
    return rgb[start:end], anchors, colors, [line["text"] for line in lines]


def compare(candidate, output):
    with Image.open(candidate) as image, Image.open(REFERENCE) as reference:
        if image.width != reference.width:
            raise ValueError(
                f"Expected a {reference.width}px-wide body crop. "
                "Pass a complete window screenshot with --window."
            )
    notion, notion_anchors, notion_colors, notion_lines = inspect(REFERENCE)
    demo, demo_anchors, demo_colors, demo_lines = inspect(candidate)
    height, width = max(len(notion), len(demo)), max(notion.shape[1], demo.shape[1])

    def pad(image):
        return np.pad(
            image,
            ((0, height - len(image)), (0, width - image.shape[1]), (0, 0)),
            constant_values=255,
        )

    notion, demo = pad(notion), pad(demo)
    notion_ink, demo_ink = notion.max(2) < INK, demo.max(2) < INK
    union = notion_ink | demo_ink
    overlay = np.full_like(notion, 255)
    overlay[notion_ink] = [13, 126, 162]
    overlay[demo_ink] = [201, 52, 95]
    overlay[notion_ink & demo_ink] = [41, 41, 41]
    positions = []
    for key in LABELS:
        notion_y = (notion_anchors[key]["top"] - notion_anchors[LABELS[0]]["top"]) / DPR
        demo_y = (demo_anchors[key]["top"] - demo_anchors[LABELS[0]]["top"]) / DPR
        positions.append(
            {
                "anchor": key,
                "notion_y": notion_y,
                "demo_y": demo_y,
                "demo_minus_notion_y": demo_y - notion_y,
                "notion_ink_height": (
                    notion_anchors[key]["bottom"] - notion_anchors[key]["top"]
                )
                / DPR,
                "demo_ink_height": (
                    demo_anchors[key]["bottom"] - demo_anchors[key]["top"]
                )
                / DPR,
            }
        )
    report = {
        "scope": "Body H1 through Review schedule heading, light mode, 2x capture",
        "ink_mask": "max(R,G,B) < 190; column edges and H1 ink tops aligned, no resizing",
        "foreground_iou_pct": round(
            100 * (notion_ink & demo_ink).sum() / union.sum(), 3
        ),
        "ink_intersection": int((notion_ink & demo_ink).sum()),
        "ink_union": int(union.sum()),
        "anchor_y_within_2px_excluding_origin": sum(
            abs(r["demo_minus_notion_y"]) <= 2 for r in positions[1:]
        ),
        "tested_non_origin_anchors": len(positions) - 1,
        "anchors_logical_px": positions,
        "sampled_rgb": {"notion": notion_colors, "demo": demo_colors},
        "ocr_lines": {"notion": notion_lines, "demo": demo_lines},
    }
    output.mkdir(parents=True, exist_ok=True)
    (output / "metrics.json").write_text(
        json.dumps(report, indent=2, ensure_ascii=False) + "\n"
    )
    panels = [
        Image.fromarray(image).resize(
            (width // DPR, height // DPR), Image.Resampling.LANCZOS
        )
        for image in (notion, demo, overlay)
    ]
    panel = Image.new("RGB", (3 * (width // DPR + 24), height // DPR + 50), "white")
    draw = ImageDraw.Draw(panel)
    for i, (image, label) in enumerate(
        zip(
            panels,
            ("Notion", "gpdoc", "Overlay   blue = Notion   red = gpdoc   black = both"),
        )
    ):
        x = i * (image.width + 24) + 12
        draw.text((x, 10), label, fill="#222222", font_size=16)
        panel.paste(image, (x, 40))
    panel.save(output / "comparison.png")
    print(f"Foreground IoU: {report['foreground_iou_pct']:.2f}%")
    print(
        f"Vertical anchors within 2 px: {report['anchor_y_within_2px_excluding_origin']}"
        f"/{len(positions) - 1}"
    )
    print(f"Results: {output.resolve()}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    source = parser.add_mutually_exclusive_group()
    source.add_argument(
        "--window", type=Path, help="existing 2x demo window screenshot"
    )
    source.add_argument(
        "--crop", type=Path, help="existing 2x body crop with the reference width"
    )
    parser.add_argument("--out", type=Path, default=DEFAULT_OUTPUT)
    args = parser.parse_args()
    if sys.platform != "darwin":
        parser.exit(2, "Body comparison requires macOS, Swift and Apple Vision.\n")
    try:
        if args.crop:
            compare(args.crop, args.out)
            return
        with tempfile.TemporaryDirectory() as tmp:
            window = args.window
            if window is None:
                window = Path(tmp) / "window.png"
                print("Click the gpdoc window to capture it.", flush=True)
                subprocess.run(
                    ["screencapture", "-i", "-W", "-o", str(window)], check=True
                )
                if not window.exists():
                    parser.exit(1, "No window captured.\n")
            crop = Path(tmp) / "body.png"
            crop_window(window, crop)
            compare(crop, args.out)
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        parser.exit(1, f"{error}\n")


if __name__ == "__main__":
    main()
