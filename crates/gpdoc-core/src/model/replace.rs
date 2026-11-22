use crate::model::{Error, Fragment, Node, ResolvedPos, Schema};
use serde_json::{Value, json};

/// A fragment whose first and last branches can be open at the cut.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Slice {
    pub content: Fragment,
    pub open_start: usize,
    pub open_end: usize,
}

impl Slice {
    pub fn new(content: Fragment, open_start: usize, open_end: usize) -> Self {
        Self {
            content,
            open_start,
            open_end,
        }
    }

    pub fn empty() -> Self {
        Self::default()
    }

    pub fn size(&self) -> usize {
        self.content
            .size()
            .saturating_sub(self.open_start)
            .saturating_sub(self.open_end)
    }

    pub fn max_open(schema: &Schema, content: Fragment, open_isolating: bool) -> Self {
        let open_start = edge_depth(&content, true, |n| {
            open_isolating || !schema.is_isolating(&n.kind)
        });
        let open_end = edge_depth(&content, false, |n| {
            open_isolating || !schema.is_isolating(&n.kind)
        });
        Self::new(content, open_start, open_end)
    }

    pub fn to_json(&self) -> Value {
        if self.content.size() == 0 {
            return Value::Null;
        }
        let mut result = json!({ "content": self.content.to_json() });
        if self.open_start > 0 {
            result["openStart"] = self.open_start.into();
        }
        if self.open_end > 0 {
            result["openEnd"] = self.open_end.into();
        }
        result
    }

    pub fn from_json(schema: &Schema, value: &Value) -> Result<Self, Error> {
        if value.is_null() {
            return Ok(Self::empty());
        }
        if !value.is_object() {
            return Err(Error("Invalid input for Slice.fromJSON".into()));
        }
        let depth = |key: &str| match value.get(key) {
            None | Some(Value::Null) => Ok(0),
            Some(n) => n
                .as_u64()
                .and_then(|n| usize::try_from(n).ok())
                .ok_or_else(|| Error(format!("Invalid slice {key}"))),
        };
        let slice = Self::new(
            Fragment::from_json(schema, &value["content"])?,
            depth("openStart")?,
            depth("openEnd")?,
        );
        slice.check_open_depths()?;
        Ok(slice)
    }

    /// All children must come from the schema where the slice will be applied.
    pub fn check_schema(&self, schema: &Schema) -> Result<(), Error> {
        if self
            .content
            .children()
            .iter()
            .any(|child| child.schema() != Some(schema))
        {
            return Err(Error("slice belongs to another schema".into()));
        }
        Ok(())
    }

    /// Check that both open depths fit inside the first and last branches.
    pub fn check_open_depths(&self) -> Result<(), Error> {
        if self.open_start > 0 && self.open_start > edge_depth(&self.content, true, |_| true)
            || self.open_end > 0 && self.open_end > edge_depth(&self.content, false, |_| true)
        {
            return Err(Error("Open depth exceeds slice content".into()));
        }
        Ok(())
    }

    pub fn insert_at(
        &self,
        schema: &Schema,
        pos: usize,
        fragment: &Fragment,
    ) -> Result<Option<Self>, Error> {
        self.check_open_depths()?;
        if pos > self.size() {
            return Err(Error("Position outside of slice".into()));
        }
        let content = insert_into(
            schema,
            &self.content,
            pos + self.open_start,
            fragment,
            self.open_start + 1,
            self.open_end + 1,
            None,
        )?;
        Ok(content.map(|content| Self::new(content, self.open_start, self.open_end)))
    }

    pub fn remove_between(&self, from: usize, to: usize) -> Result<Self, Error> {
        self.check_open_depths()?;
        if from > to || to > self.size() {
            return Err(Error("Range outside of slice".into()));
        }
        let content = remove_range(&self.content, from + self.open_start, to + self.open_start)?;
        Ok(Self::new(content, self.open_start, self.open_end))
    }
}

fn edge_depth(content: &Fragment, first: bool, can_open: impl Fn(&Node) -> bool) -> usize {
    let mut depth = 0;
    let mut content = content;
    while let Some(node) = if first {
        content.children().first()
    } else {
        content.children().last()
    } {
        if node.is_leaf() || !can_open(node) {
            break;
        }
        depth += 1;
        content = &node.content;
    }
    depth
}

fn remove_range(content: &Fragment, from: usize, to: usize) -> Result<Fragment, Error> {
    let (index, offset) = content.find_index(from)?;
    let (index_to, offset_to) = content.find_index(to)?;
    if offset == from || content.child(index).is_text() {
        if offset_to != to && !content.child(index_to).is_text() {
            return Err(Error("Removing non-flat range".into()));
        }
        return Ok(content
            .cut(0, from)?
            .append(&content.cut(to, content.size())?));
    }
    if index != index_to {
        return Err(Error("Removing non-flat range".into()));
    }
    let child = content.child(index);
    let inner = remove_range(&child.content, from - offset - 1, to - offset - 1)?;
    Ok(content.replace_child(index, child.copy(inner)))
}

fn insert_into(
    schema: &Schema,
    content: &Fragment,
    dist: usize,
    insert: &Fragment,
    open_start: usize,
    open_end: usize,
    parent: Option<&Node>,
) -> Result<Option<Fragment>, Error> {
    let (index, offset) = content.find_index(dist)?;
    if offset == dist || content.child(index).is_text() {
        if open_start == 0
            && open_end == 0
            && parent.is_some_and(|p| !p.can_replace(schema, index, index, insert))
        {
            return Ok(None);
        }
        let content = content
            .cut(0, dist)?
            .append(insert)
            .append(&content.cut(dist, content.size())?);
        return Ok(Some(content));
    }
    let child = content.child(index);
    let start = if index == 0 {
        open_start.saturating_sub(1)
    } else {
        0
    };
    let end = if index + 1 == content.child_count() {
        open_end.saturating_sub(1)
    } else {
        0
    };
    let inner = insert_into(
        schema,
        &child.content,
        dist - offset - 1,
        insert,
        start,
        end,
        Some(child),
    )?;
    Ok(inner.map(|inner| content.replace_child(index, child.copy(inner))))
}

pub(super) fn replace(
    schema: &Schema,
    from: &ResolvedPos,
    to: &ResolvedPos,
    slice: &Slice,
) -> Result<Node, Error> {
    if from.pos > to.pos {
        return Err(Error("Reversed replacement range".into()));
    }
    if slice.open_start > from.depth() {
        return Err(Error(
            "Inserted content deeper than insertion position".into(),
        ));
    }
    if to.depth().checked_sub(slice.open_end) != Some(from.depth() - slice.open_start) {
        return Err(Error("Inconsistent open depths".into()));
    }
    slice.check_open_depths()?;
    replace_outer(schema, from, to, slice, 0)
}

fn replace_outer(
    schema: &Schema,
    from: &ResolvedPos,
    to: &ResolvedPos,
    slice: &Slice,
    depth: usize,
) -> Result<Node, Error> {
    let index = from.index(depth);
    let node = from.node(depth);
    if index == to.index(depth) && depth < from.depth() - slice.open_start {
        let inner = replace_outer(schema, from, to, slice, depth + 1)?;
        return Ok(node.copy(node.content.replace_child(index, inner)));
    }
    let content = if slice.content.size() == 0 {
        replace_two_way(schema, from, to, depth)?
    } else if slice.open_start == 0
        && slice.open_end == 0
        && from.depth() == depth
        && to.depth() == depth
    {
        node.content
            .cut(0, from.parent_offset)?
            .append(&slice.content)
            .append(&node.content.cut(to.parent_offset, node.content.size())?)
    } else {
        let (start, end) = prepare_slice_for_replace(slice, from)?;
        replace_three_way(schema, from, &start, &end, to, depth)?
    };
    close(schema, node, content)
}

fn check_join(schema: &Schema, main: &Node, sub: &Node) -> Result<(), Error> {
    if !schema.compatible_content(&sub.kind, &main.kind) {
        return Err(Error(format!(
            "Cannot join {} onto {}",
            sub.kind, main.kind
        )));
    }
    Ok(())
}

fn joinable<'a>(
    schema: &Schema,
    before: &'a ResolvedPos,
    after: &ResolvedPos,
    depth: usize,
) -> Result<&'a Node, Error> {
    let node = before.node(depth);
    check_join(schema, node, after.node(depth))?;
    Ok(node)
}

fn add_range(
    start: Option<&ResolvedPos>,
    end: Option<&ResolvedPos>,
    depth: usize,
    target: &mut Vec<Node>,
) -> Result<(), Error> {
    let node = end
        .or(start)
        .expect("a replacement range has an endpoint")
        .node(depth);
    let mut first = 0;
    let last = end.map_or(node.child_count(), |end| end.index(depth));
    if let Some(start) = start {
        first = start.index(depth);
        if start.depth() > depth {
            first += 1;
        } else if start.text_offset() != 0 {
            target.extend(start.node_after()?);
            first += 1;
        }
    }
    for i in first..last {
        target.push(node.child(i).clone());
    }
    if let Some(end) = end
        && end.depth() == depth
        && end.text_offset() != 0
    {
        target.extend(end.node_before()?);
    }
    Ok(())
}

fn close(schema: &Schema, node: &Node, content: Fragment) -> Result<Node, Error> {
    if !schema.valid_content(&node.kind, &content) {
        return Err(Error(format!("Invalid content for node {}", node.kind)));
    }
    Ok(node.copy(content))
}

fn replace_three_way(
    schema: &Schema,
    from: &ResolvedPos,
    start: &ResolvedPos,
    end: &ResolvedPos,
    to: &ResolvedPos,
    depth: usize,
) -> Result<Fragment, Error> {
    let open_start = (from.depth() > depth)
        .then(|| joinable(schema, from, start, depth + 1))
        .transpose()?;
    let open_end = (to.depth() > depth)
        .then(|| joinable(schema, end, to, depth + 1))
        .transpose()?;

    let mut content = Vec::new();
    add_range(None, Some(from), depth, &mut content)?;
    if let (Some(left), Some(right)) = (open_start, open_end)
        && start.index(depth) == end.index(depth)
    {
        check_join(schema, left, right)?;
        let inner = replace_three_way(schema, from, start, end, to, depth + 1)?;
        content.push(close(schema, left, inner)?);
    } else {
        if let Some(left) = open_start {
            content.push(close(
                schema,
                left,
                replace_two_way(schema, from, start, depth + 1)?,
            )?);
        }
        add_range(Some(start), Some(end), depth, &mut content)?;
        if let Some(right) = open_end {
            content.push(close(
                schema,
                right,
                replace_two_way(schema, end, to, depth + 1)?,
            )?);
        }
    }
    add_range(Some(to), None, depth, &mut content)?;
    Ok(Fragment::from(content))
}

fn replace_two_way(
    schema: &Schema,
    from: &ResolvedPos,
    to: &ResolvedPos,
    depth: usize,
) -> Result<Fragment, Error> {
    let mut content = Vec::new();
    add_range(None, Some(from), depth, &mut content)?;
    if from.depth() > depth {
        let left = joinable(schema, from, to, depth + 1)?;
        content.push(close(
            schema,
            left,
            replace_two_way(schema, from, to, depth + 1)?,
        )?);
    }
    add_range(Some(to), None, depth, &mut content)?;
    Ok(Fragment::from(content))
}

fn prepare_slice_for_replace(
    slice: &Slice,
    along: &ResolvedPos,
) -> Result<(ResolvedPos, ResolvedPos), Error> {
    let extra = along.depth() - slice.open_start;
    let mut node = along.node(extra).copy(slice.content.clone());
    for i in (0..extra).rev() {
        node = along.node(i).copy(Fragment::from(node));
    }
    let start = node.resolve(slice.open_start + extra)?;
    let end = node.resolve(node.content.size() - slice.open_end - extra)?;
    Ok((start, end))
}
