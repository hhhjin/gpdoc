import type { ParagraphExample as RustParagraph } from "../generated/gpdoc-editor.js";

type Report = (message: string, error?: boolean) => void;
type Mark = "bold" | "italic";
type TextRun = { text: string; marks: Mark[] };

function children(node: unknown): unknown[] {
  if (typeof node === "object" && node !== null && "content" in node && Array.isArray(node.content)) {
    return node.content;
  }
  return [];
}

function readParagraph(editor: RustParagraph): TextRun[] {
  const document: unknown = JSON.parse(editor.document_json());
  const [paragraph] = children(document);
  return children(paragraph).map((node) => {
    if (typeof node !== "object" || node === null || !("text" in node) || typeof node.text !== "string") {
      throw new Error("Expected a text node in the example paragraph.");
    }
    const marks: Mark[] = [];
    const markNodes: unknown[] = "marks" in node && Array.isArray(node.marks) ? node.marks : [];
    for (const mark of markNodes) {
      if (typeof mark === "object" && mark !== null && "type" in mark &&
          (mark.type === "bold" || mark.type === "italic")) {
        marks.push(mark.type);
      }
    }
    return { text: node.text, marks };
  });
}

function renderParagraph(preview: HTMLElement, runs: TextRun[], from: number, to: number) {
  const fragment = document.createDocumentFragment();
  let offset = 0;

  for (const { text, marks } of runs) {
    const start = Math.max(0, from - offset);
    const end = Math.max(0, to - offset);
    const content = document.createDocumentFragment();
    content.append(text.slice(0, start));
    const selected = text.slice(start, end);
    if (selected) {
      const highlight = document.createElement("mark");
      highlight.textContent = selected;
      content.append(highlight);
    }
    content.append(text.slice(end));

    let styled: Node = content;
    for (const mark of marks) {
      const element = document.createElement(mark === "bold" ? "strong" : "em");
      element.append(styled);
      styled = element;
    }
    fragment.append(styled);
    offset += text.length;
  }
  preview.replaceChildren(fragment);
}

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

function setupFormat(
  form: HTMLFormElement,
  Paragraph: typeof RustParagraph,
  report: Report,
) {
  const preview = form.querySelector<HTMLElement>("[data-preview]");
  const selection = form.elements.namedItem("selection");
  const bold = form.querySelector<HTMLButtonElement>("[data-bold]");
  const italic = form.querySelector<HTMLButtonElement>("[data-italic]");
  const reset = form.querySelector<HTMLButtonElement>("[data-reset]");
  const initial = preview?.dataset.text;
  if (!preview || !initial || !(selection instanceof HTMLSelectElement) || !bold || !italic || !reset) return;

  let editor = new Paragraph(initial);

  const update = () => {
    const from = initial.indexOf(selection.value);
    const to = from + selection.value.length;
    const runs = readParagraph(editor);
    let offset = 0;
    const selected = runs.filter(({ text }) => {
      const start = offset;
      offset += text.length;
      return start < to && offset > from;
    });
    renderParagraph(preview, runs, from, to);
    bold.setAttribute("aria-pressed", String(selected.every(({ marks }) => marks.includes("bold"))));
    italic.setAttribute("aria-pressed", String(selected.every(({ marks }) => marks.includes("italic"))));
  };

  const toggle = (mark: Mark, button: HTMLButtonElement) => {
    try {
      const from = initial.indexOf(selection.value);
      const to = from + selection.value.length;
      if (mark === "bold") editor.toggle_bold(from, to);
      else editor.toggle_italic(from, to);
      update();
      const applied = button.getAttribute("aria-pressed") === "true";
      const label = mark === "bold" ? "Bold" : "Italic";
      report(`${label} ${applied ? "applied" : "removed"}.`);
    } catch (error) {
      report(errorMessage(error), true);
    }
  };

  selection.addEventListener("change", () => {
    update();
    report("");
  });
  bold.addEventListener("click", () => toggle("bold", bold));
  italic.addEventListener("click", () => toggle("italic", italic));
  reset.addEventListener("click", () => {
    const fresh = new Paragraph(initial);
    editor.free();
    editor = fresh;
    form.reset();
    update();
    report("Example reset.");
  });
}

function setupJson(form: HTMLFormElement, roundTrip: (source: string) => string, report: Report) {
  const source = form.elements.namedItem("source");
  const result = form.elements.namedItem("result");
  const copy = form.querySelector<HTMLButtonElement>("[data-copy]");
  const reset = form.querySelector<HTMLButtonElement>("[data-reset]");
  if (!(source instanceof HTMLTextAreaElement) || !(result instanceof HTMLTextAreaElement) || !copy || !reset) return;

  const clearResult = () => {
    result.value = "";
    copy.disabled = true;
    report("");
  };

  form.addEventListener("submit", (event) => {
    event.preventDefault();
    clearResult();
    try {
      result.value = JSON.stringify(JSON.parse(roundTrip(source.value)), null, 2);
      copy.disabled = false;
      report("Round-trip passed. The reopened document matches the original.");
    } catch (error) {
      report(errorMessage(error), true);
    }
  });

  source.addEventListener("input", clearResult);
  reset.addEventListener("click", () => {
    form.reset();
    clearResult();
    report("Example reset.");
  });
  copy.addEventListener("click", async () => {
    try {
      await navigator.clipboard.writeText(result.value);
      report("JSON copied.");
    } catch {
      result.focus();
      result.select();
      report("Export selected. Use your browser’s copy shortcut.");
    }
  });
}

const forms = document.querySelectorAll<HTMLFormElement>("form[data-doc-example]");
const engine = forms.length ? import("../generated/gpdoc-editor.js").then(async (module) => {
  await module.default();
  return module;
}) : undefined;

for (const form of forms) {
  const status = form.querySelector<HTMLElement>("[data-status]");
  const fields = form.querySelector("fieldset");
  if (!status || !fields) continue;

  const report: Report = (message, error = false) => {
    status.textContent = message;
    status.toggleAttribute("data-error", error);
  };

  engine?.then(({ ParagraphExample, round_trip }) => {
    switch (form.dataset.docExample) {
      case "format":
        setupFormat(form, ParagraphExample, report);
        break;
      case "json":
        setupJson(form, round_trip, report);
        break;
    }
    fields.disabled = false;
    report("");
  }).catch(() => report("Could not load the example. Reload the page to try again.", true));
}
