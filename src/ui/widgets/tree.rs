use std::time::Instant;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use procfs::process::Process;
use tui::{
    backend::Backend,
    layout::Rect,
    style::{Color, Style},
    text::{Span, Spans, Text},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::{
    ui::{InputResult, TWO_SECONDS},
    util,
};

use super::AppWidget;

pub struct TreeWidget {
    tree: util::ProcessTree,
    last_updated: Instant,
    force_update: bool,
    /// The currently selected PID
    selected_pid: i32,
    show_all: bool,
    this_pid: i32,
}

impl TreeWidget {
    pub fn new(proc: &Process) -> TreeWidget {
        let tree = util::ProcessTree::new(None).unwrap();
        TreeWidget {
            tree,
            show_all: true,
            force_update: false,
            last_updated: Instant::now(),
            selected_pid: proc.pid,
            this_pid: proc.pid,
        }
    }
    pub fn get_selected_pid(&self) -> i32 {
        self.selected_pid
    }
}

impl AppWidget for TreeWidget {
    const TITLE: &'static str = "Tree";
    fn draw<B: Backend>(&mut self, f: &mut Frame<B>, area: Rect, help_text: &mut Text) {
        let spans = Spans::from(vec![
            Span::raw("The "),
            Span::styled("Tree", Style::default().fg(Color::Yellow)),
            Span::raw(" tab shows the currently selected process in a process tree. Press "),
            Span::styled("ctrl-t", Style::default().fg(Color::Green)),
            Span::raw(" to show only the parent processes and direct children."),
        ]);
        help_text.extend(Text::from(spans));

        let selected_style = Style::default().fg(Color::Magenta);
        let self_style = Style::default().fg(Color::Yellow);
        let unselected_style = Style::default();

        let mut text: Vec<Spans> = Vec::new();

        let flattened = self.tree.flatten();

        let mut iter = flattened.iter().enumerate().peekable();
        let mut last_depth = 0;
        let mut prints = Vec::new();
        while let Some((idx, (depth, item))) = iter.next() {
            let mut line: Vec<Span> = Vec::with_capacity(2);
            let depth = *depth as usize;
            if depth > last_depth {
                prints.push(item.num_siblings);
            }
            if depth < last_depth {
                prints.truncate(depth);
            }
            assert_eq!(depth, prints.len());
            last_depth = depth;
            if depth > 0 && prints[depth - 1] > 0 {
                prints[depth - 1] -= 1;
            }

            let lines = if idx == 0 {
                "━┳╸".to_owned()
            } else {
                prints
                    .iter()
                    .enumerate()
                    .map(|(p_idx, n)| {
                        if *n > 0 {
                            if p_idx == depth - 1 {
                                "┣"
                            } else {
                                "┆"
                            }
                        } else if p_idx == depth - 1 {
                            "┗"
                        } else {
                            " "
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("")
            };

            line.push(Span::raw(lines));

            if idx > 0 {
                let has_children = iter
                    .peek()
                    .map(|(_, (p_depth, _))| *p_depth as usize > depth)
                    .unwrap_or(false);
                line.push(Span::raw(format!("{b}╸", b = if has_children { "┳" } else { "━" },)));
            }

            line.push(Span::styled(
                format!("{} {}", item.pid, item.cmdline),
                if item.pid == self.selected_pid {
                    selected_style
                } else if item.pid == self.this_pid {
                    self_style
                } else {
                    unselected_style
                },
            ));
            text.push(Spans::from(line));
        }
        let select_idx = flattened
            .iter()
            .enumerate()
            .find(|(_idx, (_, item))| item.pid == self.selected_pid)
            .unwrap()
            .0 as i32;

        // in general, we want to have our selected line in the middle of the screen:
        let target_offset = area.height as i32 / 2; // 12
        let diff = select_idx - target_offset;
        let max_scroll = std::cmp::max(0, text.len() as i32 - area.height as i32);
        let scroll = diff.clamp(0, max_scroll);

        //let max_scroll = get_numlines(text.iter(), area.width as usize) as i32 - area.height as i32;
        //self.set_max_scroll(max_scroll);
        let widget = Paragraph::new(text)
            .block(Block::default().borders(Borders::NONE))
            .scroll((scroll as u16, 0));
        f.render_widget(widget, area);
    }
    fn update(&mut self, proc: &Process) {
        if self.last_updated.elapsed() > TWO_SECONDS || self.force_update {
            // before we update, get a llist of our parents PIDs, all the way up to pid1.
            // After the refresh, our selected process might be gone, so we'll want to instead
            // select its next available parent
            let mut pid = self.selected_pid;
            let mut parents = Vec::new();
            parents.push(self.selected_pid);
            while pid > 1 {
                if let Some(entry) = self.tree.entries.get(&pid) {
                    parents.push(entry.ppid);
                    pid = entry.ppid;
                } else {
                    break;
                }
            }
            parents.push(1);
            self.tree = util::ProcessTree::new(if self.show_all { None } else { Some((&parents, proc)) }).unwrap();
            self.last_updated = Instant::now();
            self.force_update = false;

            if !self.tree.entries.contains_key(&self.selected_pid) {
                for p in parents {
                    if self.tree.entries.contains_key(&p) {
                        self.selected_pid = p;
                        break;
                    }
                }
            }
        }
    }
    fn handle_input(&mut self, input: KeyEvent, _height: u16) -> InputResult {
        let flattened = self.tree.flatten();
        // the current index of the selected pid
        let mut select_idx = flattened
            .iter()
            .enumerate()
            .find(|(_idx, (_, item))| item.pid == self.selected_pid)
            .unwrap()
            .0 as i32;

        let r = match input {
            KeyEvent {
                code: KeyCode::Char('t'),
                modifiers,
                ..
            } if modifiers.contains(KeyModifiers::CONTROL) => {
                self.show_all = !self.show_all;
                self.force_update = true;
                return InputResult::NeedsUpdate;
            }
            KeyEvent { code: KeyCode::Up, .. } => {
                if select_idx > 0 {
                    select_idx -= 1;
                    true
                } else {
                    false
                }
            }
            KeyEvent {
                code: KeyCode::Down, ..
            } => {
                if select_idx < flattened.len() as i32 {
                    select_idx += 1;
                    true
                } else {
                    false
                }
            }
            _ => false,
        };

        // calculate new pid
        if r {
            if let Some((_, item)) = flattened.get(select_idx as usize) {
                self.selected_pid = item.pid;
            }
        }
        if r {
            InputResult::NeedsRedraw
        } else {
            InputResult::None
        }
    }
}
