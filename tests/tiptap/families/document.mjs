import { CharacterCount, TrailingNode } from '@tiptap/extensions'
import { Typography } from '@tiptap/extension-typography'
import { FindAndReplace } from '@tiptap/extension-find-and-replace'
import { UniqueID } from '@tiptap/extension-unique-id'
import { TableOfContents } from '@tiptap/extension-table-of-contents'

export const plugins = [
  [CharacterCount.configure({ limit: 5 }), 'CharacterCount'],
  [Typography, 'Typography'],
  [FindAndReplace.configure({ searchDebounceMs: 0, continuesNode: node => node.type.name === 'heading' }), 'FindAndReplace'],
  [UniqueID.configure({ types: ['paragraph'], generateID: ({ pos }) => `node-${pos}` }), 'UniqueID'],
  [TableOfContents.configure({ scrollParent: undefined, getId: text => text }), 'TableOfContents'],
  [TrailingNode, 'TrailingNode'],
]

const text = text => ({type: 'text', text})
const paragraph = (...content) => ({type: 'paragraph', content})
const doc = (...content) => ({type: 'doc', content})
const command = (name, args, params) => ({op:'command', name, ...(args && {args, params})})

export const cases = [
  {name:'paste multiple marked spans',plugins:['bold','italic'],doc:doc(paragraph()),
    steps:[{op:'paste',text:'**one** and _two_ and __three__'}]},
  {name:'paste URL on selected text',plugins:['link'],doc:doc(paragraph(text('read here'))),
    steps:[{op:'select',from:6,to:10},{op:'paste',text:'https://example.org'}]},
  {name:'trim pasted characters to a limit',plugins:['characterCount'],doc:doc(paragraph(text('abc'))),
    steps:[{op:'paste',text:'xyz'}]},
  {name:'convert heading creates a table of contents anchor',plugins:['heading','tableOfContents'],doc:doc(paragraph(text('Chapter'))),
    steps:[command('setHeading',{level:2})]},
  {name:'new trailing paragraph receives a unique ID',plugins:['uniqueID','horizontalRule'],doc:doc({...paragraph(text('end')),attrs:{id:'old'}}),
    steps:[{op:'select',from:4,to:4},command('setHorizontalRule')]},
  {name:'updating a link preserves its title',plugins:['link'],doc:doc(paragraph(text('first'))),
    steps:[{op:'select',from:1,to:6},command('setLink',{href:'https://first.example',title:'saved'}),command('setLink',{href:'/relative'})]},
  ...[['bold','**'],['italic','_'],['strike','~~'],['highlight','=='],['code','`']].map(([mark,delimiter])=>{
    const prefix = '😀 ' + delimiter + 'word' + delimiter.slice(1)
    return {name:`type ${mark} delimiters and continue unmarked`,plugins:[mark],doc:doc(paragraph(text(prefix))),
      steps:[{op:'select',from:prefix.length+1,to:prefix.length+1},{op:'type',text:delimiter[0]}, {op:'type',text:'!'}]}
  }),
  {name:'autolink a parenthesized URL after an emoji',plugins:['link'],
    doc:doc(paragraph(text('😀 (example.org)'))),steps:[{op:'select',from:17,to:17},{op:'type',text:' '}]},
  {name:'link defaults, validation and unlink at a cursor',plugins:['link'],
    doc:doc(paragraph(text('site'))),steps:[{op:'select',from:1,to:5},command('setLink',{href:'https://example.org'}),
      command('setLink',{href:'javascript:doNothing()'}),{op:'select',from:3,to:3},command('unsetLink')]},
  {name: 'UTF-16 character limit declines the next character', plugins:['characterCount'],
    doc:doc(paragraph(text('A😀B'))), steps:[{op:'insert', text:'x'}, {op:'insert', text:'y'}]},
  {name:'smart punctuation keeps the prefix and fraction separator', plugins:['typography'],
    doc:doc(paragraph(text('😀-'))), steps:[{op:'select', from:4, to:4}, {op:'type', text:'-'},
      {op:'type',text:' '}, {op:'type',text:'1'}, {op:'type',text:'/'}, {op:'type',text:'2'}, {op:'type',text:' '}]},
  {name:'search replaces a match spanning continued textblocks', plugins:['findAndReplace','heading'],
    doc:doc(paragraph(text('😀 Wor')), {type:'heading',attrs:{level:2},content:[text('ld!')]}),
    steps:[command('setSearchTerm',{term:'world'},['world']),command('goToNextResult'),
      command('setReplaceTerm',{term:'Moon'},['Moon']),command('replace')]},
  {name:'search navigation and replacement across marks', plugins:['findAndReplace','bold'],
    doc:doc(paragraph(text('he'), {...text('llo'), marks:[{type:'bold'}]}, text(' HELLO'))),
    steps:[command('setSearchTerm',{term:'hello'},['hello']),command('goToNextResult'),command('goToNextResult'),
      command('setReplaceTerm',{term:'abc'},['abc']), command('replace'),command('replaceAll'),command('clearSearch')]},
  {name:'unique IDs survive editing text', plugins:['uniqueID'],
    doc:doc({...paragraph(text('a')),attrs:{id:'existing'}}), steps:[{op:'insert',text:'x'}]},
  {name:'table of contents preserves existing heading anchor', plugins:['heading','tableOfContents'],
    doc:doc({type:'heading',attrs:{level:2,id:'existing','data-toc-id':'existing'},content:[text('Heading')]}),
    steps:[command('updateTableOfContents'),{op:'insert',text:'x'}]},
  {name:'heading edit appends trailing paragraph', plugins:['heading','trailingNode'],
    doc:doc({type:'heading',attrs:{level:2},content:[text('Heading')]}), steps:[{op:'insert',text:'x'}]},
]
