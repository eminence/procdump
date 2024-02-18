use std::time::Duration;

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::terminal::Frame;
use ratatui::widgets::*;

pub mod widgets;

const ONE_SECONDS: Duration = Duration::from_secs(1);
const TWO_SECONDS: Duration = Duration::from_secs(2);
const TEN_SECONDS: Duration = Duration::from_secs(10);

#[derive(Debug, Eq, PartialEq, Copy, Clone)]
pub enum InputResult {
    /// The widget needs to be redrawn
    NeedsRedraw,
    /// The widget needs to be updated with the latest process info (implies NeedsRedraw)
    NeedsUpdate,
    None,
}

impl std::ops::BitOr for InputResult {
    type Output = InputResult;
    fn bitor(self, rhs: Self) -> Self {
        if self == InputResult::NeedsUpdate || rhs == InputResult::NeedsUpdate {
            InputResult::NeedsUpdate
        } else if self == InputResult::NeedsRedraw || rhs == InputResult::NeedsRedraw {
            InputResult::NeedsRedraw
        } else {
            InputResult::None
        }
    }
}

pub struct ScrollController {
    scroll_offset: u16,
    max_scroll: u16,
}

impl ScrollController {
    fn new() -> ScrollController {
        ScrollController {
            scroll_offset: 0,
            max_scroll: 0,
        }
    }
    fn draw_scrollbar(&self, f: &mut Frame, area: Rect) {
        let bar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(Some("↑"))
            .end_symbol(Some("↓"));
        let mut state = ScrollbarState::new(self.max_scroll as usize).position(self.scroll_offset as usize);

        f.render_stateful_widget(bar, area, &mut state);
    }
    /// Sets the maximum scroll offset (the total number of lines of the content with the scrollbar)
    fn set_max_scroll(&mut self, max: i32) {
        let max: u16 = std::cmp::max(0, max) as u16;
        if self.scroll_offset >= max {
            self.scroll_offset = max
        }
        self.max_scroll = max;
    }
    fn handle_input(&mut self, input: KeyEvent, height: u16) -> InputResult {
        let pageupdown_size = height / 3;
        match input.code {
            KeyCode::Down | KeyCode::PageDown | KeyCode::End => {
                let to_move = (self.max_scroll as i32 - self.scroll_offset as i32).clamp(
                    0,
                    if input.code == KeyCode::PageDown {
                        pageupdown_size
                    } else if input.code == KeyCode::End {
                        self.max_scroll
                    } else {
                        1
                    } as i32,
                );
                if to_move > 0 {
                    self.scroll_offset += to_move as u16;
                    InputResult::NeedsRedraw
                } else {
                    InputResult::None
                }
            }
            KeyCode::Home => {
                let p = self.scroll_offset;
                self.scroll_offset = 0;
                if p > 0 {
                    InputResult::NeedsRedraw
                } else {
                    InputResult::None
                }
            }
            KeyCode::Up | KeyCode::PageUp => {
                let mut to_move = if input.code == KeyCode::PageUp {
                    pageupdown_size
                } else {
                    1
                } as i32;
                if self.scroll_offset as i32 - to_move < 0 {
                    to_move = self.scroll_offset as i32;
                }
                if to_move > 0 {
                    self.scroll_offset -= to_move as u16;
                    InputResult::NeedsRedraw
                } else {
                    InputResult::None
                }
            }
            _ => InputResult::None,
        }
    }
}
