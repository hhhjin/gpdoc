import {
  TextStyle, Color, BackgroundColor, FontFamily, FontSize, LineHeight, TextStyleKit,
} from '@tiptap/extension-text-style'
import { TextAlign } from '@tiptap/extension-text-align'

export const plugins = [
  [TextStyle, 'TextStyle'], [Color, 'Color'], [BackgroundColor, 'BackgroundColor'],
  [FontFamily, 'FontFamily'], [FontSize, 'FontSize'], [LineHeight, 'LineHeight'],
  [TextStyleKit, 'TextStyleKit'], [TextAlign.configure({ types: ['paragraph', 'heading'] }), 'TextAlign'],
]

const text = (text, attrs) => ({ type: 'text', text, ...(attrs && { marks: [{ type: 'textStyle', attrs }] }) })
const para = (...content) => ({ type: 'paragraph', content })
const doc = (...content) => ({ type: 'doc', content })
const select = (from, to = from) => ({ op: 'select', from, to })
const command = (name, args, params) => ({ op: 'command', name, args, params })
const stringCommand = (name, key, value) => command(name, { [key]: value }, [value])

export const cases = [
  ...[['color', 'Color', '#ff1234'], ['backgroundColor', 'BackgroundColor', '#357'],
    ['fontFamily', 'FontFamily', 'Georgia'], ['fontSize', 'FontSize', '16pt'],
    ['lineHeight', 'LineHeight', '1.6']].map(([key, name, value]) => ({
    name: `set and clear ${key} on selected text`, plugins: ['textStyle', key],
    doc: doc(para(text('한글😀'))), steps: [select(1, 3), stringCommand(`set${name}`, key, value), command(`unset${name}`)],
  })),
  { name: 'merge styles across different text runs', plugins: ['textStyleKit', 'undoRedo'],
    doc: doc(para(text('one', { fontSize: '24px' }), text('two', { backgroundColor: 'red' }), text('three'))),
    steps: [select(2, 9), stringCommand('setColor', 'color', '#112233'),
      command('unsetBackgroundColor'), command('unsetColor'), { op: 'undo' }, { op: 'redo' }] },
  { name: 'compose typing styles and toggle by attribute', plugins: ['textStyleKit'],
    doc: doc(para(text('a'))), steps: [select(2), stringCommand('setColor', 'color', 'red'),
      stringCommand('setFontSize', 'fontSize', '2em'), { op: 'insert', text: '한😀' },
      select(2, 5), command('toggleTextStyle', { color: 'blue' }), command('toggleTextStyle', { color: 'blue' })] },
  { name: 'clearing the final property removes empty styles adjoining the selected range', plugins: ['textStyle', 'color'],
    doc: doc(para(text('one', { color: null }), text('two', { color: 'red' }), text('three', { color: null }))),
    steps: [select(4, 7), command('unsetColor')] },
  { name: 'clearing a typing style inside a styled run', plugins: ['textStyle', 'color'],
    doc: doc(para(text('one', { color: 'red' }))), steps: [select(2), command('unsetColor'), { op: 'insert', text: 'x' }] },
  { name: 'remove only empty style marks', plugins: ['textStyle', 'color'],
    doc: doc(para(text('normal', {}), text('red', { color: 'red' }))),
    steps: [select(2, 9), command('removeEmptyTextStyle')] },
  { name: 'block alignment across heading and paragraph', plugins: ['heading', 'textAlign'],
    doc: doc({ type: 'heading', attrs: { level: 2 }, content: [text('Title')] }, para(text('body'))),
    steps: [select(2, 9), stringCommand('setTextAlign', 'textAlign', 'center'),
      stringCommand('toggleTextAlign', 'textAlign', 'center'), select(3),
      stringCommand('setTextAlign', 'textAlign', 'right'), stringCommand('setTextAlign', 'textAlign', 'invalid'),
      command('unsetTextAlign')] },
]
