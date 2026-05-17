pub mod annotation;
pub mod document;
pub mod geometry;
pub mod history;
pub mod settings;
pub mod style;

pub use annotation::{Annotation, AnnotationId, AnnotationKind, MosaicMode};
pub use document::CaptureDocument;
pub use geometry::{Point, Rect, ResizeHandle, Size};
pub use history::History;
pub use settings::{Hotkey, Settings};
pub use style::{Color, HighlightShape, StrokeStyle, ToolKind};
