use std::time::Instant;

use indexmap::IndexMap;
use procfs::{process::Process, ProcResult};
use termion::event::Key;
use tui::{
    backend::Backend,
    layout::Rect,
    style::{Color, Style},
    text::{Span, Spans, Text},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::ui::{InputResult, ScrollController, TWO_SECONDS};

use super::AppWidget;

struct TaskData {
    task: procfs::process::Task,
    _io: procfs::process::Io,
    stat: procfs::process::Stat,
}
impl TaskData {
    fn new(task: procfs::process::Task) -> Option<Self> {
        match (task.io(), task.stat()) {
            (Ok(io), Ok(stat)) => Some(TaskData { task, _io: io, stat }),
            _ => None,
        }
    }
}
pub struct TaskWidget {
    last_updated: Instant,
    tasks: ProcResult<IndexMap<i32, TaskData>>,
    last_tasks: Option<IndexMap<i32, TaskData>>,
    scroll: ScrollController,
}
impl TaskWidget {
    pub fn new(proc: &Process) -> TaskWidget {
        let tasks = proc
            .tasks()
            .map(|i| {
                i.filter_map(|t| t.ok()).filter_map(|t| {
                    let tid = t.tid;
                    TaskData::new(t).map(|td| (tid, td))
                })
            })
            .map(IndexMap::from_iter);

        TaskWidget {
            last_updated: Instant::now(),
            tasks,
            last_tasks: None,
            scroll: ScrollController::new(),
        }
    }
    pub fn draw_scrollbar<B: Backend>(&self, f: &mut Frame<B>, area: Rect) {
        self.scroll.draw_scrollbar(f, area)
    }
}
impl AppWidget for TaskWidget {
    const TITLE: &'static str = "Task";
    fn draw<B: Backend>(&mut self, f: &mut Frame<B>, area: Rect, help_text: &mut Text) {
        let spans = Spans::from(vec![
            Span::raw("The "),
            Span::styled("Task", Style::default().fg(Color::Yellow)),
            Span::raw(" tab shows each thread in the process, its name, and how much CPU it's using."),
        ]);
        help_text.extend(Text::from(spans));

        let mut text: Vec<Spans> = Vec::new();

        if let Ok(tasks) = &self.tasks {
            for task in tasks.values() {
                let name = &task.stat.comm;

                let cpu_str = if let Some(prev) = self.last_tasks.as_ref().and_then(|map| map.get(&task.task.tid)) {
                    let diff = task.stat.utime - prev.stat.utime;
                    format!("{:.1}%", diff as f64 / 2.0)
                } else {
                    "??%".to_string()
                };

                text.push(Spans::from(Span::raw(format!(
                    "({:<16}) {:<5} {}",
                    name, task.task.tid, cpu_str
                ))));
            }
        } else {
            text.push(Spans::from(Span::raw("Error reading tasks".to_string())));
        }

        let max_scroll = crate::get_numlines_from_spans(text.iter(), area.width as usize) as i32 - area.height as i32;
        self.scroll.set_max_scroll(max_scroll);

        let widget = Paragraph::new(text)
            .block(Block::default().borders(Borders::NONE))
            .scroll((self.scroll.scroll_offset, 0));
        f.render_widget(widget, area);
    }
    fn update(&mut self, proc: &Process) {
        if self.last_updated.elapsed() > TWO_SECONDS {
            let mut new_tasks = proc
                .tasks()
                .map(|i| {
                    i.filter_map(|t| t.ok()).filter_map(|t| {
                        let tid = t.tid;
                        TaskData::new(t).map(|td| (tid, td))
                    })
                })
                .map(IndexMap::from_iter);
            std::mem::swap(&mut new_tasks, &mut self.tasks);
            // "new_tasks" now contains the "old_tasks"
            self.last_tasks = new_tasks.ok();

            self.last_updated = Instant::now();
        }
    }
    fn handle_input(&mut self, input: Key, height: u16) -> InputResult {
        self.scroll.handle_input(input, height)
    }
}
