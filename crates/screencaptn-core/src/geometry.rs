#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl Point {
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    pub fn translate(self, dx: f32, dy: f32) -> Self {
        Self::new(self.x + dx, self.y + dy)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Size {
    pub width: f32,
    pub height: f32,
}

impl Size {
    pub const fn new(width: f32, height: f32) -> Self {
        Self { width, height }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    pub const fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub fn from_points(a: Point, b: Point) -> Self {
        let x = a.x.min(b.x);
        let y = a.y.min(b.y);
        let width = (a.x - b.x).abs();
        let height = (a.y - b.y).abs();
        Self::new(x, y, width, height)
    }

    pub fn right(self) -> f32 {
        self.x + self.width
    }

    pub fn bottom(self) -> f32 {
        self.y + self.height
    }

    pub fn center(self) -> Point {
        Point::new(self.x + self.width / 2.0, self.y + self.height / 2.0)
    }

    pub fn contains(self, point: Point) -> bool {
        point.x >= self.x
            && point.x <= self.right()
            && point.y >= self.y
            && point.y <= self.bottom()
    }

    pub fn is_visible(self) -> bool {
        self.width >= 2.0 && self.height >= 2.0
    }

    pub fn translate(self, dx: f32, dy: f32) -> Self {
        Self::new(self.x + dx, self.y + dy, self.width, self.height)
    }

    pub fn resize_from_handle(self, handle: ResizeHandle, to: Point, min_size: f32) -> Self {
        let mut left = self.x;
        let mut top = self.y;
        let mut right = self.right();
        let mut bottom = self.bottom();

        match handle {
            ResizeHandle::NorthWest => {
                left = to.x.min(right - min_size);
                top = to.y.min(bottom - min_size);
            }
            ResizeHandle::North => top = to.y.min(bottom - min_size),
            ResizeHandle::NorthEast => {
                right = to.x.max(left + min_size);
                top = to.y.min(bottom - min_size);
            }
            ResizeHandle::East => right = to.x.max(left + min_size),
            ResizeHandle::SouthEast => {
                right = to.x.max(left + min_size);
                bottom = to.y.max(top + min_size);
            }
            ResizeHandle::South => bottom = to.y.max(top + min_size),
            ResizeHandle::SouthWest => {
                left = to.x.min(right - min_size);
                bottom = to.y.max(top + min_size);
            }
            ResizeHandle::West => left = to.x.min(right - min_size),
        }

        Self::new(left, top, right - left, bottom - top)
    }

    pub fn hit_resize_handle(self, point: Point, radius: f32) -> Option<ResizeHandle> {
        let handles = [
            (ResizeHandle::NorthWest, Point::new(self.x, self.y)),
            (ResizeHandle::North, Point::new(self.center().x, self.y)),
            (ResizeHandle::NorthEast, Point::new(self.right(), self.y)),
            (
                ResizeHandle::East,
                Point::new(self.right(), self.center().y),
            ),
            (
                ResizeHandle::SouthEast,
                Point::new(self.right(), self.bottom()),
            ),
            (
                ResizeHandle::South,
                Point::new(self.center().x, self.bottom()),
            ),
            (ResizeHandle::SouthWest, Point::new(self.x, self.bottom())),
            (ResizeHandle::West, Point::new(self.x, self.center().y)),
        ];

        handles
            .into_iter()
            .find(|(_, handle_point)| {
                (point.x - handle_point.x).abs() <= radius
                    && (point.y - handle_point.y).abs() <= radius
            })
            .map(|(handle, _)| handle)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResizeHandle {
    NorthWest,
    North,
    NorthEast,
    East,
    SouthEast,
    South,
    SouthWest,
    West,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rect_from_points_normalizes_negative_drag() {
        let rect = Rect::from_points(Point::new(100.0, 50.0), Point::new(20.0, 10.0));
        assert_eq!(rect, Rect::new(20.0, 10.0, 80.0, 40.0));
    }

    #[test]
    fn resize_west_keeps_minimum_width() {
        let rect = Rect::new(10.0, 10.0, 100.0, 50.0);
        let resized = rect.resize_from_handle(ResizeHandle::West, Point::new(500.0, 20.0), 24.0);
        assert_eq!(resized.width, 24.0);
        assert_eq!(resized.right(), rect.right());
    }
}
