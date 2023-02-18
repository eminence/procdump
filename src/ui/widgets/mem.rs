use std::time::Instant;

use crossterm::event::KeyEvent;
use procfs::{
    process::{Process, SmapsRollup},
    ProcResult,
};
use tui::{
    backend::Backend,
    layout::Rect,
    style::{Color, Style},
    text::{Span, Spans, Text},
    widgets::{Block, Borders, Paragraph},
};

use crate::{
    ui::{InputResult, TWO_SECONDS},
    util::fmt_bytes,
};

use super::AppWidget;

pub struct MemWidget {
    rollup: ProcResult<SmapsRollup>,
    last_updated: Instant,
}

impl MemWidget {
    pub fn new(proc: &Process) -> Self {
        Self {
            rollup: proc.smaps_rollup(),
            last_updated: Instant::now(),
        }
    }
}

impl AppWidget for MemWidget {
    const TITLE: &'static str = "Mem";

    fn draw<B: Backend>(&mut self, f: &mut tui::Frame<B>, area: Rect, _help_text: &mut Text) {
        let mut text: Vec<Spans> = Vec::new();

        match &self.rollup {
            Ok(rollup) => {
                let key_style = Style::default().fg(Color::Green);
                let data = &rollup.memory_map_data.map;
                if let Some(x) = data.get("Rss") {
                    text.push(Spans::from(vec![
                        Span::styled(format!("{:15}", "Rss:"), key_style),
                        Span::raw(fmt_bytes(*x, "B")),
                    ]));
                }
                if let Some(x) = data.get("Pss") {
                    text.push(Spans::from(vec![
                        Span::styled(format!("{:15}", "Pss:"), key_style),
                        Span::raw(fmt_bytes(*x, "B")),
                    ]));
                }
                if let Some(x) = data.get("Shared_Clean") {
                    text.push(Spans::from(vec![
                        Span::styled(format!("{:15}", "Shared_Clean:"), key_style),
                        Span::raw(fmt_bytes(*x, "B")),
                    ]));
                }
                if let Some(x) = data.get("Shared_Dirty") {
                    text.push(Spans::from(vec![
                        Span::styled(format!("{:15}", "Shared_Dirty:"), key_style),
                        Span::raw(fmt_bytes(*x, "B")),
                    ]));
                }
                if let Some(x) = data.get("Private_Clean") {
                    text.push(Spans::from(vec![
                        Span::styled(format!("{:15}", "Private_Clean:"), key_style),
                        Span::raw(fmt_bytes(*x, "B")),
                    ]));
                }
                if let Some(x) = data.get("Private_Dirty") {
                    text.push(Spans::from(vec![
                        Span::styled(format!("{:15}", "Private_Dirty:"), key_style),
                        Span::raw(fmt_bytes(*x, "B")),
                    ]));
                }
                if let Some(x) = data.get("Referenced") {
                    text.push(Spans::from(vec![
                        Span::styled(format!("{:15}", "Referenced:"), key_style),
                        Span::raw(fmt_bytes(*x, "B")),
                    ]));
                }
                if let Some(x) = data.get("Anonymous") {
                    text.push(Spans::from(vec![
                        Span::styled(format!("{:15}", "Anonymous:"), key_style),
                        Span::raw(fmt_bytes(*x, "B")),
                    ]));
                }
                if let Some(x) = data.get("Swap") {
                    text.push(Spans::from(vec![
                        Span::styled(format!("{:15}", "Swap:"), key_style),
                        Span::raw(fmt_bytes(*x, "B")),
                    ]));
                }
            }
            Err(e) => {
                text.push(Spans::from(Span::styled(
                    format!("Error getting memory rollup: {e}"),
                    Style::default().fg(Color::Red).bg(Color::Reset),
                )));
            }
        }

        let widget = Paragraph::new(text).block(Block::default().borders(Borders::NONE));
        f.render_widget(widget, area);
    }

    fn update(&mut self, proc: &Process) {
        if self.last_updated.elapsed() > TWO_SECONDS {
            self.rollup = proc.smaps_rollup();
            self.last_updated = Instant::now();
        }
    }

    fn handle_input(&mut self, _input: KeyEvent, _heightt: u16) -> InputResult {
        InputResult::None
    }
}
