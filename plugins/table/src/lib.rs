//! Tiptap table nodes and headless table editing commands.
use gpdoc_plugin::prelude::*;
mod grid;
mod selection;
mod table_map;
#[cfg(feature = "gpui")]
mod view;
use gpdoc_plugin::selection::path_at;
use grid::{Grid, Operation};
pub use selection::{cell_selection, selected_cell_endpoints};
use std::num::NonZeroUsize;
pub use table_map::TableMap;

pub use grid::ResizeColumn;
#[cfg(feature = "gpui")]
pub use view::{TableCell, default_cell, default_resize_guide, table_grid};

fn cell_type(name: &str) -> NodeSpec {
    let attrs = AttrSpecs::from([
        (
            "colspan".into(),
            AttrSpec::with_default(AttrType::Integer, Value::from(1)),
        ),
        (
            "rowspan".into(),
            AttrSpec::with_default(AttrType::Integer, Value::from(1)),
        ),
        (
            "colwidth".into(),
            AttrSpec::with_default(AttrType::Json, Value::Null),
        ),
        (
            "align".into(),
            AttrSpec::with_default(AttrType::NullableString, Value::Null),
        ),
    ]);
    NodeSpec::block(name).content("block+").attrs(attrs)
}

unit_command!(pub Fix, "fixTables");
unit_command!(pub Delete, "deleteTable");
unit_command!(pub DeleteSelected, "deleteSelectedTable");
unit_command!(pub PreviousCell, "goToPreviousCell");
unit_command!(pub Tab, "tableTab");
unit_command!(pub AddColumnBefore, "addColumnBefore");
unit_command!(pub AddColumnAfter, "addColumnAfter");
unit_command!(pub AddRowBefore, "addRowBefore");
unit_command!(pub AddRowAfter, "addRowAfter");
unit_command!(pub DeleteColumn, "deleteColumn");
unit_command!(pub DeleteRow, "deleteRow");
unit_command!(pub ToggleHeaderRow, "toggleHeaderRow");
unit_command!(pub ToggleHeaderColumn, "toggleHeaderColumn");
unit_command!(pub ToggleHeaderCell, "toggleHeaderCell");
unit_command!(pub GoToNextCell, "goToNextCell");
unit_command!(pub SplitCell, "splitCell");
unit_command!(pub MergeOrSplit, "mergeOrSplit");
unit_command!(pub MergeCells, "mergeCells");

impl Command for ResizeColumn {
    const NAME: &'static str = "setTableColumnWidth";
}

pub fn table() -> Plugin {
    let plugin = Plugin::new("table", |p| {
        p.node(NodeSpec::block("table").group("block").content("tableRow+"));
        p.drag_handle_for_subtree("table");
        p.node(NodeSpec::block("tableRow").content("(tableCell | tableHeader)*"));
        p.node(cell_type("tableCell"));
        p.node(cell_type("tableHeader"));
        p.selection(selection::CELL_SELECTION);
        p.append_edits(|change| {
            Ok(
                if change
                    .before
                    .is_some_and(|before| before != change.document)
                {
                    Grid::repairs(change.document)
                } else {
                    Vec::new()
                },
            )
        });
        p.command::<Fix>(|ctx, _| {
            let mut tx = ctx.transaction();
            for step in Grid::repairs(ctx.document()) {
                tx.push(step);
            }
            Ok(Some(tx))
        });
        p.command::<Insert>(insert_table);
        p.command::<Delete>(|ctx, _| delete_table(ctx));
        p.command::<DeleteSelected>(|ctx, _| {
            let Some((anchor, head)) = selected_cell_endpoints(ctx.selection()) else {
                return Ok(None);
            };
            let doc = ctx.document();
            let Some(table) = path_at(doc.blocks(), anchor)
                .into_iter()
                .rev()
                .find(|n| n.node.kind == "table")
            else {
                return Ok(None);
            };
            let Some(map) = TableMap::read(table.node, DocPos(table.before)) else {
                return Ok(None);
            };
            if !TableMap::selected_cells(doc, anchor, head)
                .is_some_and(|selected| selected.len() == map.cells.len())
            {
                return Ok(None);
            }
            delete_table(ctx)
        });
        for key in ["Backspace", "Mod-Backspace", "Delete", "Mod-Delete"] {
            p.shortcut(key, DeleteSelected);
        }
        p.command::<SetSelection>(set_cell_selection);
        p.command::<SetCellAttribute>(|ctx, arg| {
            Grid::command(ctx, Operation::SetAttribute(&arg.name, &arg.value))
        });
        p.command::<ResizeColumn>(Grid::resize_column);
        p.command::<AddColumnBefore>(|ctx, _| Grid::command(ctx, Operation::AddColumn(false)));
        p.command::<AddColumnAfter>(|ctx, _| Grid::command(ctx, Operation::AddColumn(true)));
        p.command::<AddRowBefore>(|ctx, _| Grid::command(ctx, Operation::AddRow(false)));
        p.command::<AddRowAfter>(|ctx, _| Grid::command(ctx, Operation::AddRow(true)));
        p.command::<DeleteColumn>(|ctx, _| Grid::command(ctx, Operation::DeleteColumn));
        p.command::<DeleteRow>(|ctx, _| Grid::command(ctx, Operation::DeleteRow));
        p.command::<ToggleHeaderRow>(|ctx, _| Grid::command(ctx, Operation::HeaderRow));
        p.command::<ToggleHeaderColumn>(|ctx, _| Grid::command(ctx, Operation::HeaderColumn));
        p.command::<ToggleHeaderCell>(|ctx, _| Grid::command(ctx, Operation::HeaderCell));
        p.command::<GoToNextCell>(|ctx, _| Grid::command(ctx, Operation::Move(true)));
        p.command::<SplitCell>(|ctx, _| Grid::command(ctx, Operation::Split));
        p.command::<MergeOrSplit>(|ctx, _| Grid::command(ctx, Operation::MergeOrSplit));
        p.command::<MergeCells>(|ctx, _| Grid::command(ctx, Operation::Merge));
        p.command::<PreviousCell>(|ctx, _| Grid::command(ctx, Operation::Move(false)));
        p.shortcut("Shift-Tab", PreviousCell);
        p.command::<Tab>(|ctx, _| {
            if let Some(tx) = Grid::command(ctx, Operation::Move(true))? {
                return Ok(Some(tx));
            }
            ctx.plan_chain([
                StoredCommand::new(AddRowAfter),
                StoredCommand::new(GoToNextCell),
            ])
        });
        p.shortcut("Tab", Tab);
    });
    #[cfg(feature = "gpui")]
    let plugin = gpdoc::PluginViewExt::gpui_default_view(plugin, view::plain_grid);
    plugin
}

fn selection_range(ctx: CommandContext<'_>) -> (usize, usize) {
    let range = ctx.selection().bounds(ctx.document());
    (range.start.0, range.end.0)
}
fn empty_cell(header: bool) -> Node {
    Node::container(
        if header { "tableHeader" } else { "tableCell" },
        vec![Node::text("paragraph", "")],
    )
}

#[derive(Clone, Debug)]
pub struct Insert {
    pub rows: NonZeroUsize,
    pub columns: NonZeroUsize,
    pub with_header_row: bool,
}

impl Command for Insert {
    const NAME: &'static str = "insertTable";
}

impl Default for Insert {
    fn default() -> Self {
        Self {
            rows: NonZeroUsize::new(3).unwrap(),
            columns: NonZeroUsize::new(3).unwrap(),
            with_header_row: true,
        }
    }
}

fn insert_table(ctx: CommandContext<'_>, args: Insert) -> Result<Option<Transaction>, Error> {
    let (rows, cols) = (args.rows.get(), args.columns.get());
    // Bound the size of a document created in one operation.
    if rows
        .checked_mul(cols)
        .is_none_or(|count| count > TableMap::MAX_SLOTS)
    {
        return Err(Error::InvalidCommandArguments {
            command: Insert::NAME.into(),
            reason: format!("a table may contain at most {} cells", TableMap::MAX_SLOTS),
        });
    }
    let row = |header| Node::container("tableRow", vec![empty_cell(header); cols]);
    let mut children = vec![row(false); rows];
    if args.with_header_row {
        children[0] = row(true);
    }
    let node = ctx
        .schema()
        .bind_node(&Node::container("table", children))?;
    let schema = ctx.schema();
    let doc = ctx.document().root();
    let (start, end) = selection_range(ctx);
    let from = doc.resolve(start)?;
    let to = doc.resolve(end)?;
    let range = if from.depth() > 0
        && from.same_parent(&to)
        && schema
            .node_type(&from.parent().kind)
            .is_some_and(|ty| ty.is_textblock())
        && from.parent_offset == 0
        && to.parent_offset == to.parent().content.size()
    {
        (from.before(from.depth())?, to.after(to.depth())?)
    } else if start == end && from.depth() > 0 && from.parent_offset == 0 {
        let before = from.before(from.depth())?;
        (before, before)
    } else {
        (start, end)
    };
    let slice = Slice::new(Fragment::from(vec![node]), 0, 0);
    let Some(step) = Step::fit_replace(schema, doc, range.0, range.1, &slice)? else {
        return Ok(None);
    };
    let (next, _) = step.preview(ctx.document())?;
    let next = next.root();
    let mut caret = None;
    next.nodes_between(
        start.saturating_add(1).min(next.content.size()),
        next.content.size(),
        |node, pos, _, _| {
            if caret.is_none()
                && schema
                    .node_type(&node.kind)
                    .is_some_and(|ty| ty.is_textblock())
                && pos >= start.saturating_sub(1)
            {
                caret = Some(pos + 1);
            }
            caret.is_none()
        },
    )?;
    let Some(caret) = caret else { return Ok(None) };
    let mut tx = ctx.transaction();
    tx.push(step);
    tx.select(Selection::caret(DocPos(caret)));
    Ok(Some(tx))
}

fn delete_table(ctx: CommandContext<'_>) -> Result<Option<Transaction>, Error> {
    let path = path_at(ctx.document().blocks(), DocPos(selection_range(ctx).0));
    let Some(table) = path.iter().rev().find(|n| n.node.kind == "table") else {
        return Ok(None);
    };
    let doc = ctx.document().root();
    let Some(step) = Step::fit_replace(
        ctx.schema(),
        doc,
        table.before,
        table.end(),
        &Slice::empty(),
    )?
    else {
        return Ok(None);
    };
    let mut tx = ctx.transaction();
    tx.push(step);
    Ok(Some(tx))
}

#[derive(Clone, Debug)]
pub struct SetCellAttribute {
    pub name: String,
    pub value: Value,
}

impl Command for SetCellAttribute {
    const NAME: &'static str = "setCellAttribute";
}

#[derive(Clone, Debug)]
pub struct SetSelection {
    pub anchor_cell: usize,
    pub head_cell: Option<usize>,
}

impl Command for SetSelection {
    const NAME: &'static str = "setCellSelection";
}

fn set_cell_selection(
    ctx: CommandContext<'_>,
    args: SetSelection,
) -> Result<Option<Transaction>, Error> {
    let anchor = DocPos(args.anchor_cell);
    let head = DocPos(args.head_cell.unwrap_or(args.anchor_cell));
    let selection = cell_selection(anchor, head);
    if selection.validate(ctx.document()).is_err() {
        return Ok(None);
    }
    let mut tx = ctx.transaction();
    tx.select(selection);
    Ok(Some(tx))
}
