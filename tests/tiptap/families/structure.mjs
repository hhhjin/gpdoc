import { Table, TableCell, TableHeader, TableRow, TableKit } from '@tiptap/extension-table'
import { ListKeymap, ListKit, TaskItem, TaskList } from '@tiptap/extension-list'

export const plugins = [
  [Table, 'Table'], [TableCell, 'TableCell'], [TableHeader, 'TableHeader'], [TableRow, 'TableRow'],
  [TableKit, 'TableKit'], [TaskItem, 'TaskItem'], [TaskList, 'TaskList'],
  [ListKeymap, 'ListKeymap'], [ListKit, 'ListKit'],
]

const doc = (...content) => ({ type: 'doc', content })
const paragraph = (value = '') => ({
  type: 'paragraph', content: value ? [{ type: 'text', text: value }] : [],
})
const command = (name, args) => ({ op: 'command', name, ...(args ? { args } : {}) })
const select = (from, to = from) => ({ op: 'select', from, to })
const cell = (value, attrs = {}) => ({ type: 'tableCell', attrs, content: [paragraph(value)] })
const row = (...content) => ({ type: 'tableRow', content })
const table = (...content) => ({ type: 'table', content })
const task = (value, checked = false) => ({ type: 'taskItem', attrs: { checked }, content: [paragraph(value)] })

export const cases = [
  ...[['--', '-'], ['—', '-'], ['___', ' '], ['***', ' ']].map(([prefix, key]) => ({
    name: `horizontal rule input marker ${prefix}${key}`, plugins: ['horizontalRule'],
    doc: doc(paragraph(prefix + 'after')), steps: [select(prefix.length + 1), { op: 'type', text: key }],
  })),
  { name: 'horizontal rule input in a list item', plugins: ['listKit', 'horizontalRule'],
    doc: doc({ type: 'bulletList', content: [{ type: 'listItem', content: [paragraph('--')] }] }),
    steps: [select(5), { op: 'type', text: '-' }] },
  { name: 'horizontal rule marker in code stays literal', plugins: ['codeBlock', 'horizontalRule'],
    doc: doc({ type: 'codeBlock', content: [{ type: 'text', text: '--' }] }),
    steps: [select(3), { op: 'type', text: '-' }] },
  ...[[2,14], [14,2]].map(([anchorCell, headCell]) => ({
    name: `horizontal rule in the head of a rectangular cell selection at ${headCell}`, plugins: ['tableKit', 'horizontalRule'],
    doc: doc(table(row(cell('A'),cell('B')),row(cell('C'),cell('D')))),
    steps: [command('setCellSelection', {anchorCell, headCell}), command('setHorizontalRule')],
  })),
  ...[['bulletList', '-'], ['orderedList', '3.'], ['taskList', '[x]']].map(([list,prefix]) => ({
    name: `type a ${list} prefix`, plugins:['listKit'], doc:doc(paragraph(prefix)), steps:[select(prefix.length + 1),{op:'type',text:' '}],
  })),
  ...['toggleBulletList','toggleOrderedList','toggleTaskList'].map(name=>({
    name: `${name} on a range of list items`, plugins:['listKit'],
    doc:doc({type:'bulletList',content:['A','B','C'].map(t=>({type:'listItem',content:[paragraph(t)]}))}),
    steps:[select(3,8),command(name)],
  })),
  { name: 'task list checks and text', plugins: ['taskItem', 'taskList', 'listKeymap'],
    doc: doc({type:'taskList',content:[task('buy milk',true),task('next')]}),
    steps: [select(4), { op:'insert',text:'!' }] },
  { name: 'toggle a task list on and off', plugins: ['listKit'], doc: doc(paragraph('hello')),
    steps: [select(2), command('toggleTaskList'), command('toggleTaskList')] },
  { name: 'table spans and column widths', plugins: ['table','tableRow','tableHeader','tableCell'],
    doc: doc(table(row(cell('한',{rowspan:2,colwidth:[140]}),cell('top')),
      row({type:'tableHeader',attrs:{align:'right'},content:[paragraph('next')]}))) },
  { name:'column edits maintain widths across a spanning header', plugins:['tableKit'],
    doc:doc(table(row(cell('AB',{colspan:2,colwidth:[80,120]})),row(cell('A',{colwidth:[80]}),cell('B',{colwidth:[120]})))),
    steps:[select(13),command('addColumnAfter'),command('deleteColumn')] },
  { name: 'merge a rectangle and split it back into separate cells', plugins:['tableKit'],
    doc:doc(table(row(cell('A'),cell('B')),row(cell('C'),cell('D')))),
    steps:[command('setCellSelection',{anchorCell:2,headCell:19}),command('mergeCells'),command('splitCell')] },
  { name: 'clear the selected table column by typing', plugins:['tableKit'],
    doc:doc(table(row(cell('A'),cell('B')),row(cell('C'),cell('D')))),
    steps:[command('setCellSelection',{anchorCell:2,headCell:14}), {op:'type',text:'X'}] },
  ...[1, 2, 4].map(at => ({ name:`insert a table at ${at} in text`,plugins:['tableKit'],
    doc:doc(paragraph('abc')),steps:[select(at),command('insertTable',{rows:1,cols:1,withHeaderRow:false})] })),
  { name:'repair a short table row after editing',plugins:['tableKit'],doc:doc(table(row(cell('A'),cell('B')),row(cell('C')))),
    steps:[select(4),{op:'type',text:'z'},command('fixTables')] },
  { name: 'insert a table, add rows and columns and navigate cells', plugins: ['tableKit'],
    doc: doc(paragraph()), steps: [command('insertTable',{rows:2,cols:2}),command('addColumnAfter'),
      command('addRowAfter'),command('goToNextCell'),command('toggleHeaderRow'),command('toggleHeaderColumn'),
      command('goToPreviousCell'),command('deleteRow'),command('deleteColumn'),command('deleteTable')] },
]
