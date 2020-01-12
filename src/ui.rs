use std::borrow::Cow;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::ffi::OsString;
use std::iter::FromIterator;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use procfs::net::{TcpNetEntry, UdpNetEntry, UnixNetEntry};
use procfs::process::{FDInfo, Process};
use procfs::ProcessCgroup;
use procfs::{ProcError, ProcResult};
use termion::event::Key;
use tui::backend::Backend;
use tui::layout::{Constraint, Direction, Layout, Rect};
use tui::style::*;
use tui::terminal::Frame;
use tui::widgets::*;

use crate::util;
use crate::util::{fmt_bytes, fmt_rate, limit_to_string};
use crate::{SparklineData, StatDelta};

const ONE_SECONDS: Duration = Duration::from_secs(1);
const TWO_SECONDS: Duration = Duration::from_secs(2);
const TEN_SECONDS: Duration = Duration::from_secs(10);

#[derive(Debug, Eq, PartialEq, Copy, Clone)]
pub enum InputResult {
    /// The widget needs to be redrawn
    NeedsRedraw,
    /// The widget needs to be updated with the latest process info (simplies NeedsRedraw)
    NeedsUpdate,
    None,
}

impl From<bool> for InputResult {
    fn from(b: bool) -> InputResult {
        if b {
            InputResult::NeedsRedraw
        } else {
            InputResult::None
        }
    }
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

pub trait AppWidget {
    const TITLE: &'static str;

    fn draw<B: Backend>(&mut self, f: &mut Frame<B>, area: Rect);
    fn update(&mut self, proc: &Process);
    fn handle_input(&mut self, input: Key, height: u16) -> InputResult;
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
    fn draw_scrollbar<B: Backend>(&self, f: &mut Frame<B>, area: Rect) {
        let p = (self.scroll_offset as f32 / self.max_scroll as f32) * area.height as f32;
        if p.is_nan() {
            return;
        }
        let whole = p.floor();
        let rest = p - whole;
        assert!(rest >= 0.0 && rest <= 1.0, "rest={} p={}", rest, p);
        //let symbols = "·⸱⸳.";
        let symbols = "\u{2588}\u{2587}\u{2586}\u{2585}\u{2584}\u{2583}\u{2582}\u{2581} "; // "█▇▆▅▄▃▂▁";
        let text = [
            Text::styled(
                "_".repeat(whole as usize),
                Style::default().fg(Color::Magenta).bg(Color::Magenta),
            ),
            {
                let idx = (rest * (symbols.chars().count() - 1) as f32).round() as usize;
                //assert!(idx <= 3, "idx={} rest={} len={}", idx, rest, symbols.chars().count());
                let c = symbols.chars().nth(idx);
                assert!(c.is_some(), "idx={}", idx);
                let c = c.unwrap();
                let fg = if c.is_whitespace() {
                    Color::Magenta
                } else {
                    Color::White
                };
                let s = format!("{}", if c.is_whitespace() { '+' } else { c });
                Text::styled(s, Style::default().fg(fg).bg(Color::Magenta))
            },
            Text::styled(
                "_".repeat(area.height as usize),
                Style::default().fg(Color::White).bg(Color::White),
            ),
        ];
        Paragraph::new(text.iter())
            .style(Style::default().fg(Color::White))
            .wrap(true)
            .render(f, area)
        //"·⸱⸳."
    }
    fn set_max_scroll(&mut self, max: i32) {
        let max: u16 = std::cmp::max(0, max) as u16;
        if self.scroll_offset >= max {
            self.scroll_offset = max
        }
        self.max_scroll = max;
    }
    fn handle_input(&mut self, input: Key, height: u16) -> bool {
        let pageupdown_size = height / 3;
        match input {
            Key::Down | Key::PageDown | Key::End => {
                let to_move = std::cmp::max(self.max_scroll as i32 - self.scroll_offset as i32, 0);
                let to_move = std::cmp::min(
                    to_move,
                    if input == Key::PageDown {
                        pageupdown_size
                    } else if input == Key::End {
                        self.max_scroll
                    } else {
                        1
                    } as i32,
                );
                if to_move > 0 {
                    self.scroll_offset += to_move as u16;
                    true
                } else {
                    false
                }
            }
            Key::Home => {
                let p = self.scroll_offset;
                self.scroll_offset = 0;
                p > 0
            }
            Key::Up | Key::PageUp => {
                let mut to_move = if input == Key::PageUp {
                    pageupdown_size
                } else {
                    1
                } as i32;
                if self.scroll_offset as i32 - to_move < 0 {
                    to_move = self.scroll_offset as i32;
                }
                if to_move > 0 {
                    self.scroll_offset -= to_move as u16;
                    true
                } else {
                    false
                }
            }
            _ => false,
        }
    }
}

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
    pub fn draw_scrollbar<B: Backend>(&self, f: &mut Frame<B>, area: Rect) {
        self.scroll.draw_scrollbar(f, area)
    }
}

impl AppWidget for EnvWidget {
    const TITLE: &'static str = "Env";
    fn handle_input(&mut self, input: Key, height: u16) -> InputResult {
        From::from(self.scroll.handle_input(input, height))
    }

    fn update(&mut self, proc: &Process) {
        if self.last_updated.elapsed() > TWO_SECONDS {
            self.env = proc.environ();
            self.last_updated = Instant::now();
        }
    }
    fn draw<B: Backend>(&mut self, f: &mut Frame<B>, area: Rect) {
        let mut text = Vec::new();

        match &self.env {
            Err(e) => {
                text.push(Text::styled(
                    format!("Error getting environment: {}", e),
                    crate::ERROR_STYLE,
                ));
            }
            Ok(map) => {
                let mut keys: Vec<_> = map.keys().collect();
                keys.sort_unstable();
                for key in keys {
                    text.push(Text::styled(
                        key.to_string_lossy().into_owned(),
                        Style::default().fg(Color::Green),
                    ));
                    text.push(Text::styled("=", Style::default().fg(Color::Green)));
                    text.push(Text::raw(map[key].to_string_lossy().into_owned()));
                    text.push(Text::raw("\n"));
                }
            }
        }
        let max_scroll =
            crate::get_numlines(text.iter(), area.width as usize) as i32 - area.height as i32;
        self.scroll.set_max_scroll(max_scroll);

        Paragraph::new(text.iter())
            .block(Block::default().borders(Borders::NONE))
            .wrap(true)
            .scroll(self.scroll.scroll_offset)
            .render(f, area);
    }
}

pub struct NetWidget {
    tcp_map: HashMap<u32, TcpNetEntry>,
    udp_map: HashMap<u32, UdpNetEntry>,
    unix_map: HashMap<u32, UnixNetEntry>,
    fd: Result<Vec<FDInfo>, ProcError>,
    last_updated: Instant,
    scroll: ScrollController,
}

impl NetWidget {
    pub fn new(proc: &Process) -> NetWidget {
        NetWidget {
            tcp_map: crate::util::get_tcp_table(),
            udp_map: crate::util::get_udp_table(),
            unix_map: crate::util::get_unix_table(),
            fd: proc.fd(),
            last_updated: Instant::now(),
            scroll: ScrollController::new(),
        }
    }
    pub fn draw_scrollbar<B: Backend>(&self, f: &mut Frame<B>, area: Rect) {
        self.scroll.draw_scrollbar(f, area)
    }
}

impl AppWidget for NetWidget {
    const TITLE: &'static str = "Net";
    fn draw<B: Backend>(&mut self, f: &mut Frame<B>, area: Rect) {
        let mut text: Vec<Text> = Vec::new();

        match &self.fd {
            Ok(fd) => {
                for fd in fd {
                    match fd.target {
                        procfs::process::FDTarget::Socket(inode) => {
                            if let Some(entry) = self.tcp_map.get(&inode) {
                                text.push(Text::styled(
                                    "[tcp] ",
                                    Style::default().fg(Color::Green),
                                ));
                                text.push(Text::raw(format!(
                                    " {} -> {} ({:?})\n",
                                    entry.local_address, entry.remote_address, entry.state
                                )));
                            }
                            if let Some(entry) = self.udp_map.get(&inode) {
                                text.push(Text::styled("[udp] ", Style::default().fg(Color::Blue)));
                                text.push(Text::raw(format!(
                                    " {} -> {} ({:?})\n",
                                    entry.local_address, entry.remote_address, entry.state
                                )));
                            }
                            if let Some(entry) = self.unix_map.get(&inode) {
                                text.push(Text::styled(
                                    "[unix]",
                                    Style::default().fg(Color::Yellow),
                                ));
                                text.push(Text::raw(match entry.socket_type as i32 {
                                    libc::SOCK_STREAM => " STREAM    ",
                                    libc::SOCK_DGRAM => " DGRAM     ",
                                    libc::SOCK_SEQPACKET => " SEQPACKET ",
                                    _ => "           ",
                                }));
                                if let Some(path) = &entry.path {
                                    text.push(Text::raw(format!(" {}", path.display())));
                                } else {
                                    text.push(Text::styled(
                                        " (no socket path)",
                                        Style::default().fg(Color::Gray),
                                    ));
                                }
                                text.push(Text::raw(format!(" ({:?})\n", entry.state)));
                            }
                        }
                        _ => {}
                    }
                }
            }
            Err(e) => {
                text.push(Text::styled(
                    format!("Error getting network connections: {}", e),
                    crate::ERROR_STYLE,
                ));
            }
        }

        if text.is_empty() {
            text.push(Text::styled(
                "(no network connections)",
                Style::default().fg(Color::Gray).modifier(Modifier::DIM),
            ));
        }

        let max_scroll =
            crate::get_numlines(text.iter(), area.width as usize) as i32 - area.height as i32;
        self.scroll.set_max_scroll(max_scroll);
        Paragraph::new(text.iter())
            .block(Block::default().borders(Borders::NONE))
            .wrap(false)
            .scroll(self.scroll.scroll_offset)
            .render(f, area);
    }
    fn update(&mut self, proc: &Process) {
        if self.last_updated.elapsed() > TWO_SECONDS {
            self.fd = proc.fd();
            self.tcp_map = crate::util::get_tcp_table();
            self.udp_map = crate::util::get_udp_table();
            self.unix_map = crate::util::get_unix_table();
            self.last_updated = Instant::now();
        }
    }
    fn handle_input(&mut self, input: Key, height: u16) -> InputResult {
        From::from(self.scroll.handle_input(input, height))
    }
}

pub struct MapsWidget {
    maps: ProcResult<Vec<procfs::process::MemoryMap>>,
    last_updated: Instant,
    scroll: ScrollController,
}

impl MapsWidget {
    pub fn new(proc: &Process) -> MapsWidget {
        MapsWidget {
            maps: proc.maps(),
            last_updated: Instant::now(),
            scroll: ScrollController::new(),
        }
    }
    pub fn draw_scrollbar<B: Backend>(&self, f: &mut Frame<B>, area: Rect) {
        self.scroll.draw_scrollbar(f, area)
    }
}

impl AppWidget for MapsWidget {
    const TITLE: &'static str = "Maps";
    fn draw<B: Backend>(&mut self, f: &mut Frame<B>, area: Rect) {
        let mut text = Vec::new();
        match &self.maps {
            Ok(maps) => {
                use procfs::process::MMapPath;
                for map in maps {
                    text.push(Text::raw(format!(
                        "0x{:012x}-0x{:012x} ",
                        map.address.0, map.address.1
                    )));
                    text.push(Text::raw(format!("{} ", map.perms)));
                    text.push(Text::raw(format!("0x{: <8x} ", map.offset)));
                    match &map.pathname {
                        MMapPath::Path(path) => text.push(Text::styled(
                            format!("{}\n", path.display()),
                            Style::default().fg(Color::Magenta),
                        )),
                        p @ MMapPath::Heap
                        | p @ MMapPath::Stack
                        | p @ MMapPath::Vdso
                        | p @ MMapPath::Vvar
                        | p @ MMapPath::Vsyscall
                        | p @ MMapPath::Anonymous => text.push(Text::styled(
                            format!("{:?}\n", p),
                            Style::default().fg(Color::Green),
                        )),
                        p => text.push(Text::raw(format!("{:?}\n", p))),
                    }
                }
            }
            Err(ref e) => {
                text.push(Text::styled(
                    format!("Error getting maps: {}", e),
                    crate::ERROR_STYLE,
                ));
            }
        }
        let max_scroll =
            crate::get_numlines(text.iter(), area.width as usize) as i32 - area.height as i32;
        self.scroll.set_max_scroll(max_scroll);

        Paragraph::new(text.iter())
            .block(Block::default().borders(Borders::NONE))
            .wrap(false)
            .scroll(self.scroll.scroll_offset)
            .render(f, area);
    }
    fn update(&mut self, proc: &Process) {
        if self.last_updated.elapsed() > TWO_SECONDS {
            self.maps = proc.maps();
            self.last_updated = Instant::now();
        }
    }
    fn handle_input(&mut self, input: Key, height: u16) -> InputResult {
        From::from(self.scroll.handle_input(input, height))
    }
}

pub struct FilesWidget {
    fds: ProcResult<Vec<procfs::process::FDInfo>>,
    pipe_inodes: HashMap<u32, (util::ProcessTreeEntry, util::ProcessTreeEntry)>,
    last_updated: Instant,
    pipes_updated: Instant,
    scroll: ScrollController,
}

impl FilesWidget {
    pub fn new(proc: &Process) -> FilesWidget {
        FilesWidget {
            fds: proc.fd(),
            last_updated: Instant::now(),
            pipe_inodes: util::get_pipe_pairs(),
            pipes_updated: Instant::now(),
            scroll: ScrollController::new(),
        }
    }
    pub fn draw_scrollbar<B: Backend>(&self, f: &mut Frame<B>, area: Rect) {
        self.scroll.draw_scrollbar(f, area)
    }
}

impl AppWidget for FilesWidget {
    const TITLE: &'static str = "Files";
    fn draw<B: Backend>(&mut self, f: &mut Frame<B>, area: Rect) {
        let mut text = Vec::new();
        match self.fds {
            Ok(ref fds) => {
                use procfs::process::FDTarget;
                let fd_style = Style::default().fg(Color::Green);
                for fd in fds {
                    text.push(Text::styled(format!("{: <3} ", fd.fd), fd_style));
                    match &fd.target {
                        FDTarget::Path(path) => text.push(Text::styled(
                            format!("{}", path.display()),
                            Style::default().fg(Color::Magenta),
                        )),
                        FDTarget::Pipe(inode) => {
                            text.push(Text::styled(
                                format!("pipe: {}", inode),
                                Style::default().fg(Color::Blue),
                            ));

                            if let Some((rd_side, wr_side)) = self.pipe_inodes.get(&inode) {
                                if fd.mode().contains(procfs::process::FDPermissions::READ) {
                                    text.push(Text::styled(
                                        format!(" (--> {} {})", wr_side.pid, wr_side.cmdline),
                                        Style::default().modifier(Modifier::DIM),
                                    ));
                                } else if fd.mode().contains(procfs::process::FDPermissions::WRITE)
                                {
                                    text.push(Text::styled(
                                        format!(" (<-- {} {})", rd_side.pid, rd_side.cmdline),
                                        Style::default().modifier(Modifier::DIM),
                                    ));
                                }
                            }
                        }
                        FDTarget::Socket(inode) => text.push(Text::styled(
                            format!("socket: {}", inode),
                            Style::default().fg(Color::Yellow),
                        )),
                        x => text.push(Text::raw(format!("{:?}", x))),
                    }
                    text.push(Text::raw("\n"));
                }
            }
            Err(ref e) => {
                text.push(Text::styled(
                    format!("Error getting fds: {}", e),
                    crate::ERROR_STYLE,
                ));
            }
        }

        let max_scroll =
            crate::get_numlines(text.iter(), area.width as usize) as i32 - area.height as i32;
        self.scroll.set_max_scroll(max_scroll);

        Paragraph::new(text.iter())
            .block(Block::default().borders(Borders::NONE))
            .wrap(false)
            .scroll(self.scroll.scroll_offset)
            .render(f, area);
    }
    fn update(&mut self, proc: &Process) {
        if self.last_updated.elapsed() > TWO_SECONDS {
            self.fds = proc.fd();
            self.last_updated = Instant::now();
        }
        if self.pipes_updated.elapsed() > TEN_SECONDS {
            self.pipe_inodes = util::get_pipe_pairs();
            self.pipes_updated = Instant::now();
        }
    }
    fn handle_input(&mut self, input: Key, height: u16) -> InputResult {
        From::from(self.scroll.handle_input(input, height))
    }
}

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
    fn draw<B: Backend>(&mut self, f: &mut Frame<B>, area: Rect) {
        let headers = ["Type", "Soft Limit", "Hard Limit", ""];
        let mut rows = Vec::new();
        if let Ok(ref limits) = self.limits {
            rows.push(Row::Data(
                vec![
                    Cow::Borrowed("Cpu Time"),
                    limit_to_string(&limits.max_cpu_time.soft_limit),
                    limit_to_string(&limits.max_cpu_time.hard_limit),
                    Cow::Borrowed("(seconds)"),
                ]
                .into_iter(),
            ));
            rows.push(Row::Data(
                vec![
                    Cow::Borrowed("File Size"),
                    limit_to_string(&limits.max_file_size.soft_limit),
                    limit_to_string(&limits.max_file_size.hard_limit),
                    Cow::Borrowed("(bytes)"),
                ]
                .into_iter(),
            ));
            rows.push(Row::Data(
                vec![
                    Cow::Borrowed("Data Size"),
                    limit_to_string(&limits.max_data_size.soft_limit),
                    limit_to_string(&limits.max_data_size.hard_limit),
                    Cow::Borrowed("(bytes)"),
                ]
                .into_iter(),
            ));
            rows.push(Row::Data(
                vec![
                    Cow::Borrowed("Stack Size"),
                    limit_to_string(&limits.max_stack_size.soft_limit),
                    limit_to_string(&limits.max_stack_size.hard_limit),
                    Cow::Borrowed("(bytes)"),
                ]
                .into_iter(),
            ));
            rows.push(Row::Data(
                vec![
                    Cow::Borrowed("Core File Size"),
                    limit_to_string(&limits.max_core_file_size.soft_limit),
                    limit_to_string(&limits.max_core_file_size.hard_limit),
                    Cow::Borrowed("(bytes)"),
                ]
                .into_iter(),
            ));
            rows.push(Row::Data(
                vec![
                    Cow::Borrowed("Resident Set"),
                    limit_to_string(&limits.max_resident_set.soft_limit),
                    limit_to_string(&limits.max_resident_set.hard_limit),
                    Cow::Borrowed("(bytes)"),
                ]
                .into_iter(),
            ));
            rows.push(Row::Data(
                vec![
                    Cow::Borrowed("Processes"),
                    limit_to_string(&limits.max_processes.soft_limit),
                    limit_to_string(&limits.max_processes.hard_limit),
                    Cow::Borrowed(""),
                ]
                .into_iter(),
            ));
            rows.push(Row::Data(
                vec![
                    Cow::Borrowed("Open Files"),
                    limit_to_string(&limits.max_open_files.soft_limit),
                    limit_to_string(&limits.max_open_files.hard_limit),
                    Cow::Borrowed(""),
                ]
                .into_iter(),
            ));
            rows.push(Row::Data(
                vec![
                    Cow::Borrowed("Locked Memory"),
                    limit_to_string(&limits.max_locked_memory.soft_limit),
                    limit_to_string(&limits.max_locked_memory.hard_limit),
                    Cow::Borrowed("(bytes)"),
                ]
                .into_iter(),
            ));
            rows.push(Row::Data(
                vec![
                    Cow::Borrowed("Address Space"),
                    limit_to_string(&limits.max_address_space.soft_limit),
                    limit_to_string(&limits.max_address_space.hard_limit),
                    Cow::Borrowed(""),
                ]
                .into_iter(),
            ));
            rows.push(Row::Data(
                vec![
                    Cow::Borrowed("File Locks"),
                    limit_to_string(&limits.max_file_locks.soft_limit),
                    limit_to_string(&limits.max_file_locks.hard_limit),
                    Cow::Borrowed(""),
                ]
                .into_iter(),
            ));
            rows.push(Row::Data(
                vec![
                    Cow::Borrowed("Pending Signals"),
                    limit_to_string(&limits.max_pending_signals.soft_limit),
                    limit_to_string(&limits.max_pending_signals.hard_limit),
                    Cow::Borrowed(""),
                ]
                .into_iter(),
            ));
            rows.push(Row::Data(
                vec![
                    Cow::Borrowed("Msgqueue Size"),
                    limit_to_string(&limits.max_msgqueue_size.soft_limit),
                    limit_to_string(&limits.max_msgqueue_size.hard_limit),
                    Cow::Borrowed("(bytes)"),
                ]
                .into_iter(),
            ));
            rows.push(Row::Data(
                vec![
                    Cow::Borrowed("Nice Priority"),
                    limit_to_string(&limits.max_nice_priority.soft_limit),
                    limit_to_string(&limits.max_nice_priority.hard_limit),
                    Cow::Borrowed(""),
                ]
                .into_iter(),
            ));
            rows.push(Row::Data(
                vec![
                    Cow::Borrowed("Realtime Priority"),
                    limit_to_string(&limits.max_realtime_priority.soft_limit),
                    limit_to_string(&limits.max_realtime_priority.hard_limit),
                    Cow::Borrowed(""),
                ]
                .into_iter(),
            ));
            rows.push(Row::Data(
                vec![
                    Cow::Borrowed("Realtime Timeout"),
                    limit_to_string(&limits.max_realtime_timeout.soft_limit),
                    limit_to_string(&limits.max_realtime_timeout.hard_limit),
                    Cow::Borrowed("(μseconds)"),
                ]
                .into_iter(),
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

        Table::new(headers.iter(), rows.into_iter())
            .widths(&[
                Constraint::Length(18),
                Constraint::Length(12),
                Constraint::Length(12),
                Constraint::Length(11),
            ])
            .render(f, area);
    }
    fn update(&mut self, proc: &Process) {
        if self.last_updated.elapsed() > TWO_SECONDS {
            self.limits = proc.limits();
            self.last_updated = Instant::now();
        }
    }
    fn handle_input(&mut self, input: Key, height: u16) -> InputResult {
        From::from(self.scroll.handle_input(input, height))
    }
}

pub struct TreeWidget {
    tree: util::ProcessTree,
    last_updated: Instant,
    force_update: bool,
    /// The currently selected PID
    selected_pid: i32,
    show_all: bool,
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
        }
    }
    pub fn get_selected_pid(&self) -> i32 {
        self.selected_pid
    }
}

impl AppWidget for TreeWidget {
    const TITLE: &'static str = "Tree";
    fn draw<B: Backend>(&mut self, f: &mut Frame<B>, area: Rect) {
        let selected_style = Style::default().fg(Color::Magenta);
        let unselected_style = Style::default();

        let mut text = Vec::new();

        let flattened = self.tree.flatten();

        let mut iter = flattened.iter().enumerate().peekable();
        let mut last_depth = 0;
        let mut prints = Vec::new();
        while let Some((idx, (depth, item))) = iter.next() {
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
                        } else {
                            if p_idx == depth - 1 {
                                "┗"
                            } else {
                                " "
                            }
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("")
            };
            text.push(Text::raw(lines));

            if idx > 0 {
                let has_children = iter
                    .peek()
                    .map(|(_, (p_depth, _))| *p_depth as usize > depth)
                    .unwrap_or(false);
                text.push(Text::raw(format!(
                    "{b}╸",
                    b = if has_children { "┳" } else { "━" },
                )));
            }

            text.push(Text::styled(
                format!("{} {}\n", item.pid, item.cmdline),
                if item.pid == self.selected_pid {
                    selected_style
                } else {
                    unselected_style
                },
            ));
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
        let scroll = std::cmp::min(std::cmp::max(0, diff), max_scroll as i32);

        //let max_scroll = get_numlines(text.iter(), area.width as usize) as i32 - area.height as i32;
        //self.set_max_scroll(max_scroll);
        Paragraph::new(text.iter())
            .block(Block::default().borders(Borders::NONE))
            .scroll(scroll as u16)
            .wrap(false)
            .render(f, area);
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
            self.tree = util::ProcessTree::new(if self.show_all {
                None
            } else {
                Some((&parents, proc))
            })
            .unwrap();
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
    fn handle_input(&mut self, input: Key, _height: u16) -> InputResult {
        let flattened = self.tree.flatten();
        // the current index of the selected pid
        let mut select_idx = flattened
            .iter()
            .enumerate()
            .find(|(_idx, (_, item))| item.pid == self.selected_pid)
            .unwrap()
            .0 as i32;

        let r = match input {
            Key::Ctrl('t') => {
                self.show_all = !self.show_all;
                self.force_update = true;
                return InputResult::NeedsUpdate;
            }
            Key::Up => {
                if select_idx > 0 {
                    select_idx -= 1;
                    true
                } else {
                    false
                }
            }
            Key::Down => {
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
        From::from(r)
    }
}

pub struct CGroupWidget {
    proc_groups: ProcResult<Vec<ProcessCgroup>>,
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
            .unwrap_or_default()
            .into_iter()
            .filter_map(|cg| if cg.enabled { Some(cg.name) } else { None })
            .collect();

        if let Ok(mountinfo) = proc.mountinfo() {
            for mut mi in mountinfo {
                if mi.fs_type == "cgroup" {
                    let super_options: HashSet<String> =
                        HashSet::from_iter(mi.super_options.drain().map(|(k, _)| k));
                    let controllers: BTreeSet<String> =
                        super_options.intersection(&groups).cloned().collect();
                    map.insert(controllers, mi.mount_point);
                }
            }
        }

        let groups = proc.cgroups().map(|mut l| {
            l.sort_by_key(|g| g.hierarchy);
            l
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
    fn draw<B: Backend>(&mut self, f: &mut Frame<B>, area: Rect) {
        use std::fs::read_to_string;

        // split the area in half -- the left side is a selectable list of controllers, and the
        // right side is some details about them

        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .margin(0)
            .constraints([Constraint::Ratio(1, 2), Constraint::Ratio(1, 2)].as_ref())
            .split(area);

        let green = Style::default().fg(Color::Green);
        let selected = Style::default().fg(Color::Yellow);

        let mut text = Vec::new();
        let mut details = Vec::new();

        if let Ok(cgroups) = &self.proc_groups {
            for (idx, cg) in cgroups.iter().enumerate() {
                let current = idx == self.select_idx as usize;
                let groups = BTreeSet::from_iter(cg.controllers.clone());
                let controller_name = if cg.controllers.is_empty() {
                    "???".to_owned()
                } else {
                    cg.controllers.join(",")
                };
                if let Some(mountpoint) = self.v1_controllers.get(&groups) {
                    text.push(Text::styled(
                        format!("{}: ", controller_name),
                        if current { green } else { selected },
                    ));
                    text.push(Text::raw(format!("{}\n", cg.pathname)));

                    let root = if cg.pathname.starts_with('/') {
                        mountpoint.join(&cg.pathname[1..])
                    } else {
                        mountpoint.join(&cg.pathname)
                    };

                    if current {
                        details.push(Text::raw(format!("{:?}\n", groups)));
                        if groups.contains("pids") {
                            let current = read_to_string(root.join("pids.current"));
                            let max = read_to_string(root.join("pids.max"));
                            if let (Ok(current), Ok(max)) = (current, max) {
                                details.push(Text::raw(format!(
                                    "{} of {}\n",
                                    current.trim(),
                                    max.trim()
                                )));
                            }
                        }
                        if groups.contains("freezer") {
                            let state = read_to_string(root.join("freezer.state"));
                            if let Ok(state) = state {
                                details.push(Text::raw(format!("state: {}\n", state.trim())));
                            }
                        }
                        if groups.contains("memory") {
                            if let Ok(usage) = read_to_string(root.join("memory.usage_in_bytes")) {
                                details.push(Text::raw(format!(
                                    "Group Usage: {} bytes\n",
                                    usage.trim()
                                )));
                            }
                            if let Ok(limit) = read_to_string(root.join("memory.limit_in_bytes")) {
                                details.push(Text::raw(format!(
                                    "Group Limit: {} bytes\n",
                                    limit.trim()
                                )));
                            }
                            if let Ok(usage) =
                                read_to_string(root.join("memory.kmem.usage_in_bytes"))
                            {
                                details.push(Text::raw(format!(
                                    "Kernel Usage: {} bytes\n",
                                    usage.trim()
                                )));
                            }
                            if let Ok(limit) =
                                read_to_string(root.join("memory.kmem.limit_in_bytes"))
                            {
                                details.push(Text::raw(format!(
                                    "Kernel Limit: {} bytes\n",
                                    limit.trim()
                                )));
                            }
                            if let Ok(limit) = read_to_string(root.join("memory.stat")) {
                                details.push(Text::raw("stats:\n"));
                                details.push(Text::raw(limit));
                            }
                        }
                        if groups.contains("net_cls") {
                            if let Ok(classid) = read_to_string(root.join("net_cls.classid")) {
                                details.push(Text::raw(format!("Class ID: {}\n", classid.trim())));
                            }
                        }
                        if groups.contains("net_prio") {
                            if let Ok(idx) = read_to_string(root.join("net_prio.prioidx")) {
                                details.push(Text::raw(format!("Prioidx: {}\n", idx)));
                            }
                            if let Ok(map) = read_to_string(root.join("net_prio.ifpriomap")) {
                                details.push(Text::raw("ifpriomap:\n"));
                                details.push(Text::raw(map));
                            }
                        }
                        if groups.contains("blkio") {}
                        if groups.contains("cpuacct") {
                            if let Ok(acct) = read_to_string(root.join("cpuacct.usage")) {
                                details.push(Text::raw(format!(
                                    "Total nanoseconds: {}\n",
                                    acct.trim()
                                )));
                            }
                            if let Ok(usage_all) = read_to_string(root.join("cpuacct.usage_all")) {
                                details.push(Text::raw(usage_all));
                            }
                        }
                        {
                            details.push(Text::raw(format!("--> {:?}\n", mountpoint)));
                            details.push(Text::raw(format!("--> {:?}\n", cg.pathname)));
                        }
                    }
                } else {
                    text.push(Text::styled(
                        format!("{}: ", controller_name),
                        if current {
                            green.modifier(Modifier::DIM)
                        } else {
                            selected.modifier(Modifier::DIM)
                        },
                    ));
                    text.push(Text::raw(format!("{}\n", cg.pathname)));
                    if idx == self.select_idx as usize {
                        details.push(Text::raw("This controller isn't supported by procdump"));
                    }
                }
            }
        }

        let target_offset = chunks[0].height as i32 / 2; // 12
        let diff = self.select_idx as i32 - target_offset;
        let max_scroll = std::cmp::max(0, text.len() as i32 - chunks[0].height as i32);
        let scroll = std::cmp::min(std::cmp::max(0, diff), max_scroll as i32);

        Paragraph::new(text.iter())
            .block(Block::default().borders(Borders::NONE))
            .wrap(false)
            .scroll(scroll as u16)
            .render(f, chunks[0]);

        Paragraph::new(details.iter())
            .block(Block::default().borders(Borders::LEFT))
            .wrap(true)
            .render(f, chunks[1]);
    }
    fn update(&mut self, proc: &Process) {
        if self.last_updated.elapsed() > TEN_SECONDS {
            self.proc_groups = proc.cgroups().map(|mut l| {
                l.sort_by_key(|g| g.hierarchy);
                l
            });
            self.last_updated = Instant::now();
        }
    }
    fn handle_input(&mut self, input: Key, height: u16) -> InputResult {
        From::from(match input {
            Key::Up => {
                if self.select_idx > 0 {
                    self.select_idx -= 1;
                    true
                } else {
                    false
                }
            }
            Key::Down => {
                let max = self
                    .proc_groups
                    .as_ref()
                    .map_or_else(|_| 0, |v| v.len() - 1);
                if (self.select_idx as usize) < max {
                    self.select_idx += 1;
                    true
                } else {
                    false
                }
            }
            _ => false,
        })
    }
}

pub struct IOWidget {
    last_updated: Instant,
    //io: procfs::ProcResult<procfs::process::Io>,
    io_d: anyhow::Result<StatDelta<procfs::process::Io>>,
    io_spark: SparklineData,
    ops_spark: SparklineData,
    disk_spark: SparklineData,
}

impl IOWidget {
    pub fn new(proc: &Process) -> IOWidget {
        //let io = proc.io();
        IOWidget {
            last_updated: Instant::now(),
            io_d: StatDelta::<procfs::process::Io>::new(proc.clone()),
            io_spark: SparklineData::new(),
            ops_spark: SparklineData::new(),
            disk_spark: SparklineData::new(),
        }
    }
}

impl AppWidget for IOWidget {
    const TITLE: &'static str = "IO";
    fn draw<B: Backend>(&mut self, f: &mut Frame<B>, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .margin(0)
            .constraints([Constraint::Length(52), Constraint::Min(1)].as_ref())
            .split(area);

        let spark_colors = [Color::LightCyan, Color::LightMagenta, Color::LightGreen];
        let mut text = Vec::new();
        let s = Style::default().fg(Color::Green);
        if let Ok(ref io_d) = self.io_d {
            let io = io_d.latest();
            let prev_io = io_d.previous();
            let duration = io_d.duration();
            let dur_sec = duration.as_millis() as f32 / 1000.0;

            // all IO
            text.push(Text::styled("all io read: ", s));
            text.push(Text::raw(format!("{: <12}", fmt_bytes(io.rchar, "B"))));
            text.push(Text::styled("all io write:", s));
            text.push(Text::raw(format!("{: <12}", fmt_bytes(io.wchar, "B"))));
            text.push(Text::styled(
                "\u{2503}\n",
                Style::default().fg(spark_colors[0]),
            ));

            let io_read_rate = (io.rchar - prev_io.rchar) as f32 / dur_sec;
            let io_write_rate = (io.wchar - prev_io.wchar) as f32 / dur_sec;

            text.push(Text::styled("read rate:   ", s));
            text.push(Text::raw(format!("{: <12}", fmt_rate(io_read_rate, "Bps"))));
            text.push(Text::styled("write rate:  ", s));
            text.push(Text::raw(format!(
                "{: <12}",
                fmt_rate(io_write_rate, "Bps")
            )));
            text.push(Text::styled(
                "\u{2503}\n",
                Style::default().fg(spark_colors[0]),
            ));

            // syscalls
            text.push(Text::styled("read ops:    ", s));
            text.push(Text::raw(format!("{: <12}", fmt_bytes(io.syscr, ""))));
            text.push(Text::styled("write ops:   ", s));
            text.push(Text::raw(format!("{: <12}", fmt_bytes(io.syscw, ""))));
            text.push(Text::styled(
                "\u{2503}\n",
                Style::default().fg(spark_colors[1]),
            ));

            let io_rop_rate = (io.syscr - prev_io.syscr) as f32 / dur_sec;
            let io_wop_rate = (io.syscw - prev_io.syscw) as f32 / dur_sec;

            text.push(Text::styled("op rate:     ", s));
            text.push(Text::raw(format!("{: <12}", fmt_rate(io_rop_rate, "ps"))));
            text.push(Text::styled("op rate:     ", s));
            text.push(Text::raw(format!("{: <12}", fmt_rate(io_wop_rate, "ps"))));
            text.push(Text::styled(
                "\u{2503}\n",
                Style::default().fg(spark_colors[1]),
            ));

            // disk IO
            text.push(Text::styled("disk reads:  ", s));
            text.push(Text::raw(format!("{: <12}", fmt_bytes(io.read_bytes, "B"))));
            text.push(Text::styled("disk writes: ", s));
            text.push(Text::raw(format!(
                "{: <12}",
                fmt_bytes(io.write_bytes, "B")
            )));
            text.push(Text::styled(
                "\u{2503}\n",
                Style::default().fg(spark_colors[2]),
            ));

            let disk_read_rate = (io.read_bytes - prev_io.read_bytes) as f32 / dur_sec;
            let disk_write_rate = (io.write_bytes - prev_io.write_bytes) as f32 / dur_sec;

            text.push(Text::styled("disk rate:   ", s));
            text.push(Text::raw(format!(
                "{: <12}",
                fmt_rate(disk_read_rate, "Bps")
            )));
            text.push(Text::styled("disk rate:   ", s));
            text.push(Text::raw(format!(
                "{: <12}",
                fmt_rate(disk_write_rate, "Bps")
            )));
            text.push(Text::styled(
                "\u{2503}\n",
                Style::default().fg(spark_colors[2]),
            ));

            //let rps  = (io.rchar - prev_io.rchar) as f32 / dur_sec;
            //text.push(Text::raw(format!("{} ({}) ", fmt_bytes(io.rchar), fmt_rate(rps))));

            //text.push(Text::styled("ops:", s.clone()));
            //let ops = (io.syscr - prev_io.syscr) as f32 / dur_sec;
            //text.push(Text::raw(format!("{} ({})", fmt_bytes(io.syscr), fmt_rate(ops))));
            //
            //text.push(Text::styled("disk:", s.clone()));
            //let rps = (io.read_bytes - prev_io.read_bytes) as f32 / dur_sec;
            //text.push(Text::raw(format!("{} ({})", fmt_bytes(io.read_bytes), fmt_rate(rps))));
        }

        Paragraph::new(text.iter())
            .block(Block::default().borders(Borders::NONE))
            .wrap(true)
            .render(f, chunks[0]);

        // split the right side into 3 areas to draw the sparklines
        //
        let spark_chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(0)
            .constraints(
                [
                    Constraint::Max(2),
                    Constraint::Max(2),
                    Constraint::Max(2),
                    Constraint::Max(2),
                ]
                .as_ref(),
            )
            .split(chunks[1]);

        for (idx, (data, max)) in [
            self.io_spark.as_slice(),
            self.ops_spark.as_slice(),
            self.disk_spark.as_slice(),
        ]
        .iter()
        .zip([10000, 100, 10000].iter())
        .enumerate()
        {
            let s = std::cmp::max(0, data.len() as i32 - chunks[1].width as i32) as usize;
            let max = std::cmp::max(*max, *data[s..].into_iter().max().unwrap_or(&1) as u64);
            Sparkline::default()
                .data(&data[s..])
                .max(max)
                .style(Style::default().fg(spark_colors[idx]))
                .render(f, spark_chunks[idx]);
        }
    }
    fn update(&mut self, _proc: &Process) {
        if self.last_updated.elapsed() > ONE_SECONDS {
            if let Ok(ref mut io_d) = self.io_d {
                io_d.update();

                let io = io_d.latest();
                let prev_io = io_d.previous();
                let duration = io_d.duration();
                let dur_sec = duration.as_millis() as f32 / 1000.0;

                let io_read_rate = (io.rchar - prev_io.rchar) as f32 / dur_sec;
                let io_write_rate = (io.wchar - prev_io.wchar) as f32 / dur_sec;
                self.io_spark.push((io_read_rate + io_write_rate) as u64);

                let io_rop_rate = (io.syscr - prev_io.syscr) as f32 / dur_sec;
                let io_wop_rate = (io.syscw - prev_io.syscw) as f32 / dur_sec;
                self.ops_spark.push((io_rop_rate + io_wop_rate) as u64);

                let disk_read_rate = (io.read_bytes - prev_io.read_bytes) as f32 / dur_sec;
                let disk_write_rate = (io.write_bytes - prev_io.write_bytes) as f32 / dur_sec;
                self.disk_spark
                    .push((disk_read_rate + disk_write_rate) as u64);
            }
            self.last_updated = Instant::now();
        }
    }
    fn handle_input(&mut self, _input: Key, _height: u16) -> InputResult {
        InputResult::NeedsRedraw
    }
}
