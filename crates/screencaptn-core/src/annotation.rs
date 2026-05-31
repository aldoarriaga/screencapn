use crate::geometry::{Point, Rect};
use crate::style::{Color, HighlightShape, StrokeStyle};

pub type AnnotationId = u64;

#[derive(Clone, Debug, PartialEq)]
pub enum AnnotationKind {
    Rectangle,
    Oval,
    Line {
        start: Point,
        end: Point,
    },
    Arrow {
        start: Point,
        end: Point,
    },
    StepNumber {
        number: u32,
    },
    Text {
        text: String,
        font_size: f32,
        framed: bool,
        filled: bool,
    },
    Tag {
        label: String,
        anchor: Point,
        font_size: f32,
    },
    Mosaic {
        mode: MosaicMode,
        brush_size: f32,
    },
    Highlighter {
        shape: HighlightShape,
        opacity: f32,
        start: Point,
        end: Point,
    },
    Pen {
        points: Vec<Point>,
    },
    PenArrow {
        points: Vec<Point>,
    },
    Watermark {
        text: String,
        opacity: f32,
    },
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
    pub step_number: Option<u32>,
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
            step_number: None,
        }
    }

    pub fn display_step_number(&self) -> Option<u32> {
        self.step_number.or(match &self.kind {
            AnnotationKind::StepNumber { number } => Some(*number),
            _ => None,
        })
    }

    pub fn accepts_auto_numbering(&self) -> bool {
        !matches!(
            self.kind,
            AnnotationKind::StepNumber { .. } | AnnotationKind::Watermark { .. }
        )
    }

    pub fn translated(&self, dx: f32, dy: f32) -> Self {
        let mut next = self.clone();
        next.bounds = next.bounds.translate(dx, dy);
        match &mut next.kind {
            AnnotationKind::Line { start, end } | AnnotationKind::Arrow { start, end } => {
                *start = start.translate(dx, dy);
                *end = end.translate(dx, dy);
            }
            AnnotationKind::Highlighter { start, end, .. } => {
                *start = start.translate(dx, dy);
                *end = end.translate(dx, dy);
            }
            AnnotationKind::Tag { anchor, .. } => {
                *anchor = anchor.translate(dx, dy);
            }
            AnnotationKind::Pen { points } | AnnotationKind::PenArrow { points } => {
                for point in points {
                    *point = point.translate(dx, dy);
                }
            }
            _ => {}
        }
        next
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn annotation(kind: AnnotationKind) -> Annotation {
        Annotation::new(
            1,
            kind,
            Rect::new(0.0, 0.0, 10.0, 10.0),
            StrokeStyle::default(),
        )
    }

    #[test]
    fn explicit_step_badge_overrides_kind_number() {
        let mut item = annotation(AnnotationKind::StepNumber { number: 3 });
        item.step_number = Some(9);

        assert_eq!(item.display_step_number(), Some(9));
    }

    #[test]
    fn step_tool_number_is_display_number() {
        let item = annotation(AnnotationKind::StepNumber { number: 4 });

        assert_eq!(item.display_step_number(), Some(4));
    }

    #[test]
    fn watermark_and_manual_step_are_not_auto_numbered() {
        assert!(!annotation(AnnotationKind::StepNumber { number: 1 }).accepts_auto_numbering());
        assert!(!annotation(AnnotationKind::Watermark {
            text: String::new(),
            opacity: 0.0
        })
        .accepts_auto_numbering());
        assert!(annotation(AnnotationKind::Rectangle).accepts_auto_numbering());
    }
}
