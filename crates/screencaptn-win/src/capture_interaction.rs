use screencaptn_core::Rect;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CapturePhase {
    SelectingRegion,
    Annotating,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PointerClaims {
    pub selected_annotation: bool,
    pub annotation: bool,
    pub region_handle: bool,
    pub region_frame: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PointerIntent {
    SelectedAnnotation,
    Annotation,
    RegionHandle,
    RegionFrame,
    AnnotationCanvas,
    RegionSelection,
}

pub fn resolve_pointer_intent(phase: CapturePhase, claims: PointerClaims) -> PointerIntent {
    if claims.selected_annotation {
        return PointerIntent::SelectedAnnotation;
    }
    if claims.annotation {
        return PointerIntent::Annotation;
    }
    if claims.region_handle {
        return PointerIntent::RegionHandle;
    }
    if claims.region_frame {
        return PointerIntent::RegionFrame;
    }
    match phase {
        CapturePhase::SelectingRegion => PointerIntent::RegionSelection,
        CapturePhase::Annotating => PointerIntent::AnnotationCanvas,
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ResponsiveMetrics {
    pub visual_scale: f32,
    pub ui_scale: f32,
    pub font_min: f32,
    pub font_default: f32,
    pub font_max: f32,
    pub default_stroke: f32,
    pub region_handle_hit_radius: f32,
    pub region_frame_hit_width: f32,
    pub annotation_handle_hit_radius: f32,
    pub crosshair_length: f32,
    pub crosshair_gap: f32,
    pub crosshair_width: f32,
}

impl ResponsiveMetrics {
    pub fn for_screen(screen: Rect) -> Self {
        let width_ratio = (screen.width / 3840.0).max(0.0);
        let height_ratio = (screen.height / 2160.0).max(0.0);
        let resolution_ratio = width_ratio.min(height_ratio);
        let visual_scale = resolution_ratio.powf(1.35).clamp(0.42, 1.0);
        let ui_scale = visual_scale * 2.76;

        Self {
            visual_scale,
            ui_scale,
            font_min: (12.0 * visual_scale).round().clamp(8.0, 12.0),
            font_default: (27.0 * visual_scale).round().clamp(14.0, 27.0),
            font_max: (56.0 * visual_scale).round().clamp(40.0, 56.0),
            default_stroke: (8.0 * visual_scale).max(3.0),
            region_handle_hit_radius: (38.0 * visual_scale).max(20.0),
            region_frame_hit_width: (32.0 * visual_scale).max(8.0),
            annotation_handle_hit_radius: (26.0 * visual_scale).max(16.0),
            crosshair_length: (75.6 * visual_scale).clamp(48.0, 75.6),
            crosshair_gap: (26.0 * visual_scale).max(12.0),
            crosshair_width: (2.0 * visual_scale).max(1.0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selected_annotations_win_over_region_targets() {
        let intent = resolve_pointer_intent(
            CapturePhase::Annotating,
            PointerClaims {
                selected_annotation: true,
                annotation: true,
                region_handle: true,
                region_frame: true,
            },
        );

        assert_eq!(intent, PointerIntent::SelectedAnnotation);
    }

    #[test]
    fn confirmed_capture_uses_annotation_canvas_outside_region() {
        assert_eq!(
            resolve_pointer_intent(CapturePhase::Annotating, PointerClaims::default()),
            PointerIntent::AnnotationCanvas
        );
        assert_eq!(
            resolve_pointer_intent(CapturePhase::SelectingRegion, PointerClaims::default()),
            PointerIntent::RegionSelection
        );
    }

    #[test]
    fn responsive_metrics_preserve_four_k_and_shrink_ultrawide_ui() {
        let four_k = ResponsiveMetrics::for_screen(Rect::new(0.0, 0.0, 3840.0, 2160.0));
        let ultrawide = ResponsiveMetrics::for_screen(Rect::new(0.0, 0.0, 3440.0, 1440.0));
        let small = ResponsiveMetrics::for_screen(Rect::new(0.0, 0.0, 1366.0, 768.0));

        assert_eq!(four_k.visual_scale, 1.0);
        assert_eq!(four_k.font_default, 27.0);
        assert!(ultrawide.ui_scale < four_k.ui_scale);
        assert!(small.ui_scale <= ultrawide.ui_scale);
        assert!(small.font_min >= 8.0);
        assert!(small.region_handle_hit_radius >= 20.0);
    }
}
