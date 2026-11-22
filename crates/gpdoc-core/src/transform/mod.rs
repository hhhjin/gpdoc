mod attr_step;
mod edit;
mod error;
mod fit;
mod map;
mod mark_step;
mod replace_step;
mod step;

pub use attr_step::{AttrStep, DocAttrStep};
pub use fit::replace_step;
pub use map::{MapResult, Mappable, Mapping, StepMap};
pub use mark_step::{AddMarkStep, AddNodeMarkStep, RemoveMarkStep, RemoveNodeMarkStep};
pub use replace_step::{ReplaceAroundStep, ReplaceStep};
pub use step::{MarkSetStep, Step};

pub use edit::{
    Affinity, Boundary, BoundaryEdit, Edit, MappedPosition, PositionMap, apply_edit, join,
    replace_text, split, wrap,
};
pub use error::TransformError;
