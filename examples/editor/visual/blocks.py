# /// script
# requires-python = ">=3.12"
# dependencies = ["pillow==13.0.0", "numpy==2.5.5"]
# ///
"""Compare 2x crops of the demo's table header, session image and code block."""

import argparse
import json
from pathlib import Path

import numpy as np
from PIL import Image


def measure(path):
    with Image.open(path) as image:
        rgb = np.asarray(image.convert("RGB"))
    filled = rgb.min(axis=2) < 250
    rows = np.flatnonzero(filled.sum(axis=1) > 0.35 * rgb.shape[1])
    if not len(rows):
        raise ValueError(f"No block fill found in {path}")
    columns = np.flatnonzero(filled[rows].mean(axis=0) > 0.7)
    if not len(columns) or np.any(np.diff(rows) != 1):
        raise ValueError(f"Expected one filled block in {path}")
    top, bottom = int(rows[0]), int(rows[-1]) + 1
    left, right = int(columns[0]), int(columns[-1]) + 1
    interior = rgb[top + 4 : bottom - 4, left + 4 : right - 4]
    if not interior.size:
        raise ValueError(f"Block too small to sample in {path}")
    colors, counts = np.unique(interior.reshape(-1, 3), axis=0, return_counts=True)
    return {
        "width_px": (right - left) / 2,
        "height_px": (bottom - top) / 2,
        "fill_rgb": "#" + bytes(colors[counts.argmax()]).hex().upper(),
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "directory", type=Path, help="directory with table.png, image.png and code.png"
    )
    args = parser.parse_args()
    reference = Path(__file__).resolve().parent / "notion"
    try:
        results = {}
        for kind in ("table", "image", "code"):
            name = f"{kind}.png"
            results[kind] = {
                "notion": measure(reference / name),
                "demo": measure(args.directory / name),
            }
    except (OSError, ValueError) as error:
        parser.exit(1, f"{error}\n")
    print(json.dumps(results, indent=2))


if __name__ == "__main__":
    main()
