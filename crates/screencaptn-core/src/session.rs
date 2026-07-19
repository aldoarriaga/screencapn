use crate::{Annotation, AnnotationId, CaptureDocument, History, Rect};

/// The authoritative, behavior-focused state for one capture overlay session.
///
/// Platform adapters may render the document directly, but all document and
/// history transitions should enter through this module so undo/selection
/// policy has one home.
#[derive(Clone, Debug)]
pub struct CaptureSession {
    pub document: CaptureDocument,
    history: History<CaptureDocument>,
}

impl CaptureSession {
    pub fn with_history_limit(limit: usize, weight_limit: usize) -> Self {
        Self {
            document: CaptureDocument::new(),
            history: History::with_weight_limit(
                limit,
                weight_limit,
                CaptureDocument::estimated_history_bytes,
            ),
        }
    }

    pub fn checkpoint(&mut self) {
        self.history.checkpoint(&self.document);
    }

    pub fn set_capture_region(&mut self, region: Rect) {
        self.document.set_capture_region(region);
    }

    pub fn clear_capture_region(&mut self) {
        self.document.capture_region = None;
        self.document.selected_annotation_id = None;
    }

    pub fn select_annotation(&mut self, id: Option<AnnotationId>) -> bool {
        if self.document.selected_annotation_id == id {
            return false;
        }
        self.document.selected_annotation_id = id;
        true
    }

    pub fn deselect_annotation(&mut self) -> bool {
        self.select_annotation(None)
    }

    pub fn add_annotation(&mut self, annotation: Annotation) -> AnnotationId {
        self.document.add_annotation(annotation)
    }

    pub fn remove_selected_annotation(&mut self) -> Option<Annotation> {
        self.document.remove_selected()
    }

    pub fn undo(&mut self) -> bool {
        let Some(previous) = self.history.undo(&self.document) else {
            return false;
        };
        self.document = previous;
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(next) = self.history.redo(&self.document) else {
            return false;
        };
        self.document = next;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AnnotationKind, Color, StrokeStyle};

    fn rectangle() -> Annotation {
        Annotation::new(
            0,
            AnnotationKind::Rectangle,
            Rect::new(10.0, 20.0, 30.0, 40.0),
            StrokeStyle::new(2.0, Color::RED),
        )
    }

    #[test]
    fn annotation_lifecycle_undoes_through_the_session() {
        let mut session = CaptureSession::with_history_limit(10, 1024 * 1024);
        session.set_capture_region(Rect::new(0.0, 0.0, 200.0, 100.0));
        session.checkpoint();

        let id = session.add_annotation(rectangle());
        assert_eq!(session.document.selected_annotation_id, Some(id));
        assert_eq!(session.document.annotations.len(), 1);

        assert!(session.undo());
        assert!(session.document.annotations.is_empty());
        assert_eq!(
            session.document.capture_region,
            Some(Rect::new(0.0, 0.0, 200.0, 100.0))
        );
    }

    #[test]
    fn selection_and_removal_stay_local_to_the_session() {
        let mut session = CaptureSession::with_history_limit(10, 1024 * 1024);
        let id = session.add_annotation(rectangle());

        assert!(session.deselect_annotation());
        assert_eq!(session.document.selected_annotation_id, None);
        assert!(!session.deselect_annotation());

        assert!(session.select_annotation(Some(id)));
        assert_eq!(
            session
                .remove_selected_annotation()
                .map(|annotation| annotation.id),
            Some(id)
        );
        assert!(session.document.annotations.is_empty());
    }

    #[test]
    fn clearing_the_region_also_clears_selection() {
        let mut session = CaptureSession::with_history_limit(10, 1024 * 1024);
        session.set_capture_region(Rect::new(0.0, 0.0, 200.0, 100.0));
        session.add_annotation(rectangle());

        session.clear_capture_region();

        assert_eq!(session.document.capture_region, None);
        assert_eq!(session.document.selected_annotation_id, None);
    }
}
