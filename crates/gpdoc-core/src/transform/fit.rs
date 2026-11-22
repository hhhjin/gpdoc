use crate::model::{
    Attrs, ContentMatch, Error, Fragment, Node, NodeType, ResolvedPos, Schema, Slice,
};
use crate::transform::{ReplaceAroundStep, ReplaceStep, Step};

/// Fit a slice into the replacement range. Returns no step when placement has no effect.
pub fn replace_step(
    schema: &Schema,
    doc: &Node,
    from: usize,
    to: usize,
    slice: &Slice,
) -> Result<Option<Step>, Error> {
    if from > to || to > doc.content.size() {
        return Err(Error("Invalid replacement range".into()));
    }
    if doc.schema() != Some(schema) {
        return Err(Error("document belongs to another schema".into()));
    }
    slice.check_schema(schema)?;
    slice.check_open_depths()?;
    if from == to && slice.size() == 0 {
        return Ok(None);
    }

    let start = doc.resolve(from)?;
    let end = doc.resolve(to)?;
    if slice.open_start == 0
        && slice.open_end == 0
        && start.same_parent(&end)
        && start.parent().can_replace(
            schema,
            start.index(start.depth()),
            end.index(end.depth()),
            &slice.content,
        )
    {
        return Ok(Some(Step::Replace(ReplaceStep {
            from,
            to,
            slice: slice.clone(),
            structure: false,
        })));
    }
    Fitter::new(schema, start, end, slice.clone())?.fit()
}

// Each level tracks which children still fit after the nodes placed at that depth.
struct Frontier<'s> {
    ty: &'s NodeType,
    state: ContentMatch,
}

struct Fittable {
    slice_depth: usize,
    frontier_depth: usize,
    parent: Option<Node>,
    inject: Option<Fragment>,
    wrap: Vec<String>,
}

struct CloseLevel {
    depth: usize,
    fit: Fragment,
    move_to: ResolvedPos,
}

struct Fitter<'s> {
    schema: &'s Schema,
    from: ResolvedPos,
    to: ResolvedPos,
    frontier: Vec<Frontier<'s>>,
    placed: Fragment,
    unplaced: Slice,
}

impl<'s> Fitter<'s> {
    fn new(
        schema: &'s Schema,
        from: ResolvedPos,
        to: ResolvedPos,
        unplaced: Slice,
    ) -> Result<Self, Error> {
        let mut frontier = Vec::with_capacity(from.depth() + 1);
        for d in 0..=from.depth() {
            let node = from.node(d);
            frontier.push(Frontier {
                ty: node_type(schema, &node.kind)?,
                state: node.content_match_at(schema, from.index_after(d))?,
            });
        }
        let mut placed = Fragment::default();
        for d in (1..=from.depth()).rev() {
            placed = Fragment::from(from.node(d).copy(placed));
        }
        Ok(Self {
            schema,
            from,
            to,
            frontier,
            placed,
            unplaced,
        })
    }

    fn depth(&self) -> usize {
        self.frontier.len() - 1
    }

    fn fit(mut self) -> Result<Option<Step>, Error> {
        while self.unplaced.size() > 0 {
            if let Some(fit) = self.find_fittable() {
                self.place_nodes(fit)?;
            } else if !self.open_more()? {
                self.drop_node();
            }
        }
        let move_inline = self.must_move_inline()?;
        let end = match move_inline {
            Some(pos) => self.from.doc().resolve(pos)?,
            None => self.to.clone(),
        };
        // In a ReplaceAroundStep the old inline tail goes after the placed content.
        let placed_size = self.placed.size() - self.depth() - self.from.depth();
        let Some(end) = self.close(end)? else {
            return Ok(None);
        };
        let (mut open_start, mut open_end) = (self.from.depth(), end.depth());
        let mut content = self.placed;
        while open_start > 0 && open_end > 0 && content.child_count() == 1 {
            content = content.child(0).content.clone();
            open_start -= 1;
            open_end -= 1;
        }
        let slice = Slice::new(content, open_start, open_end);
        if let Some(pos) = move_inline {
            Ok(Some(Step::ReplaceAround(ReplaceAroundStep {
                from: self.from.pos,
                to: pos,
                gap_from: self.to.pos,
                gap_to: self.to.end(self.to.depth()),
                insert: placed_size,
                slice,
                structure: false,
            })))
        } else if slice.size() > 0 || self.from.pos != self.to.pos {
            Ok(Some(Step::Replace(ReplaceStep {
                from: self.from.pos,
                to: end.pos,
                slice,
                structure: false,
            })))
        } else {
            Ok(None)
        }
    }

    fn find_fittable(&self) -> Option<Fittable> {
        let slice = &self.unplaced;
        let mut start_depth = slice.open_start;
        let mut cur = &slice.content;
        let mut open_end = slice.open_end;
        for depth in 0..start_depth {
            let node = cur.child(0);
            if cur.child_count() > 1 {
                open_end = 0;
            }
            if self.schema.is_isolating(&node.kind) && open_end <= depth {
                start_depth = depth;
                break;
            }
            cur = &node.content;
        }

        // Prefer direct matches or filler. Only look for wrappers when neither can fit.
        for wrapping in [false, true] {
            let deepest = if wrapping {
                slice.open_start
            } else {
                start_depth
            };
            for slice_depth in (0..=deepest).rev() {
                let parent = slice_depth
                    .checked_sub(1)
                    .map(|d| content_at(&slice.content, d).child(0));
                let fragment = parent.map_or(&slice.content, |n| &n.content);
                let first = fragment.children().first();
                for frontier_depth in (0..=self.depth()).rev() {
                    let frontier = &self.frontier[frontier_depth];
                    let mut fit = Fittable {
                        slice_depth,
                        frontier_depth,
                        parent: parent.cloned(),
                        inject: None,
                        wrap: vec![],
                    };
                    if wrapping {
                        if let Some(wrap) =
                            first.and_then(|n| frontier.state.find_wrapping(self.schema, n.kind()))
                        {
                            fit.wrap = wrap;
                            return Some(fit);
                        }
                    } else if let Some(first) = first {
                        if frontier.state.match_type(first.kind()).is_some() {
                            return Some(fit);
                        }
                        fit.inject = frontier.state.fill_before(
                            self.schema,
                            &Fragment::from(first.clone()),
                            false,
                            0,
                        );
                        if fit.inject.is_some() {
                            return Some(fit);
                        }
                    } else if parent.is_some_and(|p| {
                        self.schema.compatible_content(&frontier.ty.name, p.kind())
                    }) {
                        return Some(fit);
                    }
                    if parent.is_some_and(|p| frontier.state.match_type(p.kind()).is_some()) {
                        break;
                    }
                }
            }
        }
        None
    }

    fn open_more(&mut self) -> Result<bool, Error> {
        let slice = &mut self.unplaced;
        if max_open(self.schema, &slice.content, true)? <= slice.open_start {
            return Ok(false);
        }
        if slice.size() > 1 && max_open(self.schema, &slice.content, false)? > slice.open_end {
            slice.open_end += 1;
        }
        slice.open_start += 1;
        Ok(true)
    }

    fn drop_node(&mut self) {
        let slice = &self.unplaced;
        let inner = content_at(&slice.content, slice.open_start);
        self.unplaced = if inner.child_count() <= 1 && slice.open_start > 0 {
            let open_at_end =
                slice.content.size() - slice.open_start <= slice.open_start + inner.size();
            let depth = slice.open_start - 1;
            Slice::new(
                drop_from_fragment(&slice.content, depth, 1),
                depth,
                if open_at_end { depth } else { slice.open_end },
            )
        } else {
            Slice::new(
                drop_from_fragment(&slice.content, slice.open_start, 1),
                slice.open_start,
                slice.open_end,
            )
        };
    }

    fn place_nodes(&mut self, fit: Fittable) -> Result<(), Error> {
        let Fittable {
            slice_depth,
            frontier_depth,
            parent,
            inject,
            wrap,
        } = fit;
        while self.depth() > frontier_depth {
            self.close_frontier_node()?;
        }
        for kind in wrap {
            self.open_frontier_node(&kind)?;
        }

        let slice = self.unplaced.clone();
        let fragment = parent.as_ref().map_or(&slice.content, |p| &p.content);
        let open_start = slice.open_start - slice_depth;
        let frontier = &self.frontier[frontier_depth];
        let mut state = frontier.state.clone();
        let mut add = Vec::new();
        if let Some(inject) = inject {
            add.extend_from_slice(inject.children());
            state = state.match_fragment(&inject).ok_or_else(no_fit)?;
        }

        // None means the end is closed. Some(0) means only the parent is open.
        let mut open_end =
            (fragment.size() + slice_depth).checked_sub(slice.content.size() - slice.open_end);
        let mut taken = 0;
        for next in fragment.children() {
            let Some(matched) = state.match_type(next.kind()) else {
                break;
            };
            taken += 1;
            if taken > 1 || open_start == 0 || next.content.size() > 0 {
                state = matched;
                let marks = next
                    .marks()
                    .iter()
                    .filter(|m| frontier.ty.allows_mark(&m.kind))
                    .cloned()
                    .collect();
                let node = close_node_start(
                    self.schema,
                    next.mark(marks),
                    if taken == 1 { open_start } else { 0 },
                    if taken == fragment.child_count() {
                        open_end.unwrap_or(0)
                    } else {
                        0
                    },
                )?;
                add.push(node);
            }
        }
        let to_end = taken == fragment.child_count();
        if !to_end {
            open_end = None;
        }

        let added = Fragment::from(add);
        self.placed = add_to_fragment(&self.placed, frontier_depth, &added);
        self.frontier[frontier_depth].state = state;
        if to_end
            && open_end.is_none()
            && self.depth() > 0
            && parent
                .as_ref()
                .is_some_and(|p| p.kind() == self.frontier[self.depth()].ty.name)
        {
            self.close_frontier_node()?;
        }
        let mut cur = added.children().last();
        for _ in 0..open_end.unwrap_or(0) {
            let Some(node) = cur else { break };
            self.frontier.push(Frontier {
                ty: node_type(self.schema, node.kind())?,
                state: node.content_match_at(self.schema, node.child_count())?,
            });
            cur = node.content.children().last();
        }
        self.unplaced = if !to_end {
            Slice::new(
                drop_from_fragment(&slice.content, slice_depth, taken),
                slice_depth,
                slice.open_end,
            )
        } else if slice_depth == 0 {
            Slice::empty()
        } else {
            Slice::new(
                drop_from_fragment(&slice.content, slice_depth - 1, 1),
                slice_depth - 1,
                if open_end.is_none() {
                    slice.open_end
                } else {
                    slice_depth - 1
                },
            )
        };
        Ok(())
    }

    fn must_move_inline(&self) -> Result<Option<usize>, Error> {
        let to = &self.to;
        let top = &self.frontier[self.depth()];
        if !node_type(self.schema, to.parent().kind())?.is_textblock()
            || !top.ty.is_textblock()
            || self
                .content_after_fits(to, to.depth(), top, false)
                .is_none()
            || to.depth() == self.depth()
                && self
                    .find_close_level(to)?
                    .is_some_and(|level| level.depth == self.depth())
        {
            return Ok(None);
        }
        let mut depth = to.depth();
        let mut after = to.after(depth)?;
        while depth > 1 {
            depth -= 1;
            if after != to.end(depth) {
                break;
            }
            after += 1;
        }
        Ok(Some(after))
    }

    fn find_close_level(&self, to: &ResolvedPos) -> Result<Option<CloseLevel>, Error> {
        'scan: for depth in (0..=self.depth().min(to.depth())).rev() {
            let drop_inner =
                depth < to.depth() && to.end(depth + 1) == to.pos + (to.depth() - depth - 1);
            let Some(fit) = self.content_after_fits(to, depth, &self.frontier[depth], drop_inner)
            else {
                continue;
            };
            for d in (0..depth).rev() {
                if !self
                    .content_after_fits(to, d, &self.frontier[d], true)
                    .is_some_and(|fragment| fragment.child_count() == 0)
                {
                    continue 'scan;
                }
            }
            let move_to = if drop_inner {
                to.doc().resolve(to.after(depth + 1)?)?
            } else {
                to.clone()
            };
            return Ok(Some(CloseLevel {
                depth,
                fit,
                move_to,
            }));
        }
        Ok(None)
    }

    fn content_after_fits(
        &self,
        to: &ResolvedPos,
        depth: usize,
        frontier: &Frontier<'_>,
        open: bool,
    ) -> Option<Fragment> {
        let node = to.node(depth);
        let index = if open {
            to.index_after(depth)
        } else {
            to.index(depth)
        };
        if index == node.child_count()
            && !self
                .schema
                .compatible_content(&frontier.ty.name, node.kind())
        {
            return None;
        }
        if node.content.children()[index..].iter().any(|child| {
            child
                .marks()
                .iter()
                .any(|m| !frontier.ty.allows_mark(&m.kind))
        }) {
            return None;
        }
        frontier
            .state
            .fill_before(self.schema, &node.content, true, index)
    }

    fn close(&mut self, to: ResolvedPos) -> Result<Option<ResolvedPos>, Error> {
        let Some(level) = self.find_close_level(&to)? else {
            return Ok(None);
        };
        while self.depth() > level.depth {
            self.close_frontier_node()?;
        }
        if level.fit.child_count() > 0 {
            self.placed = add_to_fragment(&self.placed, level.depth, &level.fit);
        }
        let to = level.move_to;
        for depth in (level.depth + 1)..=to.depth() {
            let node = to.node(depth);
            let fill = node_type(self.schema, node.kind())?
                .content_match
                .fill_before(self.schema, &node.content, true, to.index(depth))
                .ok_or_else(no_fit)?;
            // Closing reopens the right-hand path with any required filler already
            // present. Placement has finished, so no frontier state remains to advance.
            let opened =
                self.schema
                    .node_unchecked(node.kind(), node.attrs.clone(), fill, vec![])?;
            self.placed = add_to_fragment(&self.placed, depth - 1, &Fragment::from(opened));
        }
        Ok(Some(to))
    }

    fn open_frontier_node(&mut self, kind: &str) -> Result<(), Error> {
        let depth = self.depth();
        let top = &mut self.frontier[depth];
        top.state = top.state.match_type(kind).ok_or_else(no_fit)?;
        let ty = node_type(self.schema, kind)?;
        let node = self
            .schema
            .node_unchecked(kind, Attrs::new(), Fragment::default(), vec![])?;
        self.placed = add_to_fragment(&self.placed, depth, &Fragment::from(node));
        self.frontier.push(Frontier {
            ty,
            state: ty.content_match.clone(),
        });
        Ok(())
    }

    fn close_frontier_node(&mut self) -> Result<(), Error> {
        let open = self.frontier.pop().expect("a frontier node is open");
        let fill = open
            .state
            .fill_before(self.schema, &Fragment::default(), true, 0)
            .ok_or_else(no_fit)?;
        if fill.child_count() > 0 {
            self.placed = add_to_fragment(&self.placed, self.frontier.len(), &fill);
        }
        Ok(())
    }
}

fn node_type<'s>(schema: &'s Schema, kind: &str) -> Result<&'s NodeType, Error> {
    schema
        .node_type(kind)
        .ok_or_else(|| Error(format!("Unknown node type '{kind}'")))
}

fn no_fit() -> Error {
    Error("Cannot fit content to the schema".into())
}

fn content_at(mut fragment: &Fragment, depth: usize) -> &Fragment {
    for _ in 0..depth {
        fragment = &fragment.child(0).content;
    }
    fragment
}

// Remove 'count' children from the first branch, 'depth' levels down.
fn drop_from_fragment(fragment: &Fragment, depth: usize, count: usize) -> Fragment {
    if depth == 0 {
        return fragment.cut_by_index(count, fragment.child_count());
    }
    let first = fragment.child(0);
    fragment.replace_child(
        0,
        first.copy(drop_from_fragment(&first.content, depth - 1, count)),
    )
}

// Append content to the last branch, 'depth' levels down.
fn add_to_fragment(fragment: &Fragment, depth: usize, content: &Fragment) -> Fragment {
    if depth == 0 {
        return fragment.append(content);
    }
    let index = fragment.child_count() - 1;
    let last = fragment.child(index);
    fragment.replace_child(
        index,
        last.copy(add_to_fragment(&last.content, depth - 1, content)),
    )
}

fn close_node_start(
    schema: &Schema,
    node: Node,
    open_start: usize,
    open_end: usize,
) -> Result<Node, Error> {
    if open_start == 0 {
        return Ok(node);
    }
    let state = &node_type(schema, node.kind())?.content_match;
    let mut fragment = node.content.clone();
    if open_start > 1 {
        let child = fragment.child(0).clone();
        let inner_end = if fragment.child_count() == 1 {
            open_end.saturating_sub(1)
        } else {
            0
        };
        fragment = fragment.replace_child(
            0,
            close_node_start(schema, child, open_start - 1, inner_end)?,
        );
    }
    fragment = state
        .fill_before(schema, &fragment, false, 0)
        .ok_or_else(no_fit)?
        .append(&fragment);
    if open_end == 0 {
        let fill = state
            .match_fragment(&fragment)
            .and_then(|s| s.fill_before(schema, &Fragment::default(), true, 0))
            .ok_or_else(no_fit)?;
        fragment = fragment.append(&fill);
    }
    Ok(node.copy(fragment))
}

fn max_open(schema: &Schema, mut fragment: &Fragment, first: bool) -> Result<usize, Error> {
    let mut depth = 0;
    while let Some(child) = if first {
        fragment.children().first()
    } else {
        fragment.children().last()
    } {
        if node_type(schema, child.kind())?.is_atom() {
            break;
        }
        fragment = &child.content;
        depth += 1;
    }
    Ok(depth)
}
