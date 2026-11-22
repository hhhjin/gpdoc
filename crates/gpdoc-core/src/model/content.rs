//! default children are inserted when several node types would fit.
use crate::model::{Error, Fragment, NodeSpec, Schema};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct ContentMatch {
    // Transitions are indexed by type name within one schema. A schema ID
    // preserves NodeType identity when matching a fragment from an external caller.
    schema_id: Arc<()>,
    states: Arc<[State]>,
    index: usize,
}

#[derive(Debug, Default)]
struct State {
    valid_end: bool,
    edges: Vec<(String, usize)>,
}

impl ContentMatch {
    pub(super) fn parse(
        expression: &str,
        types: &[(String, NodeSpec)],
        schema_id: Arc<()>,
    ) -> Result<Self, Error> {
        let mut parser = Parser::new(expression, types);
        let states = if parser.peek().is_none() {
            vec![State {
                valid_end: true,
                edges: vec![],
            }]
        } else {
            let expr = parser.choice()?;
            if parser.peek().is_some() {
                return Err(parser.error("Unexpected trailing text"));
            }
            let states = Nfa::from_expr(&expr).to_dfa();
            for state in &states {
                if !state.valid_end
                    && !state.edges.iter().any(|(name, _)| {
                        name != "text"
                            && types
                                .iter()
                                .any(|(n, s)| n == name && !s.has_required_attrs())
                    })
                {
                    return Err(parser.error("Only non-generatable nodes in a required position"));
                }
            }
            states
        };
        Ok(Self {
            schema_id,
            states: states.into(),
            index: 0,
        })
    }

    fn at(&self, index: usize) -> Self {
        Self {
            schema_id: self.schema_id.clone(),
            states: self.states.clone(),
            index,
        }
    }
    fn edges(&self) -> &[(String, usize)] {
        &self.states[self.index].edges
    }
    pub fn edge_count(&self) -> usize {
        self.edges().len()
    }
    pub fn edge(&self, index: usize) -> Option<(&str, Self)> {
        self.edges()
            .get(index)
            .map(|(name, to)| (name.as_str(), self.at(*to)))
    }
    pub fn valid_end(&self) -> bool {
        self.states[self.index].valid_end
    }
    pub(super) fn belongs_to(&self, schema: &Schema) -> bool {
        schema.has_id(&self.schema_id)
    }
    pub fn match_type(&self, name: &str) -> Option<Self> {
        self.edges()
            .iter()
            .find(|(kind, _)| kind == name)
            .map(|(_, to)| self.at(*to))
    }
    pub fn match_fragment(&self, fragment: &Fragment) -> Option<Self> {
        self.match_fragment_range(fragment, 0, fragment.children().len())
    }
    pub fn match_fragment_range(
        &self,
        fragment: &Fragment,
        start: usize,
        end: usize,
    ) -> Option<Self> {
        fragment
            .children()
            .get(start..end)?
            .iter()
            .try_fold(self.clone(), |state, node| {
                if node.schema().is_some_and(|schema| state.belongs_to(schema)) {
                    state.match_type(node.kind())
                } else {
                    None
                }
            })
    }
    pub fn compatible(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.schema_id, &other.schema_id)
            && self
                .edges()
                .iter()
                .any(|(name, _)| other.edges().iter().any(|(n, _)| name == n))
    }
    pub fn default_type(&self, schema: &Schema) -> Option<&str> {
        if !self.belongs_to(schema) {
            return None;
        }
        self.edges()
            .iter()
            .find(|(name, _)| schema.generatable(name))
            .map(|(name, _)| name.as_str())
    }

    pub fn fill_before(
        &self,
        schema: &Schema,
        after: &Fragment,
        to_end: bool,
        start_index: usize,
    ) -> Option<Fragment> {
        if !self.belongs_to(schema) {
            return None;
        }
        self.fill(schema, after, to_end, start_index, &mut Vec::new())
    }

    pub(super) fn fill(
        &self,
        schema: &Schema,
        after: &Fragment,
        to_end: bool,
        start_index: usize,
        generating: &mut Vec<String>,
    ) -> Option<Fragment> {
        fn search(
            state: &ContentMatch,
            schema: &Schema,
            after: &Fragment,
            to_end: bool,
            start: usize,
            seen: &mut HashSet<usize>,
            path: &mut Vec<String>,
        ) -> bool {
            if state
                .match_fragment_range(after, start, after.children().len())
                .is_some_and(|end| !to_end || end.valid_end())
            {
                return true;
            }
            for (kind, next) in state.edges() {
                if schema.generatable(kind) && seen.insert(*next) {
                    path.push(kind.clone());
                    if search(&state.at(*next), schema, after, to_end, start, seen, path) {
                        return true;
                    }
                    path.pop();
                }
            }
            false
        }
        let mut path = vec![];
        let mut seen = HashSet::from([self.index]);
        if !search(
            self,
            schema,
            after,
            to_end,
            start_index,
            &mut seen,
            &mut path,
        ) {
            return None;
        }
        let nodes = path
            .iter()
            .map(|name| schema.default_child(name, generating))
            .collect::<Option<Vec<_>>>()?;
        Some(Fragment::from(nodes))
    }

    pub fn find_wrapping(&self, schema: &Schema, target: &str) -> Option<Vec<String>> {
        if !self.belongs_to(schema) {
            return None;
        }
        let mut queue = VecDeque::from([(self.clone(), Vec::<String>::new())]);
        let mut seen = HashSet::new();
        while let Some((state, path)) = queue.pop_front() {
            if state.match_type(target).is_some() {
                return Some(path);
            }
            for (name, next) in state.edges() {
                let ty = schema.node_type(name)?;
                if !ty.is_leaf()
                    && !ty.spec.has_required_attrs()
                    && (path.is_empty() || state.at(*next).valid_end())
                    && seen.insert(name.clone())
                {
                    let mut wrapped = path.clone();
                    wrapped.push(name.clone());
                    queue.push_back((ty.content_match.clone(), wrapped));
                }
            }
        }
        None
    }
}

#[derive(Debug)]
enum Expr {
    Name(String),
    Choice(Vec<Expr>),
    Sequence(Vec<Expr>),
    Star(Box<Expr>),
    Plus(Box<Expr>),
    Optional(Box<Expr>),
    Range(Box<Expr>, usize, Option<usize>),
}

struct Parser<'a> {
    expression: &'a str,
    tokens: Vec<&'a str>,
    cursor: usize,
    types: &'a [(String, NodeSpec)],
    inline: Option<bool>,
}

fn word(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

impl<'a> Parser<'a> {
    fn new(expression: &'a str, types: &'a [(String, NodeSpec)]) -> Self {
        let mut tokens = Vec::new();
        let mut chars = expression.char_indices().peekable();
        while let Some((start, c)) = chars.next() {
            if c.is_whitespace() {
                continue;
            }
            if word(c) {
                while chars.next_if(|(_, ch)| word(*ch)).is_some() {}
            }
            let end = chars.peek().map_or(expression.len(), |(end, _)| *end);
            tokens.push(&expression[start..end]);
        }
        Self {
            expression,
            tokens,
            cursor: 0,
            types,
            inline: None,
        }
    }
    fn peek(&self) -> Option<&'a str> {
        self.tokens.get(self.cursor).copied()
    }
    fn error(&self, msg: &str) -> Error {
        Error(format!(
            "{msg} (in content expression '{}')",
            self.expression
        ))
    }
    fn eat(&mut self, token: &str) -> bool {
        if self.peek() == Some(token) {
            self.cursor += 1;
            true
        } else {
            false
        }
    }
    fn expect(&mut self, token: &str) -> Result<(), Error> {
        if self.eat(token) {
            Ok(())
        } else {
            Err(self.error(&format!("Expected '{token}'")))
        }
    }
    fn choice(&mut self) -> Result<Expr, Error> {
        let mut choices = vec![self.sequence()?];
        while self.eat("|") {
            choices.push(self.sequence()?);
        }
        Ok(if choices.len() == 1 {
            choices.pop().unwrap()
        } else {
            Expr::Choice(choices)
        })
    }
    fn sequence(&mut self) -> Result<Expr, Error> {
        let mut exprs = vec![self.subscript()?];
        while !matches!(self.peek(), None | Some("|" | ")")) {
            exprs.push(self.subscript()?);
        }
        Ok(if exprs.len() == 1 {
            exprs.pop().unwrap()
        } else {
            Expr::Sequence(exprs)
        })
    }
    fn subscript(&mut self) -> Result<Expr, Error> {
        let mut expr = self.atom()?;
        loop {
            expr = if self.eat("+") {
                Expr::Plus(Box::new(expr))
            } else if self.eat("*") {
                Expr::Star(Box::new(expr))
            } else if self.eat("?") {
                Expr::Optional(Box::new(expr))
            } else if self.eat("{") {
                let min = self.number()?;
                let max = if !self.eat(",") {
                    Some(min)
                } else if self.peek() == Some("}") {
                    None
                } else {
                    Some(self.number()?)
                };
                self.expect("}")?;
                Expr::Range(Box::new(expr), min, max)
            } else {
                return Ok(expr);
            };
        }
    }
    fn number(&mut self) -> Result<usize, Error> {
        let value = self
            .peek()
            .and_then(|s| s.parse().ok())
            .ok_or_else(|| self.error("Expected repeat count"))?;
        self.cursor += 1;
        Ok(value)
    }
    fn atom(&mut self) -> Result<Expr, Error> {
        if self.eat("(") {
            let expr = self.choice()?;
            self.expect(")")?;
            return Ok(expr);
        }
        let token = self
            .peek()
            .filter(|s| s.chars().all(word))
            .ok_or_else(|| self.error("Expected a node type or group"))?;
        self.cursor += 1;
        let named = self.types.iter().find(|(n, _)| n == token);
        let matches: Vec<_> = if let Some(ty) = named {
            vec![ty]
        } else {
            self.types
                .iter()
                .filter(|(_, s)| s.group.split_whitespace().any(|g| g == token))
                .collect()
        };
        if matches.is_empty() {
            return Err(self.error(&format!("No node type or group '{token}' found")));
        }
        let mut exprs = Vec::new();
        for (name, spec) in matches {
            let inline = spec.inline || name == "text";
            if self.inline.is_some_and(|i| i != inline) {
                return Err(self.error("Mixing inline and block content"));
            }
            self.inline = Some(inline);
            exprs.push(Expr::Name(name.clone()));
        }
        Ok(if exprs.len() == 1 {
            exprs.pop().unwrap()
        } else {
            Expr::Choice(exprs)
        })
    }
}

// Keep dangling edge addresses during Thompson construction so alternatives and
// repetitions retain exactly the edge order of the upstream graph.
struct Nfa(Vec<Vec<Edge>>);
struct Edge {
    term: Option<String>,
    to: Option<usize>,
}
type Outlet = (usize, usize);
impl Nfa {
    fn from_expr(expr: &Expr) -> Self {
        let mut nfa = Self(vec![vec![]]);
        let exits = nfa.compile(expr, 0);
        let end = nfa.state();
        nfa.connect(exits, end);
        nfa
    }
    fn state(&mut self) -> usize {
        self.0.push(vec![]);
        self.0.len() - 1
    }
    fn edge(&mut self, from: usize, to: Option<usize>, term: Option<String>) -> Outlet {
        let index = self.0[from].len();
        self.0[from].push(Edge { term, to });
        (from, index)
    }
    fn connect(&mut self, edges: Vec<Outlet>, to: usize) {
        for (from, index) in edges {
            self.0[from][index].to = Some(to);
        }
    }
    fn compile(&mut self, expr: &Expr, from: usize) -> Vec<Outlet> {
        match expr {
            Expr::Name(name) => vec![self.edge(from, None, Some(name.clone()))],
            Expr::Choice(choices) => choices.iter().flat_map(|c| self.compile(c, from)).collect(),
            Expr::Sequence(parts) => {
                let mut current = from;
                for part in &parts[..parts.len() - 1] {
                    let edges = self.compile(part, current);
                    current = self.state();
                    self.connect(edges, current);
                }
                self.compile(parts.last().unwrap(), current)
            }
            Expr::Star(inner) | Expr::Plus(inner) => {
                let repeat = self.state();
                if matches!(expr, Expr::Star(_)) {
                    self.edge(from, Some(repeat), None);
                } else {
                    let first = self.compile(inner, from);
                    self.connect(first, repeat);
                }
                let tail = self.compile(inner, repeat);
                self.connect(tail, repeat);
                vec![self.edge(repeat, None, None)]
            }
            Expr::Optional(inner) => {
                let mut exits = vec![self.edge(from, None, None)];
                exits.extend(self.compile(inner, from));
                exits
            }
            Expr::Range(inner, min, max) => {
                let mut current = from;
                for _ in 0..*min {
                    let next = self.state();
                    let edges = self.compile(inner, current);
                    self.connect(edges, next);
                    current = next;
                }
                if let Some(max) = max {
                    for _ in *min..*max {
                        let next = self.state();
                        self.edge(current, Some(next), None);
                        let edges = self.compile(inner, current);
                        self.connect(edges, next);
                        current = next;
                    }
                } else {
                    let edges = self.compile(inner, current);
                    self.connect(edges, current);
                }
                vec![self.edge(current, None, None)]
            }
        }
    }
    fn epsilon_closure(&self, start: usize) -> Vec<usize> {
        let mut seen = HashSet::new();
        let mut pending = vec![start];
        let mut result = vec![];
        while let Some(n) = pending.pop() {
            if !seen.insert(n) {
                continue;
            }
            let edges = &self.0[n];
            if edges.len() != 1 || edges[0].term.is_some() {
                result.push(n);
            }
            pending.extend(
                edges
                    .iter()
                    .filter(|e| e.term.is_none())
                    .map(|e| e.to.unwrap()),
            );
        }
        result.sort_unstable_by(|a, b| b.cmp(a));
        result
    }
    fn to_dfa(&self) -> Vec<State> {
        let first = self.epsilon_closure(0);
        let mut labeled = HashMap::from([(first.clone(), 0)]);
        let mut sets = vec![first];
        let mut result = vec![];
        while result.len() < sets.len() {
            let set = &sets[result.len()];
            let valid_end = set.contains(&(self.0.len() - 1));
            let mut transitions: Vec<(&str, Vec<usize>)> = Vec::new();
            for source in set {
                for edge in &self.0[*source] {
                    let Some(term) = edge.term.as_deref() else {
                        continue;
                    };
                    let dest = if let Some(i) = transitions.iter().position(|(s, _)| *s == term) {
                        &mut transitions[i].1
                    } else {
                        transitions.push((term, vec![]));
                        &mut transitions.last_mut().unwrap().1
                    };
                    dest.extend(self.epsilon_closure(edge.to.unwrap()));
                }
            }
            let mut edges = vec![];
            for (term, mut set) in transitions {
                set.sort_unstable_by(|a, b| b.cmp(a));
                set.dedup();
                let next = *labeled.entry(set.clone()).or_insert_with(|| {
                    let next = sets.len();
                    sets.push(set);
                    next
                });
                edges.push((term.to_owned(), next));
            }
            result.push(State { valid_end, edges });
        }
        result
    }
}

#[cfg(test)]
mod tests;
