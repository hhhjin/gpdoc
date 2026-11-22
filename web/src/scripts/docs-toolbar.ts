import { toolbar_icon } from "../generated/gpdoc-editor.js";

type ToolbarButton = { command: string; label: string };

export function attachDocsToolbar(root: HTMLElement, controls: ToolbarButton[], activate: (command: string) => void) {
  const bar = document.createElement("div");
  bar.className = "editor-toolbar";
  bar.setAttribute("role", "toolbar");
  bar.setAttribute("aria-label", "Editor toolbar");
  const buttons = new Map<string, HTMLButtonElement>();
  for (const { command, label } of controls) {
    const button = document.createElement("button");
    button.type = "button";
    button.title = label;
    button.setAttribute("aria-label", label);
    // The markup comes from the Lucide assets compiled into the editor.
    button.innerHTML = toolbar_icon(command);
    button.addEventListener("mousedown", (event) => event.preventDefault());
    button.addEventListener("click", () => activate(command));
    buttons.set(command, button);
    bar.append(button);
  }
  root.before(bar);

  return (command: string, pressed: boolean) => {
    buttons.get(command)?.setAttribute("aria-pressed", String(pressed));
  };
}
