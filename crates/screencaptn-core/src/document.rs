use crate::annotation::{Annotation, AnnotationId};
use crate::geometry::Rect;

#[derive(Clone, Debug, PartialEq)]
pub struct CaptureDocument {
    pub capture_region: Option<Rect>,
    pub annotations: Vec<Annotation>,
    pub selected_annotation_id: Option<AnnotationId>,
    next_annotation_id: AnnotationId,
}

impl Default for CaptureDocument {
    fn default() -> Self {
        Self::new()
    }
}

impl CaptureDocument {
    pub fn new() -> Self {
        Self {
            capture_region: None,
            annotations: Vec::new(),
            selected_annotation_id: None,
            next_annotation_id: 1,
        }
    }

    pub fn set_capture_region(&mut self, region: Rect) {
        self.capture_region = region.is_visible().then_some(region);
    }

    pub fn reserve_annotation_id(&mut self) -> AnnotationId {
        let id = self.next_annotation_id;
        self.next_annotation_id += 1;
        id
    }

    pub fn add_annotation(&mut self, mut annotation: Annotation) -> AnnotationId {
        if annotation.id == 0 {
            annotation.id = self.reserve_annotation_id();
        } else {
            self.next_annotation_id = self.next_annotation_id.max(annotation.id + 1);
        }
        let id = annotation.id;
        self.selected_annotation_id = Some(id);
        self.annotations.push(annotation);
        id
    }

    pub fn remove_selected(&mut self) -> Option<Annotation> {
        let selected = self.selected_annotation_id?;
        let index = self
            .annotations
            .iter()
            .position(|annotation| annotation.id == selected)?;
        self.selected_annotation_id = None;
        Some(self.annotations.remove(index))
    }

    pub fn select_at(&mut self, x: f32, y: f32) -> Option<AnnotationId> {
        let hit = self
            .annotations
            .iter()
            .rev()
            .find(|annotation| annotation.bounds.contains(crate::Point::new(x, y)))
            .map(|annotation| annotation.id);
        self.selected_annotation_id = hit;
        hit
    }

    pub fn annotation(&self, id: AnnotationId) -> Option<&Annotation> {
        self.annotations
            .iter()
            .find(|annotation| annotation.id == id)
    }

    pub fn annotation_mut(&mut self, id: AnnotationId) -> Option<&mut Annotation> {
        self.annotations
            .iter_mut()
            .find(|annotation| annotation.id == id)
    }

    pub fn selected(&self) -> Option<&Annotation> {
        self.annotation(self.selected_annotation_id?)
    }

    pub fn selected_mut(&mut self) -> Option<&mut Annotation> {
        self.annotation_mut(self.selected_annotation_id?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AnnotationKind, Color, StrokeStyle};

    #[test]
    fn add_annotation_assigns_stable_id() {
        let mut doc = CaptureDocument::new();
        let annotation = Annotation::new(
            0,
            AnnotationKind::Rectangle,
            Rect::new(1.0, 2.0, 30.0, 40.0),
            StrokeStyle::new(2.0, Color::RED),
        );
        let id = doc.add_annotation(annotation);
        assert_eq!(id, 1);
        assert_eq!(doc.selected_annotation_id, Some(1));
    }
}
