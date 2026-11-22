import assert from 'node:assert/strict'
import { JSDOM } from 'jsdom'

const { window } = new JSDOM('<!doctype html><html><body></body></html>', { url: 'https://example.org/', pretendToBeVisual: true })
for (const key of ['window', 'document', 'navigator', 'MutationObserver', 'getComputedStyle', 'Node', 'Element', 'HTMLElement', 'DOMParser', 'Event', 'CustomEvent', 'KeyboardEvent']) {
  Object.defineProperty(globalThis, key, { configurable: true, value: window[key] })
}

// ProseMirror detects the DOM at module initialization.
const { Editor } = await import('@tiptap/core')
const { base, plugins, resolvePlugins } = await import('./extensions.mjs')
export { resolvePlugins }

export class InvalidContent extends Error {}

export function withEditor(names, doc, read) {
  const editor = new Editor({
    extensions: [...base, ...resolvePlugins(names).map(p => p.extension)],
    content: doc,
    enableContentCheck: true,
    onContentError: ({ error }) => {
      throw new InvalidContent(error.cause?.message ?? error.message, { cause: error })
    },
  })
  try {
    editor.state.doc.check()
    return read(editor)
  } finally {
    editor.destroy()
  }
}

function apply(editor, step) {
  switch (step.op) {
    case 'select': return editor.commands.setTextSelection({ from: step.from, to: step.to })
    case 'paste': return editor.view.pasteText(step.text, new window.Event('paste'))
    case 'type': {
      const { from, to } = editor.state.selection
      const input = step.text
      const nativeInput = () => editor.state.tr.insertText(input)
      if (!editor.view.someProp('handleTextInput', handler => handler(editor.view, from, to, input, nativeInput))) {
        editor.view.dispatch(nativeInput())
      }
      return true
    }
    case 'insert': return editor.commands.insertContent({ type: 'text', text: step.text })
    case 'command': return editor.commands[step.name](...(step.params ?? (step.args ? [step.args] : [])))
    case 'key': {
      const mac = /Mac|iP(hone|[oa]d)/.test(window.navigator.platform)
      const event = new window.KeyboardEvent('keydown', {
        key: step.key, ctrlKey: step.primary && !mac, metaKey: step.primary && mac, shiftKey: step.shift,
      })
      return !!editor.view.someProp('handleKeyDown', handler => handler(editor.view, event))
    }
    case 'setSectionTitle': return editor.commands.setSectionTitle()
    case 'toggleBold': return editor.commands.toggleBold()
    case 'setHighlight': return editor.commands.setHighlight?.() ?? false
    case 'undo': return editor.commands.undo()
    case 'redo': return editor.commands.redo()
    default: throw new Error(`Unknown Tiptap operation: ${step.op}`)
  }
}

export function reference({ plugins = [], doc, steps = [] }) {
  return withEditor(plugins, doc, editor => {
    const snapshot = applied => ({
      doc: editor.getJSON(), selection: editor.state.selection.toJSON(), applied,
    })
    const states = [snapshot(true)]
    for (const step of steps) {
      const applied = apply(editor, step)
      editor.state.doc.check()
      states.push(snapshot(applied))
    }
    return { states, nodes: Object.keys(editor.schema.nodes), marks: Object.keys(editor.schema.marks) }
  })
}

// A change to the reference schema must come with at least one document using each type.
export function checkCoverage(cases, references) {
  const nodes = new Set()
  const marks = new Set()
  function visit(node) {
    nodes.add(node.type)
    for (const mark of node.marks ?? []) marks.add(mark.type)
    for (const child of node.content ?? []) visit(child)
  }
  for (const { states } of references) {
    for (const state of states) visit(state.doc)
  }
  assert.deepEqual(nodes, new Set(references.flatMap(ref => ref.nodes)), 'untested reference nodes')
  assert.deepEqual(marks, new Set(references.flatMap(ref => ref.marks)), 'untested reference marks')
  assert.deepEqual(new Set(cases.flatMap(c => c.plugins ?? [])), new Set(plugins.keys()), 'untested plugins')
}
