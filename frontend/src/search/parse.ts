export type IsoCmp = 'gt' | 'gte' | 'lt' | 'lte' | 'eq'

export type SearchNode =
  | { op: 'and'; args: SearchNode[] }
  | { op: 'or'; args: SearchNode[] }
  | { op: 'not'; arg: SearchNode }
  | { op: 'text'; value: string }
  | { op: 'type'; value: string }
  | { op: 'camera'; value: string }
  | { op: 'lens'; value: string }
  | { op: 'iso'; cmp: IsoCmp; value: number }
  | { op: 'year'; value: number }
  | { op: 'folder'; id: string }
  | { op: 'has_gps' }

/** Parser minimo della barra di ricerca. Nessuna stringa finisce in SQL: produce solo AST. */
export function parseSearch(input: string): SearchNode {
  const tokens = tokenize(input.trim())
  if (tokens.length === 0) {
    return { op: 'and', args: [] }
  }
  const { node, next } = parseOr(tokens, 0)
  if (next !== tokens.length) {
    throw new Error('unexpected token')
  }
  return node
}

type Tok =
  | { kind: 'and' | 'or' | 'not' | 'lparen' | 'rparen' }
  | { kind: 'field'; field: string; cmp: IsoCmp; value: string }
  | { kind: 'word'; value: string }

function tokenize(s: string): Tok[] {
  const out: Tok[] = []
  let i = 0
  while (i < s.length) {
    if (s[i] === ' ') {
      i += 1
      continue
    }
    if (s[i] === '(') {
      out.push({ kind: 'lparen' })
      i += 1
      continue
    }
    if (s[i] === ')') {
      out.push({ kind: 'rparen' })
      i += 1
      continue
    }
    const rest = s.slice(i)
    const field = rest.match(
      /^(type|camera|lens|iso|folder|has):([<>]=?|=)?("([^"]*)"|[^\s)]+)/i
    )
    if (field) {
      const name = field[1].toLowerCase()
      const cmpRaw = field[2] ?? '='
      const rawVal = field[4] ?? field[3]
      const cmp: IsoCmp =
        cmpRaw === '>' ? 'gt' : cmpRaw === '>=' ? 'gte' : cmpRaw === '<' ? 'lt' : cmpRaw === '<=' ? 'lte' : 'eq'
      out.push({ kind: 'field', field: name, cmp, value: rawVal })
      i += field[0].length
      continue
    }
    const word = rest.match(/^("([^"]*)"|[^\s()]+)/)
    if (!word) {
      break
    }
    const value = word[2] ?? word[1]
    const lower = value.toLowerCase()
    if (lower === 'and' || lower === 'or' || lower === 'not') {
      out.push({ kind: lower })
    } else {
      out.push({ kind: 'word', value })
    }
    i += word[0].length
  }
  return out
}

function parseOr(tokens: Tok[], i: number): { node: SearchNode; next: number } {
  const parts: SearchNode[] = []
  let next = i
  while (next < tokens.length) {
    const and = parseAnd(tokens, next)
    parts.push(and.node)
    next = and.next
    if (tokens[next]?.kind === 'or') {
      next += 1
      continue
    }
    break
  }
  if (parts.length === 1) {
    return { node: parts[0], next }
  }
  return { node: { op: 'or', args: parts }, next }
}

function parseAnd(tokens: Tok[], i: number): { node: SearchNode; next: number } {
  const parts: SearchNode[] = []
  let next = i
  while (next < tokens.length) {
    const primary = parseNot(tokens, next)
    parts.push(primary.node)
    next = primary.next
    if (tokens[next]?.kind === 'and') {
      next += 1
      continue
    }
    if (
      tokens[next] &&
      tokens[next].kind !== 'or' &&
      tokens[next].kind !== 'rparen'
    ) {
      continue
    }
    break
  }
  if (parts.length === 1) {
    return { node: parts[0], next }
  }
  return { node: { op: 'and', args: parts }, next }
}

function parseNot(tokens: Tok[], i: number): { node: SearchNode; next: number } {
  if (tokens[i]?.kind === 'not') {
    const inner = parsePrimary(tokens, i + 1)
    return { node: { op: 'not', arg: inner.node }, next: inner.next }
  }
  return parsePrimary(tokens, i)
}

function parsePrimary(tokens: Tok[], i: number): { node: SearchNode; next: number } {
  const t = tokens[i]
  if (!t) {
    throw new Error('unexpected end')
  }
  if (t.kind === 'lparen') {
    const inner = parseOr(tokens, i + 1)
    if (tokens[inner.next]?.kind !== 'rparen') {
      throw new Error('missing )')
    }
    return { node: inner.node, next: inner.next + 1 }
  }
  if (t.kind === 'field') {
    return { node: fieldNode(t.field, t.cmp, t.value), next: i + 1 }
  }
  if (t.kind === 'word') {
    if (/^\d{4}$/.test(t.value)) {
      return { node: { op: 'year', value: Number(t.value) }, next: i + 1 }
    }
    return { node: { op: 'text', value: t.value }, next: i + 1 }
  }
  throw new Error('unexpected token')
}

function fieldNode(field: string, cmp: IsoCmp, value: string): SearchNode {
  switch (field) {
    case 'type':
      return { op: 'type', value: value.toLowerCase() }
    case 'camera':
      return { op: 'camera', value }
    case 'lens':
      return { op: 'lens', value }
    case 'iso':
      return { op: 'iso', cmp, value: Number(value) }
    case 'folder':
      return { op: 'folder', id: value }
    case 'has':
      if (value.toLowerCase() === 'gps') {
        return { op: 'has_gps' }
      }
      return { op: 'text', value: `has:${value}` }
    default:
      return { op: 'text', value }
  }
}
