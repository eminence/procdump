use std::{borrow::Cow, time::Instant};

use crossterm::event::KeyEvent;
use procfs::{process::Process, ProcResult};
use tui::{
    backend::Backend,
    layout::{Constraint, Rect},
    style::{Color, Modifier, Style},
    text::{Span, Spans, Text},
    widgets::{Cell, Row, Table},
    Frame,
};

use crate::{
    ui::{InputResult, ScrollController, TWO_SECONDS},
    util::limit_to_string,
};

use super::AppWidget;

pub struct LimitWidget {
    limits: ProcResult<procfs::process::Limits>,
    last_updated: Instant,
    scroll: ScrollController,
}

impl LimitWidget {
    pub fn new(proc: &Process) -> LimitWidget {
        LimitWidget {
            limits: proc.limits(),
            last_updated: Instant::now(),
            scroll: ScrollController::new(),
        }
    }
}

impl AppWidget for LimitWidget {
    const TITLE: &'static str = "Limits";
    fn draw<B: Backend>(&mut self, f: &mut Frame<B>, area: Rect, help_text: &mut Text) {
        let spans = Spans::from(vec![
            Span::raw("The "),
            Span::styled("Limits", Style::default().fg(Color::Yellow)),
            Span::raw(" tab shows the process resource limits."),
        ]);
        help_text.extend(Text::from(spans));

        let header_cell_style = Style::default().add_modifier(Modifier::BOLD | Modifier::UNDERLINED);
        let headers = vec![
            Cell::from("Type").style(header_cell_style),
            Cell::from("Soft Limit").style(header_cell_style),
            Cell::from("Hard Limit").style(header_cell_style),
            Cell::from(""),
        ];
        let mut rows = Vec::new();

        rows.push(Row::new(headers).bottom_margin(1));

        if let Ok(ref limits) = self.limits {
            rows.push(Row::new(
                vec![
                    Cow::Borrowed("Cpu Time"),
                    limit_to_string(&limits.max_cpu_time.soft_limit),
                    limit_to_string(&limits.max_cpu_time.hard_limit),
                    Cow::Borrowed("(seconds)"),
                ]
                .into_iter()
                .map(tui::text::Text::raw),
            ));
            rows.push(Row::new(
                vec![
                    Cow::Borrowed("File Size"),
                    limit_to_string(&limits.max_file_size.soft_limit),
                    limit_to_string(&limits.max_file_size.hard_limit),
                    Cow::Borrowed("(bytes)"),
                ]
                .into_iter()
                .map(tui::text::Text::raw),
            ));
            rows.push(Row::new(
                vec![
                    Cow::Borrowed("Data Size"),
                    limit_to_string(&limits.max_data_size.soft_limit),
                    limit_to_string(&limits.max_data_size.hard_limit),
                    Cow::Borrowed("(bytes)"),
                ]
                .into_iter()
                .map(tui::text::Text::raw),
            ));
            rows.push(Row::new(
                vec![
                    Cow::Borrowed("Stack Size"),
                    limit_to_string(&limits.max_stack_size.soft_limit),
                    limit_to_string(&limits.max_stack_size.hard_limit),
                    Cow::Borrowed("(bytes)"),
                ]
                .into_iter()
                .map(tui::text::Text::raw),
            ));
            rows.push(Row::new(
                vec![
                    Cow::Borrowed("Core File Size"),
                    limit_to_string(&limits.max_core_file_size.soft_limit),
                    limit_to_string(&limits.max_core_file_size.hard_limit),
                    Cow::Borrowed("(bytes)"),
                ]
                .into_iter()
                .map(tui::text::Text::raw),
            ));
            rows.push(Row::new(
                vec![
                    Cow::Borrowed("Resident Set"),
                    limit_to_string(&limits.max_resident_set.soft_limit),
                    limit_to_string(&limits.max_resident_set.hard_limit),
                    Cow::Borrowed("(bytes)"),
                ]
                .into_iter()
                .map(tui::text::Text::raw),
            ));
            rows.push(Row::new(
                vec![
                    Cow::Borrowed("Processes"),
                    limit_to_string(&limits.max_processes.soft_limit),
                    limit_to_string(&limits.max_processes.hard_limit),
                    Cow::Borrowed(""),
                ]
                .into_iter()
                .map(tui::text::Text::raw),
            ));
            rows.push(Row::new(
                vec![
                    Cow::Borrowed("Open Files"),
                    limit_to_string(&limits.max_open_files.soft_limit),
                    limit_to_string(&limits.max_open_files.hard_limit),
                    Cow::Borrowed(""),
                ]
                .into_iter()
                .map(tui::text::Text::raw),
            ));
            rows.push(Row::new(
                vec![
                    Cow::Borrowed("Locked Memory"),
                    limit_to_string(&limits.max_locked_memory.soft_limit),
                    limit_to_string(&limits.max_locked_memory.hard_limit),
                    Cow::Borrowed("(bytes)"),
                ]
                .into_iter()
                .map(tui::text::Text::raw),
            ));
            rows.push(Row::new(
                vec![
                    Cow::Borrowed("Address Space"),
                    limit_to_string(&limits.max_address_space.soft_limit),
                    limit_to_string(&limits.max_address_space.hard_limit),
                    Cow::Borrowed(""),
                ]
                .into_iter()
                .map(tui::text::Text::raw),
            ));
            rows.push(Row::new(
                vec![
                    Cow::Borrowed("File Locks"),
                    limit_to_string(&limits.max_file_locks.soft_limit),
                    limit_to_string(&limits.max_file_locks.hard_limit),
                    Cow::Borrowed(""),
                ]
                .into_iter()
                .map(tui::text::Text::raw),
            ));
            rows.push(Row::new(
                vec![
                    Cow::Borrowed("Pending Signals"),
                    limit_to_string(&limits.max_pending_signals.soft_limit),
                    limit_to_string(&limits.max_pending_signals.hard_limit),
                    Cow::Borrowed(""),
                ]
                .into_iter()
                .map(tui::text::Text::raw),
            ));
            rows.push(Row::new(
                vec![
                    Cow::Borrowed("Msgqueue Size"),
                    limit_to_string(&limits.max_msgqueue_size.soft_limit),
                    limit_to_string(&limits.max_msgqueue_size.hard_limit),
                    Cow::Borrowed("(bytes)"),
                ]
                .into_iter()
                .map(tui::text::Text::raw),
            ));
            rows.push(Row::new(
                vec![
                    Cow::Borrowed("Nice Priority"),
                    limit_to_string(&limits.max_nice_priority.soft_limit),
                    limit_to_string(&limits.max_nice_priority.hard_limit),
                    Cow::Borrowed(""),
                ]
                .into_iter()
                .map(tui::text::Text::raw),
            ));
            rows.push(Row::new(
                vec![
                    Cow::Borrowed("Realtime Priority"),
                    limit_to_string(&limits.max_realtime_priority.soft_limit),
                    limit_to_string(&limits.max_realtime_priority.hard_limit),
                    Cow::Borrowed(""),
                ]
                .into_iter()
                .map(tui::text::Text::raw),
            ));
            rows.push(Row::new(
                vec![
                    Cow::Borrowed("Realtime Timeout"),
                    limit_to_string(&limits.max_realtime_timeout.soft_limit),
                    limit_to_string(&limits.max_realtime_timeout.hard_limit),
                    Cow::Borrowed("(μseconds)"),
                ]
                .into_iter()
                .map(tui::text::Text::raw),
            ));
        }

        self.scroll.set_max_scroll(rows.len() as i32 + 2);

        let needed_height = rows.len() as u16 + 2; // one for header and one for spacer
        let rows = if needed_height > area.height {
            // use tab_scroll_offset to remove some of the top entries
            let max_offset = needed_height - area.height;
            if self.scroll.scroll_offset > max_offset {
                self.scroll.scroll_offset = max_offset;
            }
            rows.split_off(self.scroll.scroll_offset as usize)
        } else {
            rows
        };

        let widget = Table::new(rows.into_iter()).widths(&[
            Constraint::Length(18),
            Constraint::Length(12),
            Constraint::Length(12),
            Constraint::Length(11),
        ]);
        f.render_widget(widget, area);
    }
    fn update(&mut self, proc: &Process) {
        if self.last_updated.elapsed() > TWO_SECONDS {
            self.limits = proc.limits();
            self.last_updated = Instant::now();
        }
    }
    fn handle_input(&mut self, input: KeyEvent, height: u16) -> InputResult {
        self.scroll.handle_input(input, height)
    }
}
