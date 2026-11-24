# Compare the editor demo with Notion

The reference images in [notion/](notion/) show the [Project kickoff demo](../kickoff.json) in the Notion desktop app on macOS, in light mode at 2× display scale. They contain only the editor content. Contributors can reuse them without access to the original Notion page.

## Compare a demo window

Use macOS with Swift, a 2× display and [uv](https://docs.astral.sh/uv/). Run these commands from the repository root. Start the demo in one terminal:

```sh
cargo run --locked -p gpdoc-editor-demo
```

Keep the default 960-point window width. Scroll to the top so both the body heading “Project kickoff” and the heading “Review schedule” are visible. Clear any selection. In a second terminal, run:

```sh
uv run examples/editor/visual/compare.py
```

Click the demo window when the macOS window picker appears. If macOS requests Screen Recording access for the terminal, grant it to use the picker. To use an existing shadow-free window screenshot instead, pass `--window PATH`. Both routes expect a 1920-physical-pixel-wide image. The script finds the headings with Apple Vision and crops the centered content column at native resolution.

Open `target/notion-comparison/comparison.png` for the two captures and their overlay. Blue marks reference-only ink, red marks demo-only ink, and black marks overlapping ink. The reported percentage is the intersection divided by the union of pixels with all three RGB channels below 190. The script aligns the top of the first heading once. Each subsequent vertical offset in `metrics.json` measures accumulated layout drift in logical pixels. The report also includes OCR lines, sampled colors and ink heights.

To check the setup without capturing a window, compare the reference with itself. Expect 100% foreground overlap and 11 of 11 subsequent anchors within 2 px:

```sh
uv run examples/editor/visual/compare.py --crop examples/editor/visual/notion/body.png --out target/notion-self-check
```

## Measure the table, image and code block

Save three 2× demo crops in one directory as `table.png` (the first table row only), `image.png` (the session plan diagram) and `code.png` (the shell command block). Keep the complete colored rectangle and a little surrounding white space. The rectangle must cover more than 35% of the crop width. The corresponding [reference crops](notion/) show the framing.

```sh
uv run examples/editor/visual/blocks.py path/to/demo-crops
```

The JSON output gives reference and demo widths, heights and sampled fill colors. This check uses NumPy and Pillow and also runs on Linux and Windows. Use 2× captures throughout; reference colors are sampled from screenshots and can vary with the capture method.

## Refresh the reference

Refresh the four PNGs when intentionally adopting a different Notion appearance or changing the comparison document. Use the matching demo content in light mode, default font and normal page width. Crop the body image to 1420 physical pixels wide from the content column’s left edge, with a small margin above the body H1 and below the “Review schedule” heading. Exclude the Notion page title and table. Preserve native pixel dimensions for every crop and review the baseline changes alongside the demo changes.
