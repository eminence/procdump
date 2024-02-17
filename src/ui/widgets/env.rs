use std::{collections::HashMap, ffi::OsString, time::Instant};

use crossterm::event::KeyEvent;
use procfs::{process::Process, ProcError};
use ratatui::{
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};

use crate::ui::{InputResult, ScrollController, TWO_SECONDS};

use super::AppWidget;

pub struct EnvWidget {
    env: Result<HashMap<OsString, OsString>, ProcError>,
    last_updated: Instant,
    scroll: ScrollController,
}

impl EnvWidget {
    pub fn new(proc: &Process) -> EnvWidget {
        let env = proc.environ();
        EnvWidget {
            env,
            last_updated: Instant::now(),
            scroll: ScrollController::new(),
        }
    }
    pub fn draw_scrollbar(&self, f: &mut Frame, area: Rect) {
        self.scroll.draw_scrollbar(f, area)
    }
}

impl AppWidget for EnvWidget {
    const TITLE: &'static str = "Env";
    fn handle_input(&mut self, input: KeyEvent, height: u16) -> InputResult {
        self.scroll.handle_input(input, height)
    }

    fn update(&mut self, proc: &Process) {
        if self.last_updated.elapsed() > TWO_SECONDS {
            self.env = proc.environ();
            self.last_updated = Instant::now();
        }
    }
    fn draw(&mut self, f: &mut Frame, area: Rect, help_text: &mut Text) {
        let mut text: Vec<Line> = Vec::new();

        let spans = Line::from(vec![
            Span::raw("The "),
            Span::styled("Env", Style::default().fg(Color::Yellow)),
            Span::raw(" tab shows the environment variables for the process"),
        ]);
        help_text.extend(Text::from(spans));

        match &self.env {
            Err(e) => {
                text.push(From::from(Span::styled(
                    format!("Error getting environment: {e}"),
                    Style::default().fg(Color::Red).bg(Color::Reset),
                )));
            }
            Ok(map) => {
                let mut keys: Vec<_> = map.keys().collect();
                keys.sort_unstable();
                for key in keys {
                    text.push(Line::from(vec![
                        Span::styled(key.to_string_lossy().into_owned(), Style::default().fg(Color::Green)),
                        Span::styled("=", Style::default().fg(Color::Green)),
                        Span::raw(map[key].to_string_lossy().into_owned()),
                    ]));
                }
            }
        }
        let max_scroll = crate::get_numlines_from_spans(text.iter(), area.width as usize) as i32 - area.height as i32;
        self.scroll.set_max_scroll(max_scroll);

        let widget = Paragraph::new(text)
            .block(Block::default().borders(Borders::NONE))
            .wrap(Wrap { trim: true })
            .scroll((self.scroll.scroll_offset, 0));
        f.render_widget(widget, area);
    }
}
