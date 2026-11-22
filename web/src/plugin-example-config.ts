type Choice = [value: string, label: string];
type Label = { name: string; label: string };
type Field = Label & (
  | { kind: "text" | "textarea" | "color"; value: string; optional?: boolean }
  | { kind: "number"; value: number; min: number; max: number }
  | { kind: "select"; choices: Choice[]; boolean?: boolean }
);

export type CommandArgs = Record<string, boolean | number | string>;

type Action = {
  id: string;
  command: string;
  label: string;
  fields: Field[];
  mark?: string;
  args?: CommandArgs;
  fieldsFrom?: string;
};

type Example = {
  source: string;
  hint: string;
  actions: Action[];
  selection?: [number, number];
};

const input = (name: string, label: string, value: string, optional = false): Field =>
  ({ kind: "text", name, label, value, optional });
const number = (name: string, label: string, value: number, min: number, max: number): Field =>
  ({ kind: "number", name, label, value, min, max });
const select = (name: string, label: string, choices: Choice[]): Field => ({ kind: "select", name, label, choices });
const options = (...values: string[]): Choice[] => values.map((value) => [value, value]);
const yesNo = (name: string, label: string, yes: string, no: string): Field =>
  ({ kind: "select", name, label, choices: [["true", yes], ["false", no]], boolean: true });
const color = (value: string): Field => ({ kind: "color", name: "color", label: "Color", value });
const action = (command: string, label: string, ...fields: Field[]): Action => ({ id: command, command, label, fields });
const example = (source: string, hint: string, ...actions: Action[]): Example => ({ source, hint, actions });
const withSelection = (config: Example, from: number, to = from): Example => ({ ...config, selection: [from, to] });
const mark = (source: string, phrase: string, ...actions: Action[]) => {
  const from = source.indexOf(phrase) + 1;
  return withSelection(example(source, "Select text in the editor, then use the toolbar.", ...actions), from, from + phrase.length);
};

const headings = [1, 2, 3].map((level) => ({
  ...action("setHeading", `H${level}`), id: `heading-${level}`, args: { level },
}));
const alignments = ["left", "center", "right", "justify"].map((textAlign) => ({
  ...action("setTextAlign", textAlign === "justify" ? "Justify" : `Align ${textAlign}`),
  id: `align-${textAlign}`, args: { textAlign },
}));

const language = select("language", "Language", [["rust", "Rust"], ["txt", "Plain text"]]);
const code = 'fn main() {\n    println!("Hello");\n}';
const task = action("toggleTaskList", "Task list");

export const pluginExamples = {
  overview: mark("Review the release notes before Friday.", "release notes",
    { ...action("toggleBold", "Bold"), mark: "bold" },
    { ...action("toggleItalic", "Italic"), mark: "italic" },
    { ...action("toggleUnderline", "Underline"), mark: "underline" },
    { ...action("toggleHeading", "Heading 2"), id: "heading-2", args: { level: 2 } },
    action("toggleBlockquote", "Quote"),
    action("toggleBulletList", "Bullet list"),
    action("toggleOrderedList", "Numbered list")),
  bold: mark("Review the release notes before Friday.", "release notes", { ...action("toggleBold", "Bold"), mark: "bold" }),
  italic: mark("A title like Pride and Prejudice takes italics.", "Pride and Prejudice", { ...action("toggleItalic", "Italic"), mark: "italic" }),
  underline: mark("Sign and date this form.", "Sign", { ...action("toggleUnderline", "Underline"), mark: "underline" }),
  strike: mark("Old price: $49. New price: $29.", "$49", { ...action("toggleStrike", "Strikethrough"), mark: "strike" }),
  "inline-code": mark("Run cargo build before publishing.", "cargo build", { ...action("toggleCode", "Inline code"), mark: "code" }),
  subscript: mark("H2O", "2", { ...action("toggleSubscript", "Subscript"), mark: "subscript" }),
  superscript: mark("x2", "2", { ...action("toggleSuperscript", "Superscript"), mark: "superscript" }),
  link: mark("Read the Quickstart to open an editor.", "Read the Quickstart",
    action("setLink", "Link", input("href", "Destination", "/docs/quickstart/")), action("unsetLink", "Remove link")),
  highlight: mark("Bring your notebook to the workshop.", "notebook", {
    ...action("toggleHighlight", "Highlight"), mark: "highlight",
  }),
  "ruby-text": mark("東京へ行きます。", "東京",
    action("setRubyText", "Ruby text", input("rt", "Reading", "とうきょう")), action("unsetRubyText", "Remove reading")),
  "text-align": example("A note from the editor", "Place the caret in a paragraph and change its alignment.",
    ...alignments),
  "text-style": mark("Release notes", "Release notes", action("styleSelection", "Text style", color("#397ee0"),
    select("fontSize", "Font size", options("28px", "18px", "36px")))),

  paragraph: example("Release notes", "Use Paragraph to turn the heading into a regular text block.", action("setParagraph", "Paragraph")),
  heading: example("Release notes", "Choose a heading level for the current block.",
    ...headings, action("setParagraph", "Paragraph")),
  blockquote: example("The launch is on Friday.", "Wrap the current block in a quote. Use Quote again to unwrap it.",
    action("toggleBlockquote", "Quote")),
  "bullet-list": example("Keep a changelog", "Use Bullet list to create a list, then press Enter to add an item.",
    action("toggleBulletList", "Bullet list")),
  "ordered-list": example("Publish the release", "Set a starting number. Press Enter in the list to add the next item.",
    action("toggleOrderedList", "Numbered list", number("start", "Start at", 4, 1, 99))),
  "list-item": withSelection(example("DraftReview", "Place the caret between Draft and Review, then choose Split item or press Enter.",
    action("splitListItem", "Split item"), action("sinkListItem", "Indent"), action("liftListItem", "Outdent")), 8),
  "list-keymap": withSelection(example("First item", "Place the caret at the start of the item and press Backspace to lift it out of the list.",
    action("listBackspace", "Backspace")), 3),
  "list-kit": example("Review the copy", "Switch list types from the toolbar or type a list marker in an empty paragraph.",
    action("toggleBulletList", "Bullet list"), action("toggleOrderedList", "Numbered list"), task),
  "task-list": example("Write the release notes", "Make a task list, then click a checkbox. You can also type [x] followed by a space on an empty line.",
    task),
  "horizontal-rule": example("Next section", "Insert a divider at the caret.", action("setHorizontalRule", "Divider")),
  "hard-break": withSelection(example("FirstSecond", "Place the caret between First and Second, then insert a line break without creating a paragraph.",
    action("setHardBreak", "Line break")), 6),
  "code-block": example(code, "Turn the current text block into code and keep editing it.",
    action("setCodeBlock", "Code block", language), action("setParagraph", "Paragraph")),
  "code-block-lowlight": example(code, "Choose Rust or Plain text to compare syntax coloring, then edit the code.",
    action("setCodeBlock", "Highlight code", language), action("setParagraph", "Paragraph")),
  details: example("The full changelog is ready.", "Wrap the current block and click the disclosure arrow to show or hide its content.",
    action("wrapDetails", "Disclosure", input("summary", "Summary", "Release details")), action("unsetDetails", "Unwrap")),
  table: example("", "Insert a table, then click any cell to type.",
    action("insertTable", "Table", number("rows", "Rows", 3, 1, 6), number("cols", "Columns", 2, 1, 6),
      yesNo("withHeaderRow", "First row", "Header row", "Regular cells")),
    action("addRowAfter", "Add row"), action("addColumnAfter", "Add column")),

  image: example("", "Insert the sample image or enter your own image URL.",
    action("setImage", "Image", input("src", "Image URL", "/favicon.png"), input("alt", "Alternative text", "gpdoc logo"),
      number("width", "Width in pixels", 112, 24, 640))),
  audio: example("", "Insert the sample chime and play it from the editor.",
    action("setAudio", "Audio", input("src", "Audio URL", "/examples/chime.wav"),
      yesNo("loop", "Playback", "Loop", "Play once"))),
  mathematics: withSelection(example("Area = ", "Insert a formula at the caret.", action("insertInlineMath", "Formula", input("latex", "LaTeX", "\\pi r^2"))), 8),
  mention: withSelection(example("Ask @A", "Put the caret after an @ query then choose a person and click Mention.",
    action("acceptMention", "Mention", select("label", "Person", options("Ada", "Alex")))), 7),

  typography: example("", 'Type three periods, straight quotes, or (c). The editor applies the typing rules as you go.'),
  "character-count": example("Hi! 😀", "Type in the editor to update the count. The limit is 24 UTF-16 code units."),
  "find-and-replace": example("The draft includes draft notes.", "Search your text from the toolbar, then replace all matches.",
    action("find", "Find", input("term", "Find", "draft"), input("replacement", "Replace with", "final", true)),
    action("goToNextResult", "Next match"),
    { ...action("replaceMatches", "Replace all"), fieldsFrom: "find" }),
  markdown: example("Release notes", "Import Markdown from the toolbar, edit the result, then export it.",
    action("setMarkdown", "Import Markdown", {
      kind: "textarea", name: "markdown", label: "Markdown", value: "# Release notes\n\nA **bold** word and an *italic* word.",
    }), action("exportMarkdown", "Export Markdown")),
  suggestion: withSelection(example("Ask @Ada", "Put the caret after @Ada, then inspect the query detected at that position.",
    action("findSuggestion", "Inspect query", select("trigger", "Trigger", options("@", ":")))), 9),
  "table-of-contents": example("Release notes", "Edit either heading to update the outline and its anchors.",
    ...headings, action("setParagraph", "Paragraph")),
  "trailing-node": example("Release notes", "Convert the final paragraph to a heading. A new paragraph appears beneath it.",
    ...headings),
  "unique-id": example("Release notes", "Duplicate a paragraph to compare its generated ID with the original.",
    action("duplicate", "Duplicate block")),
  collaboration: example("Release notes", "Type in either editor to see the change in both."),
  "collaboration-caret": example("Release notes", "Move the caret in either editor to see it in the other. Edits sync as you type."),
} satisfies Record<string, Example>;

export function pluginExample(id: string): Example | undefined {
  return Object.entries(pluginExamples).find(([key]) => key === id)?.[1];
}
