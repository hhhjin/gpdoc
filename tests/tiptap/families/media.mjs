import Audio from '@tiptap/extension-audio'
import Image from '@tiptap/extension-image'
import Mathematics from '@tiptap/extension-mathematics'
import Mention from '@tiptap/extension-mention'
import RubyText from '@tiptap/extension-ruby-text'

export const plugins = [
  [Image, 'Image'], [Audio, 'Audio'],
  [Mention, 'Mention'], [Mathematics, 'Mathematics'], [RubyText, 'RubyText'],
]

const doc = (...content) => ({ type: 'doc', content })
const p = (...content) => ({ type: 'paragraph', content })
const text = text => ({ type: 'text', text })
const cmd = (name, args, params) => ({ op: 'command', name, args, ...(params ? { params } : {}) })
const select = (from, to = from) => ({op: 'select', from, to})

export const cases = [
  ...[
    ['image', 'setImage', {src: '/media/bird.png', width: 240, alt: 'a swallow'}],
    ['audio', 'setAudio', {src: '/media/stream.ogg', muted: true}],
  ].map(([type, command, attrs]) => ({
    name: `insert and undo ${type}`, plugins: [type, 'undoRedo'],
    doc: doc(p(text('ab'))), steps: [select(2), cmd(command, attrs), {op: 'undo'}, {op: 'redo'}],
  })),
  {name: 'replace an empty paragraph with an image', plugins: ['image'], doc: doc(p()),
    steps: [cmd('setImage', {src: '/media/cover.png'})]},
  {name: 'unsupported audio URL declines', plugins: ['audio'], doc: doc(p()),
    steps: [cmd('setAudio', {src: 'file:///music'})]},
  {name: 'mention embedded in text', plugins: ['mention'], doc: doc(p(text('hi '),
    {type: 'mention', attrs: {label: '민수', id: 'user-3'}}, text('!')))},
  {name: 'insert edit and delete a math expression', plugins: ['Mathematics'], doc: doc(p(text('ab'))),
    steps: [select(2), cmd('insertInlineMath', {latex: 'x^2'}), cmd('updateInlineMath', {pos:2, latex:'z'}),
      cmd('deleteInlineMath', {pos:2}), cmd('insertBlockMath', {latex:'x+y'})]},
  {name: 'annotate Japanese text', plugins: ['rubyText'], doc: doc(p(text('東京'))),
    steps: [select(1, 3),cmd('setRubyText',{rt:'とうきょう'}),select(2),cmd('unsetRubyText', {})]},
  {name: 'type an inline formula after an emoji', plugins: ['Mathematics'], doc: doc(p(text('😀 $$x$'))),
    steps: [select(8), {op:'type',text:'$'}]},
  {name: 'type a whole-block formula', plugins: ['Mathematics'], doc: doc(p(text('$$$x$$'))),
    steps: [select(7), {op:'type',text:'$'}]},
  {name: 'type a markdown image', plugins: ['image'], doc: doc(p(text('![표지](cover.png'))),
    steps: [select(16), {op:'type',text:')'}]},
  {name:'type and paste Unicode emoji as text',doc:doc(p(text('한글 '))),
    steps:[select(4),{op:'type',text:'😀😄🚀❤️'},{op:'paste',text:' 👩🏽‍💻'}]},
  {name: 'paste an audio URL', plugins: ['audio'], doc: doc(p()),
    steps: [{op: 'paste', text: 'https://example.org/tune.mp3'}]},
]
