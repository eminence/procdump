use std::{collections::HashMap, time::Instant};

use procfs::{
    net::{TcpNetEntry, UdpNetEntry, UnixNetEntry},
    process::{FDInfo, Process},
    ProcError,
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

use crate::ui::{InputResult, ScrollController, TWO_SECONDS};

use super::AppWidget;

pub struct NetWidget {
    tcp_map: HashMap<u64, TcpNetEntry>,
    udp_map: HashMap<u64, UdpNetEntry>,
    unix_map: HashMap<u64, UnixNetEntry>,
    fd: Result<Vec<FDInfo>, ProcError>,
    last_updated: Instant,
    scroll: ScrollController,
}

impl NetWidget {
    pub fn new(proc: &Process) -> NetWidget {
        NetWidget {
            tcp_map: crate::util::get_tcp_table(proc),
            udp_map: crate::util::get_udp_table(proc),
            unix_map: crate::util::get_unix_table(proc),
            fd: proc.fd().map(|iter| iter.filter_map(|f| f.ok()).collect()),
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
    fn draw<B: Backend>(&mut self, f: &mut Frame<B>, area: Rect, help_text: &mut Text) {
        let mut text: Vec<Spans> = Vec::new();

        let spans = Spans::from(vec![
            Span::raw("The "),
            Span::styled("Net", Style::default().fg(Color::Yellow)),
            Span::raw(" tab shows all of the open network connections."),
        ]);
        help_text.extend(Text::from(spans));

        match &self.fd {
            Ok(fd) => {
                for fd in fd {
                    if let procfs::process::FDTarget::Socket(inode) = fd.target {
                        if let Some(entry) = self.tcp_map.get(&inode) {
                            text.push(Spans::from(vec![
                                Span::styled("[tcp] ", Style::default().fg(Color::Green)),
                                Span::raw(format!(
                                    " {} -> {} ({:?})",
                                    entry.local_address, entry.remote_address, entry.state
                                )),
                            ]));
                        }
                        if let Some(entry) = self.udp_map.get(&inode) {
                            text.push(Spans::from(vec![
                                Span::styled("[udp] ", Style::default().fg(Color::Blue)),
                                Span::raw(format!(
                                    " {} -> {} ({:?})",
                                    entry.local_address, entry.remote_address, entry.state
                                )),
                            ]));
                        }
                        if let Some(entry) = self.unix_map.get(&inode) {
                            text.push(Spans::from(vec![
                                Span::styled("[unix]", Style::default().fg(Color::Yellow)),
                                Span::raw(match entry.socket_type as i32 {
                                    libc::SOCK_STREAM => " STREAM    ",
                                    libc::SOCK_DGRAM => " DGRAM     ",
                                    libc::SOCK_SEQPACKET => " SEQPACKET ",
                                    _ => "           ",
                                }),
                                if let Some(path) = &entry.path {
                                    Span::raw(format!(" {}", path.display()))
                                } else {
                                    Span::styled(" (no socket path)", Style::default().fg(Color::Gray))
                                },
                                Span::raw(format!(" ({:?})\n", entry.state)),
                            ]));
                        }
                    }
                }
            }
            Err(e) => {
                text.push(Spans::from(Span::styled(
                    format!("Error getting network connections: {e}"),
                    Style::default().fg(Color::Red).bg(Color::Reset),
                )));
            }
        }

        if text.is_empty() {
            text.push(Spans::from(Span::styled(
                "(no network connections)",
                Style::default().fg(Color::Gray).add_modifier(Modifier::DIM),
            )));
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
            self.fd = proc.fd().map(|iter| iter.filter_map(|f| f.ok()).collect());
            self.tcp_map = crate::util::get_tcp_table(proc);
            self.udp_map = crate::util::get_udp_table(proc);
            self.unix_map = crate::util::get_unix_table(proc);
            self.last_updated = Instant::now();
        }
    }
    fn handle_input(&mut self, input: Key, height: u16) -> InputResult {
        From::from(self.scroll.handle_input(input, height))
    }
}
