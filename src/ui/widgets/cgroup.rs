use std::{
    collections::{BTreeSet, HashMap, HashSet},
    fs::read_to_string,
    path::PathBuf,
    time::Instant,
};

use crossterm::event::{KeyCode, KeyEvent};
use procfs::{process::Process, ProcResult, ProcessCGroup};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};

use crate::ui::{InputResult, TEN_SECONDS};

use super::AppWidget;

pub struct CGroupWidget {
    proc_groups: ProcResult<Vec<ProcessCGroup>>,
    last_updated: Instant,

    // map from controller name to mount path
    v1_controllers: HashMap<BTreeSet<String>, PathBuf>,
    select_idx: u16,
}

impl CGroupWidget {
    pub fn new(proc: &Process) -> CGroupWidget {
        let mut map = HashMap::new();

        // get the list of v1 controllers on this system
        let groups: HashSet<String> = procfs::cgroups()
            .ok()
            .map(|cgcs| cgcs.0)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|cg| if cg.enabled { Some(cg.name) } else { None })
            .collect();

        if let Ok(mountinfo) = proc.mountinfo() {
            for mut mi in mountinfo {
                if mi.fs_type == "cgroup" {
                    let super_options: HashSet<String> = HashSet::from_iter(mi.super_options.drain().map(|(k, _)| k));
                    let controllers: BTreeSet<String> = super_options.intersection(&groups).cloned().collect();
                    map.insert(controllers, mi.mount_point);
                }
            }
        }

        let groups = proc.cgroups().map(|mut l| {
            l.0.sort_by_key(|g| g.hierarchy);
            l.0
        });

        CGroupWidget {
            last_updated: Instant::now(),
            proc_groups: groups,
            v1_controllers: map,
            select_idx: 0,
        }
    }
}

impl AppWidget for CGroupWidget {
    const TITLE: &'static str = "CGroups";
    fn draw(&mut self, f: &mut Frame, area: Rect, help_text: &mut Text) {
        let line = Line::from(vec![
            Span::raw("The "),
            Span::styled("CGroups", Style::default().fg(Color::Yellow)),
            Span::raw(" tab shows info about the active container groups for this process."),
        ]);
        help_text.extend(Text::from(line));

        // split the area in half -- the left side is a selectable list of controllers, and the
        // right side is some details about them

        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .margin(0)
            .constraints([Constraint::Ratio(1, 2), Constraint::Ratio(1, 2)].as_ref())
            .split(area);

        let green = Style::default().fg(Color::Green);
        let selected = Style::default().fg(Color::Yellow);

        let mut text: Vec<Line> = Vec::new();
        let mut details: Vec<Line> = Vec::new();

        if let Ok(cgroups) = &self.proc_groups {
            for (idx, cg) in cgroups.iter().enumerate() {
                let mut line: Vec<Span> = Vec::new();
                let current = idx == self.select_idx as usize;
                let groups = BTreeSet::from_iter(cg.controllers.clone());
                let controller_name = if cg.controllers.is_empty() {
                    "???".to_owned()
                } else {
                    cg.controllers.join(",")
                };
                if let Some(mountpoint) = self.v1_controllers.get(&groups) {
                    line.push(Span::styled(
                        format!("{controller_name}: "),
                        if current { green } else { selected },
                    ));
                    line.push(Span::raw(format!("{}\n", cg.pathname)));

                    let root = if cg.pathname.starts_with('/') {
                        mountpoint.join(&cg.pathname[1..])
                    } else {
                        mountpoint.join(&cg.pathname)
                    };

                    if current {
                        details.push(Line::from(Span::raw(format!("{groups:?}"))));
                        if groups.contains("pids") {
                            let current = read_to_string(root.join("pids.current"));
                            let max = read_to_string(root.join("pids.max"));
                            if let (Ok(current), Ok(max)) = (current, max) {
                                details.push(Line::from(Span::raw(format!("{} of {}", current.trim(), max.trim()))));
                            }
                        }
                        if groups.contains("freezer") {
                            let state = read_to_string(root.join("freezer.state"));
                            if let Ok(state) = state {
                                details.push(Line::from(Span::raw(format!("state: {}", state.trim()))));
                            }
                        }
                        if groups.contains("memory") {
                            if let Ok(usage) = read_to_string(root.join("memory.usage_in_bytes")) {
                                details.push(Line::from(Span::raw(format!("Group Usage: {} bytes", usage.trim()))));
                            }
                            if let Ok(limit) = read_to_string(root.join("memory.limit_in_bytes")) {
                                details.push(Line::from(Span::raw(format!("Group Limit: {} bytes", limit.trim()))));
                            }
                            if let Ok(usage) = read_to_string(root.join("memory.kmem.usage_in_bytes")) {
                                details.push(Line::from(Span::raw(format!("Kernel Usage: {} bytes", usage.trim()))));
                            }
                            if let Ok(limit) = read_to_string(root.join("memory.kmem.limit_in_bytes")) {
                                details.push(Line::from(Span::raw(format!("Kernel Limit: {} bytes", limit.trim()))));
                            }
                            if let Ok(limit) = read_to_string(root.join("memory.stat")) {
                                details.push(Line::from(vec![Span::raw("stats:\n"), Span::raw(limit)]));
                            }
                        }
                        if groups.contains("net_cls") {
                            if let Ok(classid) = read_to_string(root.join("net_cls.classid")) {
                                details.push(Line::from(Span::raw(format!("Class ID: {}", classid.trim()))));
                            }
                        }
                        if groups.contains("net_prio") {
                            if let Ok(idx) = read_to_string(root.join("net_prio.prioidx")) {
                                details.push(Line::from(Span::raw(format!("Prioidx: {idx}"))));
                            }
                            if let Ok(map) = read_to_string(root.join("net_prio.ifpriomap")) {
                                details.push(Line::from(vec![Span::raw("ifpriomap:"), Span::raw(map)]));
                            }
                        }
                        if groups.contains("blkio") {}
                        if groups.contains("cpuacct") {
                            if let Ok(acct) = read_to_string(root.join("cpuacct.usage")) {
                                details.push(Line::from(Span::raw(format!("Total nanoseconds: {}", acct.trim()))));
                            }
                            if let Ok(usage_all) = read_to_string(root.join("cpuacct.usage_all")) {
                                details.push(Line::from(Span::raw(usage_all)));
                            }
                        }
                        {
                            details.push(Line::from(Span::raw(format!("--> {mountpoint:?}"))));
                            details.push(Line::from(Span::raw(format!("--> {:?}", cg.pathname))));
                        }
                    }
                } else {
                    line.push(Span::styled(
                        format!("{controller_name}: "),
                        if current {
                            green.add_modifier(Modifier::DIM)
                        } else {
                            selected.add_modifier(Modifier::DIM)
                        },
                    ));
                    line.push(Span::raw(cg.pathname.to_string()));
                    if idx == self.select_idx as usize {
                        details.push(Line::from(Span::raw("This controller isn't supported by procdump")));
                    }
                }
                text.push(Line::from(line));
            }
        }

        let target_offset = chunks[0].height as i32 / 2; // 12
        let diff = self.select_idx as i32 - target_offset;
        let max_scroll = std::cmp::max(0, text.len() as i32 - chunks[0].height as i32);
        let scroll = diff.clamp(0, max_scroll);

        let widget = Paragraph::new(text)
            .block(Block::default().borders(Borders::NONE))
            .scroll((0, scroll as u16));
        f.render_widget(widget, chunks[0]);

        let widget = Paragraph::new(details)
            .block(Block::default().borders(Borders::LEFT))
            .wrap(Wrap { trim: false });
        f.render_widget(widget, chunks[1]);
    }
    fn update(&mut self, proc: &Process) {
        if self.last_updated.elapsed() > TEN_SECONDS {
            self.proc_groups = proc.cgroups().map(|mut l| {
                l.0.sort_by_key(|g| g.hierarchy);
                l.0
            });
            self.last_updated = Instant::now();
        }
    }
    fn handle_input(&mut self, input: KeyEvent, _height: u16) -> InputResult {
        match input.code {
            KeyCode::Up => {
                if self.select_idx > 0 {
                    self.select_idx -= 1;
                    InputResult::NeedsRedraw
                } else {
                    InputResult::None
                }
            }
            KeyCode::Down => {
                let max = self.proc_groups.as_ref().map_or_else(|_| 0, |v| v.len() - 1);
                if (self.select_idx as usize) < max {
                    self.select_idx += 1;
                    InputResult::NeedsRedraw
                } else {
                    InputResult::None
                }
            }
            _ => InputResult::None,
        }
    }
}
