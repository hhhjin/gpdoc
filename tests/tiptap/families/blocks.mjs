import { Details, DetailsContent, DetailsSummary } from '@tiptap/extension-details'
import { CodeBlockLowlight } from '@tiptap/extension-code-block-lowlight'
import { common, createLowlight } from 'lowlight'

export const plugins = [
  [Details, 'Details'], [DetailsSummary, 'DetailsSummary'], [DetailsContent, 'DetailsContent'],
  [CodeBlockLowlight.configure({ lowlight: createLowlight(common) }), 'CodeBlockLowlight', 'codeBlockLowlight'],
]

const p = (...content) => ({type:'paragraph',content})
const q = (...content) => ({type:'blockquote',content})
const t = (text, marks = []) => ({type:'text',text,marks})
const quoteMarker = (at) => [{op:'select',from:at,to:at},{op:'type',text:' '}]
export const cases = [
  {name:'wrap indented quote marker before formatted suffix',plugins:['blockquote','bold'],
    doc:{type:'doc',content:[p(t('  >'),t('😀rest',[{type:'bold'}]))]},steps:quoteMarker(4)},
  {name:'join consecutive quote inside an existing quote',plugins:['blockquote'],
    doc:{type:'doc',content:[q(q(p(t('one'))),p(t('>next')))]},steps:quoteMarker(10)},
  {name:'keep quote markers literal in code',plugins:['blockquote','codeBlock','code'],
    doc:{type:'doc',content:[{type:'codeBlock',content:[t('>')]},p(t('>',[{type:'code'}]))]},
    steps:[...quoteMarker(2),...quoteMarker(6)]},

  {
    name: 'toggle heading level and decline invalid level', plugins: ['heading'],
    doc: { type: 'doc', content: [{ type: 'paragraph', content: [{ type: 'text', text: 'title' }] }] },
    steps: [2, 3, 3, 7].map(level => ({op:'command',name:'toggleHeading',args:{level}})),
  },
  {
    name: 'convert formatted content and line breaks to code and back', plugins: ['bold','hardBreak','codeBlock'],
    doc: {type:'doc',content:[{type:'paragraph',content:[{type:'text',text:'x',marks:[{type:'bold'}]},
      {type:'hardBreak'},{type:'text',text:'y'}]}]},
    steps: [{op:'command',name:'toggleCodeBlock',args:{language:'rust'}},{op:'command',name:'toggleCodeBlock'}],
  },
  {
    name: 'type a language fence', plugins: ['codeBlock'],
    doc: {type:'doc',content:[{type:'paragraph',content:[{type:'text',text:'~~~rust'}]}]},
    steps: [{op:'select',from:8,to:8},{op:'type',text:' '}],
  },
  {
    name: 'wrap paragraphs in details, write a summary and unwrap',
    plugins: ['details', 'detailsSummary', 'detailsContent'],
    doc: { type: 'doc', content: ['first', 'second'].map(text => ({
      type: 'paragraph', content: [{ type: 'text', text }],
    })) },
    steps: [{ op: 'select', from: 2, to: 9 }, { op: 'command', name: 'setDetails' },
      { op: 'insert', text: '요약' }, { op: 'command', name: 'unsetDetails' }],
  },
  {
    name: 'edit syntax highlighted code', plugins: ['codeBlockLowlight'],
    doc: { type: 'doc', content: [{ type: 'codeBlock', attrs: { language: 'rust' },
      content: [{ type: 'text', text: 'fn main() {}' }] }] },
    steps: [{ op: 'select', from: 4, to: 8 }, { op: 'insert', text: 'example' }],
  },
]
