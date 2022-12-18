use std::time::Instant;

use procfs::{
    process::{MMapPath, Process},
    ProcResult,
};
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
    fn draw<B: Backend>(&mut self, f: &mut Frame<B>, area: Rect, help_text: &mut Text) {
        let mut text: Vec<Spans> = Vec::new();

        let spans = Spans::from(vec![
            Span::raw("The "),
            Span::styled("Maps", Style::default().fg(Color::Yellow)),
            Span::raw(" tab shows the currently mapped memory regions."),
        ]);
        help_text.extend(Text::from(spans));

        match &self.maps {
            Ok(maps) => {
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
            Err(ref e) => {
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
        if self.last_updated.elapsed() > TWO_SECONDS {
            self.maps = proc.maps();
            self.last_updated = Instant::now();
        }
    }
    fn handle_input(&mut self, input: Key, height: u16) -> InputResult {
        From::from(self.scroll.handle_input(input, height))
    }
}
