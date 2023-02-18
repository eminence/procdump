use std::time::Instant;

use crossterm::event::{KeyCode, KeyEvent};
use procfs::{
    process::{MMapPath, MemoryMap, MemoryMapData, Process},
    ProcResult,
};
use tui::{
    backend::Backend,
    layout::Rect,
    style::{Color, Style},
    text::{Span, Spans, Text},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::{
    ui::{InputResult, ScrollController, TWO_SECONDS},
    util::fmt_bytes,
};

use super::AppWidget;

enum Maps {
    Maps(ProcResult<Vec<MemoryMap>>),
    SMaps(ProcResult<Vec<(MemoryMap, MemoryMapData)>>),
}

pub struct MapsWidget {
    maps: Maps,
    want_smaps: bool,
    last_updated: Instant,
    scroll: ScrollController,
    force_update: bool,
}

impl MapsWidget {
    pub fn new(proc: &Process) -> MapsWidget {
        MapsWidget {
            maps: Maps::Maps(proc.maps()),
            want_smaps: false,
            last_updated: Instant::now(),
            scroll: ScrollController::new(),
            force_update: false,
        }
    }
    pub fn draw_scrollbar<B: Backend>(&self, f: &mut Frame<B>, area: Rect) {
        self.scroll.draw_scrollbar(f, area)
    }
}

impl AppWidget for MapsWidget {
    const TITLE: &'static str = "Maps";
    fn draw<B: Backend>(&mut self, f: &mut Frame<B>, area: Rect, help_text: &mut Text) {
        let mut text: Vec<Spans> = Vec::new();

        let spans = Spans::from(vec![
            Span::raw("The "),
            Span::styled("Maps", Style::default().fg(Color::Yellow)),
            Span::raw(" tab shows the currently mapped memory regions. Press "),
            Span::styled("d", Style::default().fg(Color::Green)),
            Span::raw(" to toggle extra details about each map."),
        ]);
        help_text.extend(Text::from(spans));
        if self.want_smaps {
            let spans = Spans::from(vec![
                Span::raw(" The "),
                Span::styled("Size", Style::default().fg(Color::Magenta)),
                Span::raw(" column shows the total size of the mapped page, and the "),
                Span::styled("Rss", Style::default().fg(Color::Magenta)),
                Span::raw(" column shows how much of that size is mapped into physical memory."),
            ]);
            help_text.extend(Text::from(spans));
        }

        match &self.maps {
            Maps::Maps(Ok(maps)) => {
                for map in maps {
                    let mut line = vec![
                        Span::raw(format!("0x{:012x}-0x{:012x} ", map.address.0, map.address.1)),
                        Span::raw(format!("{} ", map.perms)),
                        Span::raw(format!("0x{: <8x} ", map.offset)),
                    ];
                    match &map.pathname {
                        MMapPath::Path(path) => line.push(Span::styled(
                            format!("{}\n", path.display()),
                            Style::default().fg(Color::Magenta),
                        )),
                        p @ MMapPath::Heap
                        | p @ MMapPath::Stack
                        | p @ MMapPath::Vdso
                        | p @ MMapPath::Vvar
                        | p @ MMapPath::Vsyscall
                        | p @ MMapPath::Anonymous => {
                            line.push(Span::styled(format!("{p:?}\n"), Style::default().fg(Color::Green)))
                        }
                        p => line.push(Span::raw(format!("{p:?}"))),
                    }
                    text.push(Spans::from(line));
                }
            }
            Maps::SMaps(Ok(maps)) => {
                let header_style = Style::default().fg(Color::Magenta);
                text.push(Spans::from(vec![
                    Span::styled(format!("{:29} ", "Address"), header_style),
                    Span::styled("Flag ", header_style),
                    Span::styled("Offset     ", header_style),
                    Span::styled("Size       ", header_style),
                    Span::styled("Rss        ", header_style),
                ]));
                for (map, map_data) in maps {
                    let mut line = vec![
                        Span::raw(format!("0x{:012x}-0x{:012x} ", map.address.0, map.address.1)),
                        Span::raw(format!("{:4} ", map.perms)),
                        Span::raw(format!("0x{: <8x} ", map.offset)),
                        Span::raw(format!(
                            "{:10} ",
                            map_data
                                .map
                                .get("Size")
                                .map(|b| fmt_bytes(*b, "B"))
                                .unwrap_or_else(|| "?".into()),
                        )),
                        Span::raw(format!(
                            "{:10} ",
                            map_data
                                .map
                                .get("Rss")
                                .map(|b| fmt_bytes(*b, "B"))
                                .unwrap_or_else(|| "?".into()),
                        )),
                    ];
                    match &map.pathname {
                        MMapPath::Path(path) => line.push(Span::styled(
                            format!("{}\n", path.display()),
                            Style::default().fg(Color::Magenta),
                        )),
                        p @ MMapPath::Heap
                        | p @ MMapPath::Stack
                        | p @ MMapPath::Vdso
                        | p @ MMapPath::Vvar
                        | p @ MMapPath::Vsyscall
                        | p @ MMapPath::Anonymous => {
                            line.push(Span::styled(format!("{p:?}\n"), Style::default().fg(Color::Green)))
                        }
                        p => line.push(Span::raw(format!("{p:?}"))),
                    }
                    text.push(Spans::from(line));
                }
            }
            Maps::Maps(Err(ref e)) | Maps::SMaps(Err(ref e)) => {
                text.push(Spans::from(Span::styled(
                    format!("Error getting maps: {e}"),
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
        if self.last_updated.elapsed() > TWO_SECONDS || self.force_update {
            if self.want_smaps {
                self.maps = Maps::SMaps(proc.smaps());
            } else {
                self.maps = Maps::Maps(proc.maps());
            }
            self.last_updated = Instant::now();
            self.force_update = false;
        }
    }
    fn handle_input(&mut self, input: KeyEvent, height: u16) -> InputResult {
        if let KeyCode::Char('d') = input.code {
            self.want_smaps = !self.want_smaps;
            self.force_update = true;
            return InputResult::NeedsUpdate;
        }
        self.scroll.handle_input(input, height)
    }
}
