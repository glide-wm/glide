// TODO: Consider moving animation state out of the model layer.

use std::time::Instant;

use objc2_core_foundation::CGRect;

use super::spring::SpringAnimation;
use crate::config::CenterMode;

#[derive(Debug, Clone)]
pub enum ScrollState {
    Static(f64),
    Animating(SpringAnimation),
}

impl ScrollState {
    pub fn current(&self, now: Instant) -> f64 {
        match self {
            ScrollState::Static(v) => *v,
            ScrollState::Animating(spring) => spring.current(now),
        }
    }

    pub fn target(&self) -> f64 {
        match self {
            ScrollState::Static(v) => *v,
            ScrollState::Animating(spring) => spring.target(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ViewportState {
    pub scroll: ScrollState,
    pub active_column_index: usize,
    pub screen_width: f64,
    pub user_scrolling: bool,
    pub scroll_progress: f64,
}

impl ViewportState {
    pub fn new(screen_width: f64) -> Self {
        ViewportState {
            scroll: ScrollState::Static(0.0),
            active_column_index: 0,
            screen_width,
            user_scrolling: false,
            scroll_progress: 0.0,
        }
    }

    pub fn scroll_offset(&self, now: Instant) -> f64 {
        self.scroll.current(now)
    }

    pub fn target_offset(&self) -> f64 {
        self.scroll.target()
    }

    pub fn set_screen_width(&mut self, width: f64) {
        self.screen_width = width;
    }

    pub fn ensure_column_visible(
        &mut self,
        column_index: usize,
        column_x: f64,
        column_width: f64,
        center_mode: CenterMode,
        gap: f64,
        now: Instant,
    ) {
        self.active_column_index = column_index;
        self.user_scrolling = false;
        let current = self.target_offset();

        let new_offset = match center_mode {
            CenterMode::Always => column_x + column_width / 2.0 - self.screen_width / 2.0,
            CenterMode::OnOverflow => {
                if column_width > self.screen_width {
                    column_x + column_width / 2.0 - self.screen_width / 2.0
                } else {
                    self.compute_edge_fit(column_x, column_width, current, gap)
                }
            }
            CenterMode::Never => self.compute_edge_fit(column_x, column_width, current, gap),
        };

        if (new_offset - current).abs() > 0.5 {
            self.animate_to(new_offset, now);
        }
    }

    fn compute_edge_fit(&self, col_x: f64, col_w: f64, current: f64, gap: f64) -> f64 {
        let view_left = current;
        let view_right = current + self.screen_width;

        if col_x >= view_left && col_x + col_w <= view_right {
            return current;
        }

        let padding = ((self.screen_width - col_w) / 2.0).clamp(0.0, gap);

        if col_x < view_left {
            col_x - padding
        } else {
            col_x + col_w + padding - self.screen_width
        }
    }

    pub fn snap_to_offset(&mut self, offset: f64) {
        self.scroll = ScrollState::Static(offset);
    }

    pub fn animate_to(&mut self, target: f64, now: Instant) {
        match &mut self.scroll {
            ScrollState::Animating(spring) => {
                spring.retarget(target, now);
            }
            ScrollState::Static(current) => {
                self.scroll =
                    ScrollState::Animating(SpringAnimation::with_defaults(*current, target, now));
            }
        }
    }

    pub fn accumulate_scroll(&mut self, delta: f64, avg_column_width: f64) -> Option<i32> {
        if avg_column_width <= 0.0 {
            return None;
        }
        self.scroll_progress += delta;
        let steps = (self.scroll_progress / avg_column_width).trunc() as i32;
        if steps != 0 {
            self.scroll_progress -= steps as f64 * avg_column_width;
            Some(steps)
        } else {
            None
        }
    }

    pub fn is_animating(&self, now: Instant) -> bool {
        match &self.scroll {
            ScrollState::Static(_) => false,
            ScrollState::Animating(spring) => !spring.is_complete(now),
        }
    }

    pub fn tick(&mut self, now: Instant) {
        if let ScrollState::Animating(spring) = &self.scroll {
            if spring.is_complete(now) {
                self.scroll = ScrollState::Static(spring.target());
                self.user_scrolling = false;
            }
        }
    }

    pub fn offset_rect(&self, rect: CGRect, now: Instant) -> CGRect {
        let offset = self.scroll_offset(now);
        CGRect::new(
            objc2_core_foundation::CGPoint::new(rect.origin.x - offset, rect.origin.y),
            rect.size,
        )
    }

    pub fn is_visible(&self, rect: CGRect, now: Instant) -> bool {
        let offset = self.scroll_offset(now);
        let view_left = offset;
        let view_right = offset + self.screen_width;
        let rect_left = rect.origin.x;
        let rect_right = rect.origin.x + rect.size.width;
        rect_right > view_left && rect_left < view_right
    }

    pub fn apply_viewport_to_frames<T>(
        &self,
        screen: CGRect,
        frames: Vec<(T, CGRect)>,
        now: Instant,
    ) -> Vec<(T, CGRect)> {
        let offset = self.scroll_offset(now);
        let view_left = offset;
        let view_right = offset + self.screen_width;

        frames
            .into_iter()
            .map(|(wid, rect)| {
                let rect_right = rect.origin.x + rect.size.width;
                let rect_left = rect.origin.x;

                if rect_right > view_left && rect_left < view_right {
                    (wid, self.offset_rect(rect, now))
                } else if rect_right <= view_left {
                    let hidden = CGRect::new(
                        objc2_core_foundation::CGPoint::new(
                            screen.origin.x - rect.size.width,
                            rect.origin.y,
                        ),
                        rect.size,
                    );
                    (wid, hidden)
                } else {
                    let hidden = CGRect::new(
                        objc2_core_foundation::CGPoint::new(
                            screen.origin.x + screen.size.width,
                            rect.origin.y,
                        ),
                        rect.size,
                    );
                    (wid, hidden)
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use objc2_core_foundation::{CGPoint, CGSize};

    use super::*;

    fn make_rect(x: f64, y: f64, w: f64, h: f64) -> CGRect {
        CGRect::new(CGPoint::new(x, y), CGSize::new(w, h))
    }

    #[test]
    fn ensure_column_visible_already_visible() {
        let now = Instant::now();
        let mut vp = ViewportState::new(1920.0);
        vp.snap_to_offset(0.0);
        vp.ensure_column_visible(0, 100.0, 500.0, CenterMode::Never, 0.0, now);
        assert_eq!(vp.target_offset(), 0.0);
    }

    #[test]
    fn ensure_column_visible_scrolls_left() {
        let now = Instant::now();
        let mut vp = ViewportState::new(1920.0);
        vp.snap_to_offset(500.0);
        vp.ensure_column_visible(0, 100.0, 500.0, CenterMode::Never, 0.0, now);
        assert_eq!(vp.target_offset(), 100.0);
    }

    #[test]
    fn apply_viewport_returns_all_windows_with_correct_positions() {
        let mut vp = ViewportState::new(1920.0);
        vp.snap_to_offset(960.0);
        let screen = make_rect(0.0, 0.0, 1920.0, 1080.0);

        let frames: Vec<(usize, CGRect)> = (0..5)
            .map(|i| {
                let wid = i;
                let rect = make_rect(i as f64 * 640.0, 0.0, 640.0, 1080.0);
                (wid, rect)
            })
            .collect();

        let result = vp.apply_viewport_to_frames(screen, frames, Instant::now());
        assert_eq!(result.len(), 5);

        for (_, r) in &result {
            assert_eq!(r.size.width, 640.0, "all windows should preserve original width");
            assert_eq!(
                r.size.height, 1080.0,
                "all windows should preserve original height"
            );
        }

        let on_screen: Vec<_> = result
            .iter()
            .filter(|(_, r)| r.origin.x + r.size.width > 0.0 && r.origin.x < 1920.0)
            .collect();
        let off_screen: Vec<_> = result
            .iter()
            .filter(|(_, r)| r.origin.x + r.size.width <= 0.0 || r.origin.x >= 1920.0)
            .collect();
        assert!(!on_screen.is_empty());
        assert!(!off_screen.is_empty());
    }

    #[test]
    fn is_visible_checks_correctly() {
        let vp = ViewportState::new(1920.0);
        assert!(vp.is_visible(make_rect(0.0, 0.0, 500.0, 1080.0), Instant::now()));
        assert!(!vp.is_visible(make_rect(2000.0, 0.0, 500.0, 1080.0), Instant::now()));
    }

    #[test]
    fn static_viewport_is_not_animating() {
        let vp = ViewportState::new(1000.0);
        assert!(!vp.is_animating(Instant::now()));
    }

    #[test]
    fn completed_animation_settles_to_static() {
        let now = Instant::now();
        let mut vp = ViewportState::new(1000.0);
        vp.animate_to(10.0, now);
        let later = now + std::time::Duration::from_secs(1);
        vp.tick(later);
        assert!(!vp.is_animating(later));
    }
}
