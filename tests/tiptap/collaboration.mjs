import assert from 'node:assert/strict'
import { execFileSync } from 'node:child_process'
import { getSchema } from '@tiptap/core'
import { Document } from '@tiptap/extension-document'
import { Paragraph } from '@tiptap/extension-paragraph'
import { Text } from '@tiptap/extension-text'
import { Bold } from '@tiptap/extension-bold'
import { prosemirrorJSONToYXmlFragment, yXmlFragmentToProseMirrorRootNode } from '@tiptap/y-tiptap'
import * as Y from 'yjs'

const schema = getSchema([Document, Paragraph, Text, Bold])
const doc = new Y.Doc()
const fragment = doc.getXmlFragment('default')
prosemirrorJSONToYXmlFragment(schema, {
  type: 'doc', content: [{ type: 'paragraph', content: [
    { type: 'text', text: 'Hello', marks: [{ type: 'bold' }] }, { type: 'text', text: ' 😀' },
  ] }],
}, fragment)
const reply = execFileSync('cargo', ['run', '--quiet', '--locked', '-p', 'gpdoc-plugin-collaboration',
  '--example', 'tiptap_peer'], { cwd: new URL('../../', import.meta.url), input: Y.encodeStateAsUpdate(doc) })
Y.applyUpdate(doc, reply)
assert.deepEqual(yXmlFragmentToProseMirrorRootNode(fragment, schema).toJSON(), {
  type: 'doc', content: [{ type: 'paragraph', content: [{ type: 'text', text: 'Hello 😀!' }] }],
})
doc.destroy()
console.log('PASS  Tiptap → gpdoc → Tiptap collaboration update')
