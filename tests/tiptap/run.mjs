import assert from 'node:assert/strict'
import { execFileSync } from 'node:child_process'
import { readFileSync } from 'node:fs'
import { isDeepStrictEqual, parseArgs } from 'node:util'
import { checkCoverage, InvalidContent, reference, resolvePlugins, withEditor } from './oracle.mjs'
import { cases as corpus } from './cases.mjs'

const known = JSON.parse(readFileSync(new URL('./known-gaps.json', import.meta.url), 'utf8'))
const { values: { strict, plugin } } = parseArgs({ options: {
  strict: { type: 'boolean' }, plugin: { type: 'string' },
} })
const cases = plugin ? corpus.filter(c => ['paragraph', ...(c.plugins ?? []), ...(c.without ?? [])].includes(plugin)) : corpus
assert(cases.length, `No compatibility cases for plugin: ${plugin}`)
assert.equal(new Set(corpus.map(c => c.name)).size, corpus.length, 'duplicate case name')
for (const name of Object.keys(known)) {
  assert(corpus.some(c => c.name === name), `obsolete known gap: ${name}`)
}

// Reference errors and driver failures are infrastructure failures in both modes.
const expected = cases.map(reference)
if (!plugin) checkCoverage(cases, expected)
const input = cases.map(({ plugins = [], steps = [], doc }) => ({
  plugins: resolvePlugins(plugins).map(p => p.gpdoc).filter(name => name !== null),
  doc, steps,
}))
const actual = JSON.parse(execFileSync('cargo', [
  'run', '--locked', '--quiet', '-p', 'gpdoc-integration-tests', '--example', 'tiptap_compat',
], {
  cwd: new URL('../../', import.meta.url),
  input: JSON.stringify(input),
  stdio: ['pipe', 'pipe', 'inherit'],
}))
assert.equal(actual.length, cases.length)

function differences(expected, actual, path) {
  if (Object.is(expected, actual)) return []
  if (expected && actual && typeof expected === 'object' && typeof actual === 'object'
      && Array.isArray(expected) === Array.isArray(actual)) {
    const keys = [...new Set([...Object.keys(expected), ...Object.keys(actual)])].sort()
    return keys.flatMap(key => differences(expected[key], actual[key], `${path}/${key}`))
  }
  return [`${path}: expected ${JSON.stringify(expected)}, got ${JSON.stringify(actual)}`]
}

function compare(wanted, got, steps, plugins) {
  const issues = []
  for (const [i, state] of wanted.entries()) {
    const stage = i === 0 ? 'open' : `step ${i} (${steps[i - 1].op})`
    const result = got[i]
    assert(result, `driver missing state for ${stage}`)
    for (const key of ['openError', 'editError', 'saveError']) {
      if (key in result) {
        assert.equal(got.length, i + 1, 'driver continued after error')
        return [...issues, `${stage} ${key}: ${result[key]}`]
      }
    }
    for (const field of ['doc', 'selection', 'applied']) {
      assert(Object.hasOwn(result, field), `driver missing ${field}`)
      issues.push(...differences(state[field], result[field], `${stage} ${field}`))
    }
    try {
      const reopened = withEditor(plugins, result.doc, editor => editor.getJSON())
      issues.push(...differences(result.doc, reopened, `${stage} Tiptap reopen`))
    } catch (error) {
      if (!(error instanceof InvalidContent)) throw error
      issues.push(`${stage} Tiptap reopen: ${error.message}`)
    }
  }
  assert.equal(got.length, wanted.length, 'driver returned extra states')
  return issues
}

let passed = 0
let gaps = 0
let unexpected = 0
for (const [i, { name, steps = [], plugins = [] }] of cases.entries()) {
  const issues = compare(expected[i].states, actual[i], steps, plugins)
  const label = `[${['paragraph', ...plugins].join('+')}] ${name}`
  if (!issues.length && !Object.hasOwn(known, name)) {
    passed++
    console.log(`PASS  ${label}`)
  } else if (issues.length && isDeepStrictEqual(issues, known[name])) {
    gaps++
    console.log(`GAP   ${label} — ${issues.join('\n      ')}`)
  } else {
    unexpected++
    console.error(`FAIL  ${label} — ${issues.join('\n      ') || 'now passes; remove its known gap'}`)
    if (name in known) console.error('      recorded:', known[name])
  }
}
console.log(`\n${cases.length} cases: ${passed} compatible, ${gaps} known gaps, ${unexpected} unexpected`)
if (unexpected || (strict && gaps)) process.exitCode = 1
