import { plugins as documentPlugins } from './families/document.mjs'
import { plugins as structurePlugins } from './families/structure.mjs'
import { plugins as blocksPlugins } from './families/blocks.mjs'
import { plugins as stylesPlugins } from './families/styles.mjs'
import { plugins as mediaPlugins } from './families/media.mjs'
import { Extension } from '@tiptap/core'
import { Document } from '@tiptap/extension-document'
import { Text } from '@tiptap/extension-text'
import { Paragraph } from '@tiptap/extension-paragraph'
import { Blockquote } from '@tiptap/extension-blockquote'
import { Bold } from '@tiptap/extension-bold'
import { Code } from '@tiptap/extension-code'
import { CodeBlock } from '@tiptap/extension-code-block'
import { HardBreak } from '@tiptap/extension-hard-break'
import { Heading } from '@tiptap/extension-heading'
import { Highlight } from '@tiptap/extension-highlight'
import { HorizontalRule } from '@tiptap/extension-horizontal-rule'
import { Italic } from '@tiptap/extension-italic'
import { Link } from '@tiptap/extension-link'
import { BulletList, ListItem, OrderedList } from '@tiptap/extension-list'
import { Subscript } from '@tiptap/extension-subscript'
import { Superscript } from '@tiptap/extension-superscript'
import { Strike } from '@tiptap/extension-strike'
import { Underline } from '@tiptap/extension-underline'
import { UndoRedo } from '@tiptap/extensions'

// The app exposes one command that changes the block and marks the selected text.
const SectionTitle = Extension.create({
  name: 'sectionTitle',
  addOptions() { return { level: 2 } },
  addCommands() {
    return { setSectionTitle: () => ({ chain }) =>
      chain().setHeading({ level: this.options.level }).setBold().run() }
  },
  addKeyboardShortcuts() {
    return { 'Mod-Shift-2': () => this.editor.commands.setSectionTitle() }
  },
})

const ArticleTitle = SectionTitle.extend({
  addCommands() {
    return { ...this.parent(), applyTitle: () => ({ commands }) => commands.setSectionTitle() }
  },
  addKeyboardShortcuts() {
    return { F7: () => this.editor.commands.applyTitle() }
  },
}).configure({ level: 3 })

// Document and Text are intrinsic to the Rust editor. The driver installs Paragraph.
export const base = [Document, Paragraph, Text]
export const plugins = new Map([
  [Subscript, 'Subscript'], [Superscript, 'Superscript'],
  [SectionTitle, 'SectionTitle'], [ArticleTitle, 'ArticleTitle', 'articleTitle'], [Blockquote, 'Blockquote'], [Bold, 'Bold'], [Code, 'InlineCode'],
  [CodeBlock, 'CodeBlock'], [Heading, 'Heading'], [Highlight, 'Highlight'],
  [Italic, 'Italic'], [Link, 'Link'], [BulletList, 'BulletList'],
  [ListItem, 'ListItem'], [OrderedList, 'OrderedList'],
  [UndoRedo, null], // Local history lives in gpdoc-core.
  [HardBreak, 'HardBreak'], [HorizontalRule, 'HorizontalRule'], [Strike, 'Strike'], [Underline, 'Underline'],
  ...structurePlugins,
  ...documentPlugins,
  ...blocksPlugins,
  ...stylesPlugins,
  ...mediaPlugins,
].map(([extension, gpdoc, alias]) => [alias ?? extension.name, { extension, gpdoc }]))

export function resolvePlugins(names) {
  return names.map(name => {
    const plugin = plugins.get(name)
    if (!plugin) throw new Error(`Unknown test plugin: ${name}`)
    return plugin
  })
}
