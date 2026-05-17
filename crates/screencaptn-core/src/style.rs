#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const BLACK: Self = Self::rgb(0, 0, 0);
    pub const BLUE: Self = Self::rgb(41, 98, 255);
    pub const RED: Self = Self::rgb(230, 57, 70);
    pub const YELLOW: Self = Self::rgba(255, 214, 10, 130);
    pub const WHITE: Self = Self::rgb(255, 255, 255);

    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StrokeStyle {
    pub width: f32,
    pub color: Color,
    pub opacity: f32,
}

impl StrokeStyle {
    pub const fn new(width: f32, color: Color) -> Self {
        Self {
            width,
            color,
            opacity: 1.0,
        }
    }
}

impl Default for StrokeStyle {
    fn default() -> Self {
        Self::new(3.0, Color::BLUE)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HighlightShape {
    Rectangle,
    Oval,
    RoundedRectangle,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ToolKind {
    StepNumber,
    Rectangle,
    Oval,
    Line,
    Arrow,
    Pen,
    Text,
    Tag,
    Mosaic,
    Highlighter,
    Watermark,
}

impl ToolKind {
    pub const ALL: [ToolKind; 11] = [
        ToolKind::StepNumber,
        ToolKind::Rectangle,
        ToolKind::Oval,
        ToolKind::Line,
        ToolKind::Arrow,
        ToolKind::Pen,
        ToolKind::Text,
        ToolKind::Tag,
        ToolKind::Mosaic,
        ToolKind::Highlighter,
        ToolKind::Watermark,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ToolKind::StepNumber => "Step",
            ToolKind::Rectangle => "Rect",
            ToolKind::Oval => "Oval",
            ToolKind::Line => "Line",
            ToolKind::Arrow => "Arrow",
            ToolKind::Pen => "Pen",
            ToolKind::Text => "Text",
            ToolKind::Tag => "Tag",
            ToolKind::Mosaic => "Mosaic",
            ToolKind::Highlighter => "Highlight",
            ToolKind::Watermark => "Watermark",
        }
    }
}
