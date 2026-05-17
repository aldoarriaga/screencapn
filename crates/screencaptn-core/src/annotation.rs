use crate::geometry::{Point, Rect};
use crate::style::{Color, HighlightShape, StrokeStyle};

pub type AnnotationId = u64;

#[derive(Clone, Debug, PartialEq)]
pub enum AnnotationKind {
    Rectangle,
    Oval,
    Line { start: Point, end: Point },
    Arrow { start: Point, end: Point },
    StepNumber { number: u32 },
    Text { text: String, font_size: f32 },
    Tag { label: String, anchor: Point },
    Mosaic { mode: MosaicMode, brush_size: f32 },
    Highlighter { shape: HighlightShape, opacity: f32 },
    Pen { points: Vec<Point> },
    Watermark { text: String, opacity: f32 },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MosaicMode {
    Area,
    Brush,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Annotation {
    pub id: AnnotationId,
    pub bounds: Rect,
    pub stroke: StrokeStyle,
    pub fill: Option<Color>,
    pub kind: AnnotationKind,
}

impl Annotation {
    pub fn new(id: AnnotationId, kind: AnnotationKind, bounds: Rect, stroke: StrokeStyle) -> Self {
        Self {
            id,
            kind,
            bounds,
            stroke,
            fill: None,
        }
    }

    pub fn translated(&self, dx: f32, dy: f32) -> Self {
        let mut next = self.clone();
        next.bounds = next.bounds.translate(dx, dy);
        match &mut next.kind {
            AnnotationKind::Line { start, end } | AnnotationKind::Arrow { start, end } => {
                *start = start.translate(dx, dy);
                *end = end.translate(dx, dy);
            }
            AnnotationKind::Tag { anchor, .. } => {
                *anchor = anchor.translate(dx, dy);
            }
            AnnotationKind::Pen { points } => {
                for point in points {
                    *point = point.translate(dx, dy);
                }
            }
            _ => {}
        }
        next
    }
}
