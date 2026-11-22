import assert from 'node:assert/strict'
import { execFileSync } from 'node:child_process'
import { getSchema } from '@tiptap/core'
import { Document } from '@tiptap/extension-document'
import { Paragraph } from '@tiptap/extension-paragraph'
import { Text } from '@tiptap/extension-text'
import { Heading } from '@tiptap/extension-heading'
import { Blockquote } from '@tiptap/extension-blockquote'
import { Bold } from '@tiptap/extension-bold'
import { Italic } from '@tiptap/extension-italic'
import { Link } from '@tiptap/extension-link'
import { Code } from '@tiptap/extension-code'
import { CodeBlock } from '@tiptap/extension-code-block'
import { Strike } from '@tiptap/extension-strike'
import { HorizontalRule } from '@tiptap/extension-horizontal-rule'
import { MarkdownManager } from '@tiptap/markdown'

import { OrderedList, ListItem, TaskItem, TaskList } from '@tiptap/extension-list'
import { TableKit } from '@tiptap/extension-table'
import { Mathematics } from '@tiptap/extension-mathematics'
import { Image } from '@tiptap/extension-image'

const extensions = [Document, Paragraph, Text, Heading, Blockquote, OrderedList, ListItem, Bold, Italic, Link, Code, CodeBlock, Strike, HorizontalRule, TaskItem, TaskList, TableKit, Image, Mathematics]
const manager = new MarkdownManager({ extensions })
const schema = getSchema(extensions)
const parse = source => schema.nodeFromJSON(manager.parse(source)).toJSON()
const inputs = [
  '## Hello 😀\n\n> **ABC** and *한글*.\n\nDone.',
  '3. First\n4. Second\n\nEnd.',
  '```rust\nfn hello() {}\n```\n\nAn inline `code` span.',
  '~~Strike~~ and **bold**.',
  '[Link](https://example.org "Example").',
  '- [x] complete\n- [ ] next\n\nEnd.',
  '| Left | Right |\n| :--- | ---: |\n| **A** | B |\n\nEnd.',
  'Energy $E=mc^2$.\n\n$$x=y$$\n\nEnd.',
  '![Example](/example.png "Image")\n\nEnd.',
]
const result = JSON.parse(execFileSync('cargo', ['run', '--quiet', '--locked', '-p', 'gpdoc-plugin-markdown',
  '--example', 'tiptap_markdown'], { cwd: new URL('../../', import.meta.url), input: JSON.stringify(inputs) }))
for (const [i, source] of inputs.entries()) {
  assert.deepEqual(result[i].doc, parse(source), source)
  assert.deepEqual(parse(result[i].markdown), result[i].doc, result[i].markdown)
}
console.log(`PASS  ${inputs.length} Tiptap Markdown import and export cases`)
