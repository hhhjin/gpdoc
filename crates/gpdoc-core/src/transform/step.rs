use crate::model::{Error, Node, Schema, Slice};
use crate::transform::{
    AddMarkStep, AddNodeMarkStep, AttrStep, DocAttrStep, Mappable, RemoveMarkStep,
    RemoveNodeMarkStep, ReplaceAroundStep, ReplaceStep, StepMap,
};
use serde_json::{Value, json};

/// Atomic document changes, using the upstream step JSON identifiers.
#[derive(Clone, Debug, PartialEq)]
pub enum Step {
    Replace(ReplaceStep),
    ReplaceAround(ReplaceAroundStep),
    AddMark(AddMarkStep),
    RemoveMark(RemoveMarkStep),
    AddNodeMark(AddNodeMarkStep),
    RemoveNodeMark(RemoveNodeMarkStep),
    MarkSet(MarkSetStep),
    Attr(AttrStep),
    DocAttr(DocAttrStep),
}

/// Range mark operations have an identity position map. This constructor also
/// accepts nested groups produced by inverting a mark-set step.
#[derive(Clone, Debug, PartialEq)]
pub struct MarkSetStep(Vec<Step>);

impl MarkSetStep {
    pub fn steps(&self) -> &[Step] {
        &self.0
    }
}

impl Step {
    pub fn mark_set(steps: Vec<Step>) -> Result<Self, Error> {
        if steps
            .iter()
            .all(|s| matches!(s, Self::AddMark(_) | Self::RemoveMark(_) | Self::MarkSet(_)))
        {
            Ok(Self::MarkSet(MarkSetStep(steps)))
        } else {
            Err(Error("markSet only accepts range mark steps".into()))
        }
    }

    pub fn apply(&self, schema: &Schema, doc: &Node) -> Result<Node, Error> {
        match self {
            Self::Replace(s) => s.apply(schema, doc),
            Self::ReplaceAround(s) => s.apply(schema, doc),
            Self::AddMark(s) => s.apply(schema, doc),
            Self::RemoveMark(s) => s.apply(schema, doc),
            Self::AddNodeMark(s) => s.apply(schema, doc),
            Self::RemoveNodeMark(s) => s.apply(schema, doc),
            Self::MarkSet(MarkSetStep(steps)) => steps
                .iter()
                .try_fold(doc.clone(), |doc, step| step.apply(schema, &doc)),
            Self::Attr(s) => s.apply(schema, doc),
            Self::DocAttr(s) => s.apply(schema, doc),
        }
    }

    pub fn get_map(&self) -> StepMap {
        match self {
            Self::Replace(s) => s.get_map(),
            Self::ReplaceAround(s) => s.get_map(),
            _ => StepMap::default(),
        }
    }

    pub fn invert(&self, schema: &Schema, doc: &Node) -> Result<Self, Error> {
        match self {
            Self::Replace(s) => s.invert(doc).map(Self::Replace),
            Self::ReplaceAround(s) => s.invert(doc).map(Self::ReplaceAround),
            Self::AddMark(s) => s.invert(schema, doc),
            Self::RemoveMark(s) => s.invert(schema, doc),
            Self::AddNodeMark(s) => s.invert(schema, doc),
            Self::RemoveNodeMark(s) => s.invert(schema, doc),
            Self::MarkSet(MarkSetStep(steps)) => steps
                .iter()
                .map(|step| step.invert(schema, doc))
                .collect::<Result<_, _>>()
                .map(|steps| Self::MarkSet(MarkSetStep(steps))),
            Self::Attr(s) => s.invert(doc).map(Self::Attr),
            Self::DocAttr(s) => Ok(Self::DocAttr(s.invert(doc))),
        }
    }

    pub fn map(&self, mapping: &impl Mappable) -> Option<Self> {
        match self {
            Self::Replace(s) => s.map(mapping).map(Self::Replace),
            Self::ReplaceAround(s) => s.map(mapping).map(Self::ReplaceAround),
            Self::AddMark(s) => s.map(mapping).map(Self::AddMark),
            Self::RemoveMark(s) => s.map(mapping).map(Self::RemoveMark),
            Self::AddNodeMark(s) => s.map(mapping).map(Self::AddNodeMark),
            Self::RemoveNodeMark(s) => s.map(mapping).map(Self::RemoveNodeMark),
            Self::MarkSet(MarkSetStep(steps)) => {
                let mapped: Vec<_> = steps.iter().filter_map(|s| s.map(mapping)).collect();
                (!mapped.is_empty()).then_some(Self::MarkSet(MarkSetStep(mapped)))
            }
            Self::Attr(s) => s.map(mapping).map(Self::Attr),
            Self::DocAttr(_) => Some(self.clone()),
        }
    }

    pub fn merge(&self, other: &Self) -> Option<Self> {
        match (self, other) {
            (Self::Replace(a), Self::Replace(b)) => a.merge(b).map(Self::Replace),
            (Self::AddMark(a), Self::AddMark(b)) => a.merge(b).map(Self::AddMark),
            (Self::RemoveMark(a), Self::RemoveMark(b)) => a.merge(b).map(Self::RemoveMark),
            _ => None,
        }
    }

    pub fn to_json(&self) -> Value {
        match self {
            Self::Replace(s) => with_slice(
                json!({"stepType":"replace", "from":s.from, "to":s.to}),
                &s.slice,
                s.structure,
            ),
            Self::ReplaceAround(s) => with_slice(
                json!({"stepType":"replaceAround", "from":s.from, "to":s.to, "gapFrom":s.gap_from, "gapTo":s.gap_to, "insert":s.insert}),
                &s.slice,
                s.structure,
            ),
            Self::AddMark(s) => {
                json!({"stepType":"addMark", "from":s.from,"to":s.to,"mark":s.mark.to_json()})
            }
            Self::RemoveMark(s) => {
                json!({"stepType":"removeMark", "from":s.from,"to":s.to,"mark":s.mark.to_json()})
            }
            Self::AddNodeMark(s) => {
                json!({"stepType":"addNodeMark", "pos":s.pos,"mark":s.mark.to_json()})
            }
            Self::RemoveNodeMark(s) => {
                json!({"stepType":"removeNodeMark", "pos":s.pos,"mark":s.mark.to_json()})
            }
            Self::MarkSet(MarkSetStep(steps)) => {
                json!({"stepType":"markSet","steps":steps.iter().map(Step::to_json).collect::<Vec<_>>()})
            }
            Self::Attr(s) => json!({"stepType":"attr", "pos":s.pos,"attr":s.attr,"value":s.value}),
            Self::DocAttr(s) => json!({"stepType":"docAttr", "attr":s.attr,"value":s.value}),
        }
    }

    pub fn from_json(schema: &Schema, json: &Value) -> Result<Self, Error> {
        let position = |key: &str| {
            json[key]
                .as_u64()
                .and_then(|n| usize::try_from(n).ok())
                .ok_or_else(|| Error(format!("Invalid step position {key}")))
        };
        let range = || {
            let from = position("from")?;
            let to = position("to")?;
            if from > to {
                Err(Error("Reversed step range".into()))
            } else {
                Ok((from, to))
            }
        };
        let attr = || {
            json["attr"]
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| Error("Invalid step attribute".into()))
        };
        let mark = || schema.mark_from_json(&json["mark"]);
        let structure = json["structure"].as_bool().unwrap_or(false);
        Ok(match json["stepType"].as_str() {
            Some("replace") => {
                let (from, to) = range()?;
                let step = ReplaceStep {
                    from,
                    to,
                    slice: Slice::from_json(schema, &json["slice"])?,
                    structure,
                };
                step.check_ranges()?;
                Self::Replace(step)
            }
            Some("replaceAround") => {
                let (from, to) = range()?;
                let step = ReplaceAroundStep {
                    from,
                    to,
                    gap_from: position("gapFrom")?,
                    gap_to: position("gapTo")?,
                    insert: position("insert")?,
                    slice: Slice::from_json(schema, &json["slice"])?,
                    structure,
                };
                step.check_ranges()?;
                Self::ReplaceAround(step)
            }
            Some("addMark") => {
                let (from, to) = range()?;
                Self::AddMark(AddMarkStep {
                    from,
                    to,
                    mark: mark()?,
                })
            }
            Some("removeMark") => {
                let (from, to) = range()?;
                Self::RemoveMark(RemoveMarkStep {
                    from,
                    to,
                    mark: mark()?,
                })
            }
            Some("addNodeMark") => Self::AddNodeMark(AddNodeMarkStep {
                pos: position("pos")?,
                mark: mark()?,
            }),
            Some("removeNodeMark") => Self::RemoveNodeMark(RemoveNodeMarkStep {
                pos: position("pos")?,
                mark: mark()?,
            }),
            Some("markSet") => {
                let items = json["steps"]
                    .as_array()
                    .ok_or_else(|| Error("Invalid JSON for MarkSetStep".into()))?;
                if !items
                    .iter()
                    .all(|step| matches!(step["stepType"].as_str(), Some("addMark" | "removeMark")))
                {
                    return Err(Error("Invalid JSON for MarkSetStep".into()));
                }
                Self::MarkSet(MarkSetStep(
                    items
                        .iter()
                        .map(|item| Self::from_json(schema, item))
                        .collect::<Result<_, _>>()?,
                ))
            }
            Some("attr") => Self::Attr(AttrStep {
                pos: position("pos")?,
                attr: attr()?,
                value: json["value"].clone(),
            }),
            Some("docAttr") => Self::DocAttr(DocAttrStep {
                attr: attr()?,
                value: json["value"].clone(),
            }),
            _ => return Err(Error(format!("Unknown step type {}", json["stepType"]))),
        })
    }
}

fn with_slice(mut json: Value, slice: &Slice, structure: bool) -> Value {
    if slice.size() != 0 {
        json["slice"] = slice.to_json();
    }
    if structure {
        json["structure"] = true.into();
    }
    json
}
