import { cases as documentCases } from './families/document.mjs'
import { cases as structureCases } from './families/structure.mjs'
import { cases as blocksCases } from './families/blocks.mjs'
import { cases as stylesCases } from './families/styles.mjs'
import { cases as mediaCases } from './families/media.mjs'
const text = (value, ...marks) => ({ type: 'text', text: value, marks })
const paragraph = (...content) => ({ type: 'paragraph', content })
const item = (...content) => ({ type: 'listItem', content })
const mark = type => ({ type })
const doc = (...content) => ({ type: 'doc', content })
const select = (from, to = from) => ({ op: 'select', from, to })
const insert = text => ({ op: 'insert', text })

const command = name => ({ op: 'command', name })
const sectionTitleShortcut = { op: 'key', key: '2', primary: true, shift: true }

// Both editors receive the original seed, including any omitted attributes.
export const cases = [
  ...structureCases,
  ...documentCases,
  ...blocksCases,
  { name: 'compose table, media and style extensions',
    plugins: ['bold','tableKit','textStyleKit','textAlign','taskItem','taskList','mention','Mathematics','image','highlight'],
    doc: doc({type:'taskList',content:[{type:'taskItem',content:[paragraph(text('한글👋'))]}]},
      {type:'table',content:[{type:'tableRow',content:[{type:'tableCell',content:[paragraph(text('cell'))]}]}]},
      {type:'image',attrs:{src:'local.png'}},paragraph(text('last'))),
    steps: [select(3,5),command('setBold'),{op:'command',name:'setColor',args:{color:'#730'},params:['#730']},
      {op:'command',name:'setTextAlign',args:{textAlign:'center'},params:['center']}] },
  ...stylesCases,
  ...mediaCases,

  { name: 'edit list text and record history', plugins: ['bulletList', 'listItem', 'bold', 'italic', 'undoRedo'],
    doc: doc({ type: 'bulletList', content: [item(paragraph(text('one')))] }, paragraph()),
    steps: [select(3, 6), command('toggleBold'), command('toggleItalic'), { op: 'undo' }, { op: 'redo' }] },
  ...['Subscript', 'Superscript'].map(name => ({
    name: `${name} commands on a range and at the caret`, plugins: [name.toLowerCase()],
    doc: doc(paragraph(text('H2O'))), steps: [select(2, 3), command(`set${name}`), command(`unset${name}`),
      command(`toggle${name}`), select(4), command(`toggle${name}`), insert('2')],
  })),
  { name: 'custom section title command and history', plugins: ['sectionTitle', 'heading', 'bold', 'undoRedo'],
    doc: doc(paragraph(text('한😀글'))), steps: [select(1, 4), { op: 'setSectionTitle' }, { op: 'undo' }, { op: 'redo' }, { op: 'undo' }, sectionTitleShortcut] },
  { name: 'derived extension uses configured options, inherited commands and a new keymap',
    plugins: ['articleTitle', 'heading', 'bold', 'undoRedo'], doc: doc(paragraph(text('Title'))),
    steps: [select(1, 6), sectionTitleShortcut, { op: 'key', key: 'F7' }, { op: 'undo' }, command('setSectionTitle')] },
  { name: 'empty paragraph', doc: doc(paragraph()) },
  { name: 'multilingual text', doc: doc(paragraph(text('한글 👩🏽‍💻 e\u0301')), paragraph(text('مرحبا'))) },
  { name: 'heading levels', plugins: ['heading'], doc: doc(...[1, 2, 3, 4, 5, 6].map(level =>
    ({ type: 'heading', attrs: { level }, content: [text(String(level))] }))) },
  { name: 'nested quotes', plugins: ['blockquote'], doc: doc({ type: 'blockquote', content: [
    paragraph(text('outer')), { type: 'blockquote', content: [paragraph(text('inner'))] },
  ] }) },
  ...['bold', 'italic', 'code', 'strike', 'underline'].map(type => ({
    name: type, plugins: [type], doc: doc(paragraph(text('before '), text('marked', mark(type)), text(' after'))),
  })),
  { name: 'combined marks', plugins: ['bold', 'italic'], doc: doc(paragraph(text('both', mark('italic'), mark('bold')))) },
  { name: 'link attributes', plugins: ['link'], doc: doc(paragraph(text('link', {
    type: 'link', attrs: { href: 'https://example.org/p#abc', target: '_self', rel: 'author', class: 'external' },
  }))) },
  { name: 'hard break', plugins: ['hardBreak'], doc: doc(paragraph(text('a'), { type: 'hardBreak' }, text('b'))) },
  { name: 'horizontal rule', plugins: ['horizontalRule'], doc: doc(paragraph(), { type: 'horizontalRule' }, paragraph()) },
  { name: 'code block language', plugins: ['codeBlock'], doc: doc({ type: 'codeBlock', attrs: { language: 'rust' },
    content: [text('fn main() {\n}\n')] }) },
  { name: 'ordered list start', plugins: ['listItem', 'orderedList'], doc: doc({ type: 'orderedList', attrs: { start: 3 },
    content: [item(paragraph(text('third')))] }) },
  { name: 'nested bullet lists', plugins: ['listItem', 'bulletList'], doc: doc({ type: 'bulletList', content: [
    item(paragraph(text('parent')), { type: 'bulletList', content: [item(paragraph(text('child')))] }),
  ] }) },
  { name: 'replace emoji and undo/redo', plugins: ['undoRedo'], doc: doc(paragraph(text('A😀B'))),
    steps: [select(2, 4), insert('한글'), { op: 'undo' }, { op: 'redo' }] },
  { name: 'replace across paragraphs', doc: doc(paragraph(text('ABC')), paragraph(text('DEF'))),
    steps: [select(2, 8), insert('!')] },
  { name: 'toggle bold on selection', plugins: ['bold'], doc: doc(paragraph(text('abc'))),
    steps: [select(1, 3), { op: 'toggleBold' }] },
  { name: 'toggle bold before typing', plugins: ['bold'], doc: doc(paragraph()),
    steps: [{ op: 'toggleBold' }, insert('굵게')] },
  { name: 'compose typing marks and turn one off inside a marked run', plugins: ['bold', 'underline', 'strike'],
    doc: doc(paragraph(text('ab', mark('bold')))), steps: [select(2), command('toggleBold'), command('toggleUnderline'),
      command('toggleStrike'), insert('한'), command('unsetUnderline'), insert('😀')] },
  { name: 'moving the caret clears pending marks', plugins: ['bold'], doc: doc(paragraph(text('ab'))),
    steps: [command('toggleBold'), select(3), insert('plain')] },
  { name: 'keep typing marks across a hard break', plugins: ['bold', 'hardBreak', 'undoRedo'],
    doc: doc(paragraph(text('ab'))), steps: [select(2), command('toggleBold'), command('setHardBreak'), insert('한')] },
  ...[1, 2, 4].map(at => ({ name: `insert horizontal rule at position ${at}`, plugins: ['horizontalRule', 'undoRedo'],
    doc: doc(paragraph(text('abc'))), steps: [select(at), command('setHorizontalRule'), { op: 'undo' }, { op: 'redo' }] })),
  { name: 'highlight selection', plugins: ['highlight'], doc: doc(paragraph(text('one two'))),
    steps: [select(5, 8), { op: 'setHighlight' }] },
  { name: 'highlight in heading with undo', plugins: ['heading', 'highlight', 'undoRedo'],
    doc: doc({ type: 'heading', attrs: { level: 2 }, content: [text('제목')] }),
    steps: [select(1, 3), { op: 'setHighlight' }, { op: 'undo' }, { op: 'redo' }] },
  { name: 'highlight command absent', without: ['highlight'], doc: doc(paragraph(text('plain'))),
    steps: [select(1, 6), { op: 'setHighlight' }] },
]
