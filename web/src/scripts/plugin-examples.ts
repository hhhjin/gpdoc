import type { InputKind, PluginEditor } from "../generated/gpdoc-editor.js";
import { pluginExample, type CommandArgs } from "../plugin-example-config";
import { attachDocsToolbar } from "./docs-toolbar";
import { EditorView, element, isObject, isPosition, isSearchResults, items, type Position } from "./plugin-editor-view";

type Field = HTMLInputElement | HTMLSelectElement | HTMLTextAreaElement;
type Report = (message: string, error?: boolean) => void;

function readArguments(group: HTMLElement): CommandArgs | undefined {
  const fields: CommandArgs = {};
  for (const field of group.querySelectorAll<Field>("input, select, textarea")) {
    if (!field.reportValidity()) return;
    if (field instanceof HTMLInputElement && field.type === "number") {
      fields[field.name] = field.valueAsNumber;
    } else if (field.hasAttribute("data-boolean")) {
      fields[field.name] = field.value === "true";
    } else {
      fields[field.name] = field.value;
    }
  }
  return fields;
}

function isFact(value: unknown): value is [string, string] {
  return Array.isArray(value) && value.length === 2 && value.every((part: unknown) => typeof part === "string");
}

export function setupPlugin(form: HTMLFormElement, Editor: typeof PluginEditor, report: Report) {
  const slug = form.dataset.plugin;
  const config = slug && pluginExample(slug);
  const root = form.querySelector<HTMLElement>("[data-editor]");
  const peer = form.querySelector<HTMLElement>("[data-peer]");
  const facts = form.querySelector<HTMLElement>("[data-plugin-facts]");
  if (!slug || !config || !root || !facts) return;

  const initial: Position = config.selection ?? [0, 0];
  const actions = new Map(config.actions.map((action) => [action.id, action]));
  const controls = config.actions.map(({ id, label }) => ({ command: id, label }));
  const view = new EditorView(root, "Bob");
  const remote = peer && new EditorView(peer, "Alice");
  const updateToolbar = controls.length ? attachDocsToolbar(root, controls, activate) : undefined;
  const groups = new Map(Array.from(form.querySelectorAll<HTMLElement>("[data-fields]"), (group) => [group.dataset.fields, group]));
  const engine = new Editor(slug, config.source, ...initial);
  let selection: Position = initial;
  let remoteSelection: Position = [1, 1];

  function remember() {
    selection = view.selection() ?? selection;
    remoteSelection = remote?.selection() ?? remoteSelection;
  }

  function markButtons() {
    for (const action of config.actions) {
      if (action.mark) updateToolbar?.(action.id, view.hasMark(action.mark, selection));
    }
  }

  const draw = (focus?: EditorView) => {
    const result: unknown = JSON.parse(engine.snapshot());
    if (!isObject(result) || !isPosition(result.selection)) throw new Error("Invalid editor snapshot.");
    selection = result.selection;
    view.draw(result.document, {
      caret: typeof result.bob_caret === "number" ? result.bob_caret : undefined,
      presentation: isObject(result.presentation) ? result.presentation : undefined,
      search: isSearchResults(result.search) ? result.search : undefined,
    });
    if (remote && result.peer) {
      if (isPosition(result.peer_selection)) remoteSelection = result.peer_selection;
      remote.draw(result.peer, { caret: typeof result.alice_caret === "number" ? result.alice_caret : undefined });
    }
    const entries = items(result.facts).filter(isFact);
    facts.hidden = entries.length === 0;
    facts.replaceChildren(...entries.flatMap(([key, value]) => [element("dt", key), element("dd", value)]));
    if (focus) {
      focus.root.focus({ preventScroll: true });
      focus.restore(focus === view ? selection : remoteSelection);
    }
    markButtons();
  };

  function perform(operation: () => void, focus?: EditorView) {
    try {
      operation();
      draw(focus);
      report("");
    } catch (error) {
      report(error instanceof Error ? error.message : String(error), true);
      if (focus) draw(focus);
    }
  }

  function edit(editor: EditorView, text: string, kind: InputKind, range?: Position) {
    if (editor === view) {
      engine.select(...(range ?? view.selection() ?? selection));
      engine.input(text, kind);
    } else {
      engine.peer_input(...(range ?? remote?.selection() ?? remoteSelection), text, kind);
    }
  }

  function run(command: string, fields: CommandArgs = {}) {
    perform(() => {
      engine.select(...selection);
      engine.action(JSON.stringify({ ...fields, action: command }));
    }, view);
  }

  function activate(command: string) {
    remember();
    const action = actions.get(command);
    if (!action) return;
    const group = groups.get(action.fieldsFrom ?? action.id);
    const args = group ? readArguments(group) : action.args ?? {};
    if (args) run(action.command, args);
  }

  for (const [id, group] of groups) {
    group.addEventListener("keydown", (event) => {
      if (id && event.key === "Enter" && !event.isComposing && event.target instanceof HTMLInputElement) {
        event.preventDefault();
        activate(id);
      }
    });
  }

  function bind(editor: EditorView) {
    const editable = editor.root;
    const history = (redo: boolean) => perform(() => {
      if (editor === view) engine.history(redo);
      else engine.peer_history(redo);
    }, editor);
    editable.contentEditable = "true";
    editable.spellcheck = false;
    let composing: Position | undefined;
    editable.addEventListener("compositionstart", () => {
      composing = editor.selection() ?? (editor === view ? selection : remoteSelection);
    });
    editable.addEventListener("compositionend", (event) => {
      const range = composing;
      composing = undefined;
      if (!range) return;
      if (event.data) perform(() => edit(editor, event.data, "text", range), editor);
      else {
        draw(editor);
        editor.restore(range);
      }
    });
    editable.addEventListener("beforeinput", (event) => {
      if (!(event instanceof InputEvent) || event.isComposing || composing) return;
      event.preventDefault();
      const range = editor.selection() ?? (editor === view ? selection : remoteSelection);
      const kind = event.inputType;
      if (kind === "insertText" || kind === "insertReplacementText") {
        if (event.data !== null) perform(() => edit(editor, event.data ?? "", "text", range), editor);
      } else if (kind === "insertParagraph" || kind === "insertLineBreak") {
        perform(() => edit(editor, "", kind === "insertParagraph" ? "paragraphBreak" : "lineBreak", range), editor);
      } else if (kind.startsWith("delete")) {
        const [from, to] = range;
        const adjacent = editor.adjacent(from, kind.endsWith("Forward"));
        if (from !== to || Number.isFinite(adjacent)) {
          perform(() => edit(editor, "", "text", from === to ? [from, adjacent] : range), editor);
        }
      } else if (kind === "historyUndo" || kind === "historyRedo") {
        history(kind === "historyRedo");
      }
    });
    editable.addEventListener("paste", (event) => {
      event.preventDefault();
      const text = event.clipboardData?.getData("text/plain");
      if (text) perform(() => edit(editor, text, "paste"), editor);
    });
    editable.addEventListener("drop", (event) => event.preventDefault());
    editable.addEventListener("keydown", (event) => {
      if (event.isComposing || composing || event.target !== editable) return;
      const primary = event.metaKey || event.ctrlKey;
      if (primary && event.key.toLowerCase() === "z") {
        event.preventDefault();
        history(event.shiftKey);
        return;
      }
      if (editor !== view) return;
      if (!(primary || event.altKey || ["Backspace", "Delete", "Enter", "Tab"].includes(event.key))) return;
      try {
        engine.select(...(view.selection() ?? selection));
        if (engine.shortcut(event.key, primary, event.shiftKey, event.altKey)) {
          event.preventDefault();
          draw(editor);
          report("");
        }
      } catch (error) {
        event.preventDefault();
        report(error instanceof Error ? error.message : String(error), true);
      }
    });
  }

  bind(view);
  if (remote) bind(remote);
  root.addEventListener("change", (event) => {
    if (!(event.target instanceof HTMLInputElement) || !event.target.dataset.checkTask) return;
    const position = Number(event.target.dataset.checkTask);
    run("checkTask", { position });
  });
  root.addEventListener("click", (event) => {
    if (event.target instanceof Element && event.target.closest("a") && !(event.metaKey || event.ctrlKey)) {
      const link = event.target.closest("a");
      if (link?.target !== "_blank") event.preventDefault();
    }
  });
  if (slug === "collaboration-caret" && remote) {
    for (const [sender, receiver] of [[view, remote], [remote, view]] as const) {
      const fromBob = sender === remote;
      function publish(focused: boolean) {
        const range = sender.selection();
        if (focused && !range) return;
        try {
          const [anchor, head] = range ?? [0, 0];
          engine.publish_caret(fromBob, anchor, head, focused);
          const result: unknown = JSON.parse(engine.snapshot());
          if (!isObject(result)) return;
          const caret = fromBob ? result.bob_caret : result.alice_caret;
          receiver.draw(fromBob ? result.document : result.peer, { caret: typeof caret === "number" ? caret : undefined });
        } catch (error) {
          report(error instanceof Error ? error.message : String(error), true);
        }
      }
      sender.root.addEventListener("focus", () => publish(true));
      sender.root.addEventListener("blur", () => publish(false));
      document.addEventListener("selectionchange", () => {
        if (document.activeElement === sender.root) publish(true);
      });
    }
  }
  document.addEventListener("selectionchange", () => {
    remember();
    markButtons();
  });
  form.addEventListener("submit", (event) => event.preventDefault());
  draw();
}
