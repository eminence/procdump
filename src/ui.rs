use std::borrow::Cow;
use std::collections::HashMap;
use std::ffi::OsString;
use std::time::{Duration, Instant};

use procfs::net::{TcpNetEntry, UdpNetEntry, UnixNetEntry};
use procfs::process::{FDInfo, Process};
use procfs::{ProcError, ProcResult};
use termion::event::Key;
use tui::backend::Backend;
use tui::layout::Rect;
use tui::style::*;
use tui::terminal::Frame;
use tui::widgets::*;

use crate::util;
use crate::util::limit_to_string;

const TWO_SECONDS: Duration = Duration::from_secs(2);
const TEN_SECONDS: Duration = Duration::from_secs(10);

pub trait AppWidget {
    const TITLE: &'static str;

    fn draw<B: Backend>(&mut self, f: &mut Frame<B>, area: Rect);
    fn update(&mut self, proc: &Process);
    fn handle_input(&mut self, input: Key, height: u16) -> bool;
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
            Key::Down | Key::PageDown => {
                let to_move = std::cmp::max(self.max_scroll as i32 - self.scroll_offset as i32, 0);
                let to_move = std::cmp::min(
                    to_move,
                    if input == Key::PageDown {
                        pageupdown_size
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
}

impl AppWidget for EnvWidget {
    const TITLE: &'static str = "Env";
    fn handle_input(&mut self, input: Key, height: u16) -> bool {
        self.scroll.handle_input(input, height)
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
                                    " {} -> {})\n",
                                    entry.local_address, entry.remote_address
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
                                    text.push(Text::raw(format!(" {}\n", path.display())));
                                } else {
                                    text.push(Text::styled(
                                        " (no socket path)\n",
                                        Style::default().fg(Color::Gray),
                                    ));
                                }
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
    fn handle_input(&mut self, input: Key, height: u16) -> bool {
        self.scroll.handle_input(input, height)
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
    fn handle_input(&mut self, input: Key, height: u16) -> bool {
        self.scroll.handle_input(input, height)
    }
}

pub struct FilesWidget {
    fds: ProcResult<Vec<procfs::process::FDInfo>>,
    pipe_inodes: HashMap<u32, (util::ProcTreeInfo, util::ProcTreeInfo)>,
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
                    text.push(Text::styled(format!("{: <3}", fd.fd), fd_style));
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
    fn handle_input(&mut self, input: Key, height: u16) -> bool {
        self.scroll.handle_input(input, height)
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
                    Cow::Borrowed("Realtiem Timeout"),
                    limit_to_string(&limits.max_realtime_timeout.soft_limit),
                    limit_to_string(&limits.max_realtime_timeout.hard_limit),
                    Cow::Borrowed("(useconds)"),
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
            .widths(&[18, 12, 12, 11])
            .render(f, area);
    }
    fn update(&mut self, proc: &Process) {
        if self.last_updated.elapsed() > TWO_SECONDS {
            self.limits = proc.limits();
            self.last_updated = Instant::now();
        }
    }
    fn handle_input(&mut self, input: Key, height: u16) -> bool {
        self.scroll.handle_input(input, height)
    }
}

pub struct TreeWidget {
    parents: Vec<util::ProcTreeInfo>,
    children: Vec<util::ProcTreeInfo>,
    self_cmdline: String,
    self_pid: i32,
    last_updated: Instant,
    select_idx: i16,
}

impl TreeWidget {
    pub fn new(proc: &Process) -> TreeWidget {
        let (parents, children) = util::proc_tree(proc);
        TreeWidget {
            parents,
            children,
            self_pid: proc.stat.pid,
            self_cmdline: proc
                .cmdline()
                .ok()
                .map_or(proc.stat.comm.clone(), |cmdline| cmdline.join(" ")),
            last_updated: Instant::now(),
            select_idx: 0,
        }
    }
    pub fn get_selected_pid(&self) -> i32 {
        if self.select_idx < 0 {
            let idx = self.parents.len() as i16 + self.select_idx;
            self.parents[idx as usize].pid
        } else if self.select_idx == 0 {
            self.self_pid
        } else {
            self.children[(self.select_idx - 1) as usize].pid
        }
    }
}

impl AppWidget for TreeWidget {
    const TITLE: &'static str = "Tree";
    fn draw<B: Backend>(&mut self, f: &mut Frame<B>, area: Rect) {
        let selected_style = Style::default().fg(Color::Magenta);
        let unselected_style = Style::default();

        let mut text = Vec::new();
        let mut indent = if self.parents.is_empty() { 2 } else { 0 };
        // Show our parents
        for (i, pti) in self.parents.iter().enumerate() {
            if indent == 0 {
                text.push(Text::raw("\u{257e}\u{252c}\u{2574}"));
            } else {
                text.push(Text::raw(format!(
                    "{:width$}\u{2514}\u{252c}\u{2574}",
                    " ",
                    width = indent
                )));
            }
            text.push(Text::styled(
                format!("{} {}\n", pti.pid, pti.cmdline),
                if self.select_idx == 0 - self.parents.len() as i16 + i as i16 {
                    selected_style
                } else {
                    unselected_style
                },
            ));

            indent += 1;
        }
        // Show ourself in the tree
        text.push(Text::raw(format!(
            "{:width$}\u{2514}{}\u{2574}",
            " ",
            if self.children.is_empty() {
                "\u{2500}"
            } else {
                "\u{252c}"
            },
            width = indent
        )));
        text.push(Text::styled(
            format!("{} ", self.self_pid),
            if self.select_idx == 0 {
                Style::default().fg(Color::Yellow)
            } else {
                unselected_style
            },
        ));
        text.push(Text::styled(
            format!("{}\n", self.self_cmdline),
            Style::default().fg(Color::Yellow),
        ));

        // Show our children
        indent += 1;
        for (i, pti) in self.children.iter().enumerate() {
            text.push(Text::raw(format!(
                "{:width$}{}\u{2500}\u{2574}",
                " ",
                if i == self.children.len() - 1 {
                    "\u{2514}"
                } else {
                    "\u{251c}"
                },
                width = indent
            )));

            text.push(Text::styled(
                format!("{} {}\n", pti.pid, pti.cmdline),
                if (self.select_idx - 1) == i as i16 {
                    selected_style
                } else {
                    unselected_style
                },
            ));
        }

        // in general, we want to have our selected line in the middle of the screen:
        let target_offset = area.height as i32 / 2; // 12
        let selected_offset = self.select_idx as i32 + self.parents.len() as i32;
        let diff = selected_offset - target_offset;
        let max_scroll = std::cmp::max(
            0,
            self.parents.len() as i32 + 1 + self.children.len() as i32 - area.height as i32,
        );
        let scroll = std::cmp::min(std::cmp::max(0, diff), max_scroll as i32);

        //let max_scroll = get_numlines(text.iter(), area.width as usize) as i32 - area.height as i32;
        //self.set_max_scroll(max_scroll);
        Paragraph::new(text.iter())
            .block(Block::default().borders(Borders::NONE))
            .wrap(false)
            .scroll(scroll as u16)
            .render(f, area);
    }
    fn update(&mut self, proc: &Process) {
        if self.last_updated.elapsed() > TWO_SECONDS {
            let (parents, children) = util::proc_tree(proc);
            self.parents = parents;
            self.children = children;
            self.last_updated = Instant::now();
        }
    }
    fn handle_input(&mut self, input: Key, _height: u16) -> bool {
        match input {
            Key::Up => {
                if self.select_idx > 0 - self.parents.len() as i16 {
                    self.select_idx -= 1;
                    true
                } else {
                    false
                }
            }
            Key::Down => {
                if self.select_idx < self.children.len() as i16 {
                    self.select_idx += 1;
                    true
                } else {
                    false
                }
            }
            _ => false,
        }
    }
}
