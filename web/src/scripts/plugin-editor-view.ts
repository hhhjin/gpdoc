type Attributes = Record<string, unknown>;
type Mark = { type: string; attrs: Attributes };
type DocNode = Mark & { text: string; marks: Mark[]; content: DocNode[] };
export type Position = [number, number];
type Point = { node: Node; offset: number };
type Segment = { node: Text; from: number; to: number; marks: Mark[] };
type Boundary = { point: Point; pos: number };
type SearchResults = { ranges: Position[]; current: number | null };
type Color = [number, number, number, number];

export type Decorations = {
  presentation?: Attributes;
  search?: SearchResults;
  caret?: number;
};

export function isObject(value: unknown): value is Attributes {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

export function items(value: unknown): unknown[] {
  return Array.isArray(value) ? value : [];
}

export function isPosition(value: unknown): value is Position {
  return Array.isArray(value) && value.length === 2 &&
    value.every((n: unknown) => typeof n === "number" && Number.isInteger(n) && n >= 0);
}

export function isSearchResults(value: unknown): value is SearchResults {
  if (!isObject(value) || !Array.isArray(value.ranges) || !value.ranges.every(isPosition)) return false;
  const { current } = value;
  return current === null || (typeof current === "number" && Number.isInteger(current) && current >= 0 && current < value.ranges.length);
}

function color(value: unknown): value is Color {
  return Array.isArray(value) && value.length === 4 &&
    value.every((n: unknown) => typeof n === "number" && Number.isInteger(n) && n >= 0 && n <= 255);
}

function mark(value: unknown): Mark {
  if (!isObject(value) || typeof value.type !== "string") throw new Error("Invalid gpdoc node.");
  return { type: value.type, attrs: isObject(value.attrs) ? value.attrs : {} };
}

function parse(value: unknown): DocNode {
  if (!isObject(value)) throw new Error("Invalid gpdoc document.");
  return {
    ...mark(value),
    text: typeof value.text === "string" ? value.text : "",
    marks: items(value.marks).map(mark),
    content: items(value.content).map(parse),
  };
}

export function element<K extends keyof HTMLElementTagNameMap>(tag: K, ...children: (Node | string)[]) {
  const node = document.createElement(tag);
  node.append(...children);
  return node;
}

const graphemes = new Intl.Segmenter(undefined, { granularity: "grapheme" });
const leafNodes = new Set(["hardBreak", "horizontalRule", "mention", "inlineMath", "image", "audio"]);
const containers = new Map<string, "blockquote" | "ul" | "li" | "tr" | "td" | "th" | "div">([
  ["blockquote", "blockquote"], ["bulletList", "ul"], ["taskList", "ul"], ["listItem", "li"],
  ["tableRow", "tr"], ["tableCell", "td"], ["tableHeader", "th"], ["detailsContent", "div"],
]);
const markTags = new Map<string, "strong" | "em" | "u" | "s" | "code" | "sub" | "sup">([
  ["bold", "strong"], ["italic", "em"], ["underline", "u"], ["strike", "s"],
  ["code", "code"], ["subscript", "sub"], ["superscript", "sup"],
]);

function size(node: DocNode): number {
  if (node.type === "text") return node.text.length;
  if (leafNodes.has(node.type)) return 1;
  return 2 + node.content.reduce((total, child) => total + size(child), 0);
}

function safeUrl(value: unknown, asset = false): string | undefined {
  if (typeof value !== "string" || !value.trim()) return;
  try {
    const base = asset ? new URL(import.meta.env.BASE_URL, window.location.origin) : window.location.href;
    const url = new URL(value, base);
    if (url.protocol === "http:" || url.protocol === "https:") return url.href;
  } catch {
    return;
  }
}

function hex(value: unknown): string | undefined {
  return typeof value === "string" && /^#[0-9a-f]{6}$/i.test(value) ? value : undefined;
}

function format({ type, attrs }: Mark, content: Node): Node {
  const tag = markTags.get(type);
  if (tag) return element(tag, content);
  switch (type) {
    case "rubyText": {
      const reading = element("rt", String(attrs.rt ?? ""));
      reading.contentEditable = "false";
      return element("ruby", content, reading);
    }
    case "link": {
      const href = safeUrl(attrs.href);
      if (!href) return content;
      const link = element("a", content);
      link.href = href;
      return link;
    }
    case "highlight": {
      const highlight = element("mark", content);
      const value = hex(attrs.color);
      if (value) {
        highlight.style.backgroundColor = value;
        highlight.style.color = "#181818";
      }
      return highlight;
    }
    case "textStyle": {
      const span = element("span", content);
      span.style.color = hex(attrs.color) ?? "";
      if (typeof attrs.fontSize === "string" && /^\d{1,2}px$/.test(attrs.fontSize)) {
        span.style.fontSize = attrs.fontSize;
      }
      return span;
    }
    default:
      return content;
  }
}

export class EditorView {
  private spans = new WeakMap<Node, Position>();
  private segments: Segment[] = [];
  private boundaries: Boundary[] = [];
  private closed = new Set<number>();
  private decorations: Decorations = {};
  private caretDrawn = false;

  constructor(readonly root: HTMLElement, private readonly peerName = "") {}

  draw(value: unknown, decorations: Decorations = {}) {
    this.decorations = decorations;
    this.caretDrawn = false;
    this.spans = new WeakMap();
    this.segments = [];
    this.boundaries = [];
    const doc = parse(value);
    this.children(this.root, doc.content, 0, true);
  }

  private children(parent: HTMLElement, nodes: DocNode[], start: number, root = false) {
    parent.replaceChildren();
    let pos = start;
    for (const child of nodes) {
      if (root && this.decorations.caret === pos && !this.caretDrawn) parent.append(this.caret());
      parent.append(this.render(child, pos));
      pos += size(child);
    }
    return pos;
  }

  private cursor(parent: HTMLElement, nodes: DocNode[], start: number) {
    const end = this.children(parent, nodes, start);
    if (!nodes.length || nodes.at(-1)?.type === "hardBreak") parent.append(element("br"));
    if (!nodes.length && this.decorations.caret === start && !this.caretDrawn) parent.prepend(this.caret());
    let found = false;
    parent.childNodes.forEach((child, offset) => {
      const span = this.spans.get(child);
      if (span) {
        this.boundaries.push({ point: { node: parent, offset }, pos: span[0] });
        found = true;
      }
    });
    this.boundaries.push({ point: { node: parent, offset: !found ? 0 : parent.childNodes.length }, pos: end });
  }

  private span(node: Node, from: number, to: number) {
    this.spans.set(node, [from, to]);
    return node;
  }

  private caret() {
    this.caretDrawn = true;
    const caret = element("span", element("span", this.peerName));
    caret.className = "remote-caret";
    caret.contentEditable = "false";
    caret.setAttribute("aria-label", `${this.peerName}’s caret`);
    return caret;
  }

  private text(text: string, from: number, marks: Mark[]) {
    const end = from + text.length;
    const fragment = document.createDocumentFragment();
    const cuts = new Set([from, end]);
    const { caret, search } = this.decorations;
    const matches = search?.ranges ?? [];
    if (caret !== undefined && caret >= from && caret <= end) cuts.add(caret);
    for (const [start, stop] of matches) {
      if (start < end && stop > from) {
        cuts.add(Math.max(start, from));
        cuts.add(Math.min(stop, end));
      }
    }
    const points = [...cuts].sort((a, b) => a - b);
    for (const [index, start] of points.entries()) {
      if (start === caret && !this.caretDrawn) {
        fragment.append(this.caret());
      }
      const stop = points[index + 1];
      if (stop === undefined) break;
      const node = document.createTextNode(text.slice(start - from, stop - from));
      this.segments.push({ node, from: start, to: stop, marks });
      this.span(node, start, stop);
      const match = matches.findIndex(([a, b]) => a <= start && stop <= b);
      let content: Node = node;
      if (match !== -1) {
        const highlight = element("mark", node);
        highlight.className = "search-match";
        if (match === search?.current) highlight.setAttribute("aria-current", "true");
        content = highlight;
      }
      const formatted = marks.reduce<Node>((result, mark) => format(mark, result), content);
      fragment.append(this.span(formatted, start, stop));
    }
    // Keep decorations and annotations within a single positioned text run.
    return this.span(element("span", fragment), from, end);
  }

  private render(node: DocNode, from: number): Node {
    const { type, attrs, content } = node;
    const tag = containers.get(type);
    const decoration = this.decorations.presentation?.[from];
    const view = isObject(decoration) ? decoration : {};
    const wrap = (box: HTMLElement) => {
      this.children(box, content, from + 1);
      return box;
    };
    let html: Node;
    if (tag) html = wrap(element(tag));
    else switch (type) {
      case "text":
        return this.text(node.text, from, node.marks);
      case "paragraph":
      case "heading":
      case "detailsSummary": {
        const block = type === "paragraph" ? element("p") : element("div");
        if (type === "heading") {
          block.setAttribute("role", "heading");
          block.setAttribute("aria-level", String(attrs.level ?? 1));
        }
        if (attrs.textAlign === "left" || attrs.textAlign === "center" || attrs.textAlign === "right" || attrs.textAlign === "justify") {
          block.style.textAlign = attrs.textAlign;
        }
        this.cursor(block, content, from + 1);
        html = block;
        break;
      }
      case "orderedList": {
        const list = element("ol");
        list.start = typeof attrs.start === "number" ? attrs.start : 1;
        html = wrap(list);
        break;
      }
      case "taskItem": {
        const item = wrap(element("li"));
        item.dataset.task = "";
        const check = element("input");
        check.type = "checkbox";
        check.checked = attrs.checked === true;
        check.contentEditable = "false";
        check.dataset.checkTask = String(from);
        check.setAttribute("aria-label", "Task complete");
        item.prepend(check);
        html = item;
        break;
      }
      case "table":
        html = element("table", wrap(element("tbody")));
        break;
      case "details": {
        const details = wrap(element("div"));
        details.className = "disclosure";
        const [summary, body] = details.children;
        if (summary instanceof HTMLElement && body instanceof HTMLElement) {
          const control = element("button");
          control.type = "button";
          control.contentEditable = "false";
          control.className = "disclosure-toggle";
          control.setAttribute("aria-label", "Toggle disclosure");
          const toggle = () => {
            body.hidden = this.closed.has(from);
            control.textContent = body.hidden ? "▸" : "▾";
            control.setAttribute("aria-expanded", String(!body.hidden));
          };
          toggle();
          control.onclick = () => {
            if (!this.closed.delete(from)) this.closed.add(from);
            toggle();
          };
          details.prepend(control);
        }
        html = details;
        break;
      }
      case "codeBlock": {
        const code = element("code");
        const pre = element("pre", code);
        const tokens = items(view.tokens).flatMap((token): DocNode[] =>
          isObject(token) && typeof token.text === "string" && color(token.color)
            ? [{ type: "text", attrs: { color: token.color }, text: token.text, marks: [], content: [] }]
            : []);
        // Token text has the same document offsets as the source text it colors.
        this.cursor(code, tokens.length ? tokens : content, from + 1);
        if (tokens.length) {
          pre.dataset.syntax = "";
          code.childNodes.forEach((segment, index) => {
            const rgba = tokens[index]?.attrs.color;
            if (segment instanceof HTMLElement && color(rgba)) {
              const [r, g, b, a] = rgba;
              segment.style.color = `rgba(${r}, ${g}, ${b}, ${a / 255})`;
            }
          });
        }
        html = pre;
        break;
      }
      case "hardBreak":
        html = element("br");
        break;
      case "horizontalRule":
        html = element("hr");
        break;
      case "mention":
      case "inlineMath": {
        const fallback = type === "inlineMath" ? attrs.latex : attrs.label;
        html = element("code", String(view.label ?? fallback ?? ""));
        break;
      }
      case "image": {
        const image = element("img");
        image.alt = String(attrs.alt ?? "");
        const src = safeUrl(attrs.src, true);
        if (src) image.src = src;
        if (typeof attrs.width === "number") image.width = attrs.width;
        html = image;
        break;
      }
      case "audio": {
        const audio = element("audio");
        audio.controls = true;
        audio.preload = "metadata";
        const src = safeUrl(attrs.src, true);
        if (src) audio.src = src;
        audio.loop = attrs.loop === true;
        html = audio;
        break;
      }
      default:
        html = wrap(element("div"));
    }
    if (html instanceof HTMLElement && leafNodes.has(type) && type !== "hardBreak") {
      html.contentEditable = "false";
    }
    return this.span(html, from, from + size(node));
  }

  private at(pos: number): { point: Point; pos: number } | undefined {
    const hit = this.segments.find((segment) => pos >= segment.from && pos <= segment.to);
    if (hit) return { point: { node: hit.node, offset: pos - hit.from }, pos };
    return this.boundaries.reduce<Boundary | undefined>((best, current) =>
      !best || Math.abs(current.pos - pos) < Math.abs(best.pos - pos) ? current : best, undefined);
  }

  private position(node: Node, offset: number): number | undefined {
    const span = this.spans.get(node);
    if (node instanceof Text && span) return span[0] + Math.min(offset, node.length);
    if (node instanceof Element) {
      for (const child of Array.from(node.childNodes).slice(offset)) {
        const next = this.spans.get(child);
        if (next) return next[0];
      }
      for (const child of Array.from(node.childNodes).slice(0, offset).reverse()) {
        const previous = this.spans.get(child);
        if (previous) return previous[1];
      }
    }
    const exact = this.boundaries.find(({ point }) => point.node === node && point.offset === offset);
    if (exact) return exact.pos;
    if (span) return span[0];
    if (!node.parentNode || node === this.root) return;
    const index = Array.from(node.parentNode.childNodes).findIndex((child) => child === node);
    return this.position(node.parentNode, index);
  }

  selection(): Position | undefined {
    const selection = window.getSelection();
    if (!selection?.anchorNode || !selection.focusNode ||
        !this.root.contains(selection.anchorNode) || !this.root.contains(selection.focusNode)) return;
    const from = this.position(selection.anchorNode, selection.anchorOffset);
    const to = this.position(selection.focusNode, selection.focusOffset);
    if (from === undefined || to === undefined) return;
    return [this.at(from)?.pos ?? from, this.at(to)?.pos ?? to];
  }

  restore([anchor, head]: Position) {
    const from = this.at(anchor)?.point;
    const to = this.at(head)?.point;
    if (from && to) window.getSelection()?.setBaseAndExtent(from.node, from.offset, to.node, to.offset);
  }

  hasMark(type: string, [anchor, head]: Position) {
    const from = Math.min(anchor, head);
    const to = Math.max(anchor, head);
    const affected = this.segments.filter((segment) => from === to
      ? segment.from <= from && segment.to >= from
      : segment.from < to && segment.to > from);
    return affected.length > 0 && affected.every((segment) => segment.marks.some((mark) => mark.type === type));
  }

  adjacent(pos: number, forward: boolean) {
    const segment = this.segments.find((run) => forward
      ? run.from <= pos && pos < run.to
      : run.from < pos && pos <= run.to);
    if (segment) {
      const parts = graphemes.segment(segment.node.data);
      const char = parts.containing(pos - segment.from + (forward ? 0 : -1));
      if (char) return segment.from + char.index + (forward ? char.segment.length : 0);
    }
    const offsets = this.boundaries.map((point) => point.pos);
    for (const run of this.segments) offsets.push(run.from, run.to);
    const next = offsets.filter((value) => forward ? value > pos : value < pos);
    return (forward ? Math.min(...next, Infinity) : Math.max(...next, -Infinity));
  }
}
