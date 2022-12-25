use std::{collections::HashMap, ffi::CString, os::unix::prelude::OsStrExt, time::Instant};

use procfs::{
    net::{TcpNetEntry, UdpNetEntry, UnixNetEntry},
    process::{FDTarget, Process},
    ProcResult,
};
use termion::event::Key;
use tui::{
    backend::Backend,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Span, Spans, Text},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::{
    ui::{InputResult, ScrollController, TEN_SECONDS, TWO_SECONDS},
    util,
};

use super::AppWidget;

pub struct FilesWidget {
    fds: ProcResult<Vec<procfs::process::FDInfo>>,
    locks: ProcResult<Vec<procfs::Lock>>,
    pipe_inodes: HashMap<u64, (util::ProcessTreeEntry, util::ProcessTreeEntry)>,
    tcp_map: HashMap<u64, TcpNetEntry>,
    udp_map: HashMap<u64, UdpNetEntry>,
    unix_map: HashMap<u64, UnixNetEntry>,
    last_updated: Instant,
    pipes_updated: Instant,
    scroll: ScrollController,
}

impl FilesWidget {
    pub fn new(proc: &Process) -> FilesWidget {
        FilesWidget {
            fds: proc.fd().map(|iter| iter.filter_map(|f| f.ok()).collect()),
            locks: util::get_locks_for_pid(proc.pid),
            tcp_map: crate::util::get_tcp_table(proc),
            udp_map: crate::util::get_udp_table(proc),
            unix_map: crate::util::get_unix_table(proc),
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
    fn draw<B: Backend>(&mut self, f: &mut Frame<B>, area: Rect, help_text: &mut Text) {
        let mut text: Vec<Spans> = Vec::new();

        let spans = Spans::from(vec![
            Span::raw("The "),
            Span::styled("Files", Style::default().fg(Color::Yellow)),
            Span::raw(" tab shows the currently open files."),
        ]);
        help_text.extend(Text::from(spans));

        match self.fds {
            Ok(ref fds) => {
                let fd_style = Style::default().fg(Color::Green);
                for fd in fds {
                    let mut line = Vec::new();
                    line.push(Span::styled(format!("{: <3} ", fd.fd), fd_style));
                    match &fd.target {
                        FDTarget::Path(path) => {
                            line.push(Span::styled(
                                format!("{}", path.display()),
                                Style::default().fg(Color::Magenta),
                            ));

                            // get the inode and device for this path to see if it is locked
                            let cstr = CString::new(path.as_os_str().as_bytes()).unwrap();
                            let mut stat = unsafe { std::mem::zeroed() };
                            if unsafe { libc::stat(cstr.as_ptr(), &mut stat) } == 0 {
                                if let Ok(locks) = &self.locks {
                                    if let Some(lock) = locks.iter().find(|lock| {
                                        let lock_dev = libc::makedev(lock.devmaj, lock.devmin);
                                        lock.inode == stat.st_ino && stat.st_dev == lock_dev
                                    }) {
                                        line.push(Span::styled(
                                            format!(" ({:?} {:?} {:?})", lock.lock_type, lock.mode, lock.kind),
                                            Style::default().add_modifier(Modifier::DIM),
                                        ));
                                    }
                                }
                            }
                        }
                        FDTarget::Pipe(inode) => {
                            line.push(Span::styled(format!("pipe: {inode}"), Style::default().fg(Color::Blue)));

                            if let Some((rd_side, wr_side)) = self.pipe_inodes.get(inode) {
                                if fd.mode().contains(procfs::process::FDPermissions::READ) {
                                    line.push(Span::styled(
                                        format!(" (--> {} {})", wr_side.pid, wr_side.cmdline),
                                        Style::default().add_modifier(Modifier::DIM),
                                    ));
                                } else if fd.mode().contains(procfs::process::FDPermissions::WRITE) {
                                    line.push(Span::styled(
                                        format!(" (<-- {} {})", rd_side.pid, rd_side.cmdline),
                                        Style::default().add_modifier(Modifier::DIM),
                                    ));
                                }
                            }
                        }
                        FDTarget::Socket(inode) => {
                            line.push(Span::styled(
                                format!("socket: {inode} "),
                                Style::default().fg(Color::Yellow),
                            ));
                            // do we have an entry for this socket inode in any of our tables?
                            if let Some(entry) = self.tcp_map.get(inode) {
                                line.push(Span::raw(format!(
                                    "[tcp] {} -> {} ({:?})",
                                    entry.local_address, entry.remote_address, entry.state
                                )));
                            } else if let Some(entry) = self.udp_map.get(inode) {
                                line.push(Span::raw(format!(
                                    "[udp] {} -> {} ({:?})",
                                    entry.local_address, entry.remote_address, entry.state
                                )));
                            } else if let Some(entry) = self.unix_map.get(inode) {
                                line.push(Span::styled("[unix]", Style::default().fg(Color::Yellow)));
                                line.push(Span::raw(match entry.socket_type as i32 {
                                    libc::SOCK_STREAM => " STREAM    ",
                                    libc::SOCK_DGRAM => " DGRAM     ",
                                    libc::SOCK_SEQPACKET => " SEQPACKET ",
                                    _ => "           ",
                                }));
                                if let Some(path) = &entry.path {
                                    line.push(Span::raw(format!(" {}", path.display())));
                                } else {
                                    line.push(Span::styled(" (no socket path)", Style::default().fg(Color::Gray)));
                                }
                                line.push(Span::raw(format!(" ({:?})\n", entry.state)));
                            } else {
                                line.push(Span::styled(
                                    format!("socket: {inode}"),
                                    Style::default().fg(Color::Yellow),
                                ))
                            }
                        }
                        x => line.push(Span::raw(format!("{x:?}"))),
                    }
                    text.push(Spans::from(line));
                }
            }
            Err(ref e) => {
                text.push(Spans::from(Span::styled(
                    format!("Error getting fds: {e}"),
                    Style::default().fg(Color::Red).bg(Color::Reset),
                )));
            }
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
            self.fds = proc.fd().map(|iter| iter.filter_map(|f| f.ok()).collect());
            self.locks = util::get_locks_for_pid(proc.pid);
            self.last_updated = Instant::now();
            self.tcp_map = crate::util::get_tcp_table(proc);
            self.udp_map = crate::util::get_udp_table(proc);
            self.unix_map = crate::util::get_unix_table(proc);
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
