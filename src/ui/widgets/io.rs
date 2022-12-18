use std::time::Instant;

use procfs::process::Process;
use termion::event::Key;
use tui::{
    backend::Backend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Span, Spans, Text},
    widgets::{Block, Borders, Paragraph, Sparkline, Wrap},
    Frame,
};

use crate::{
    ui::{InputResult, ONE_SECONDS},
    util::{fmt_bytes, fmt_rate},
    SparklineData, StatDelta,
};

use super::AppWidget;

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
            io_d: StatDelta::<procfs::process::Io>::new(proc),
            io_spark: SparklineData::new(),
            ops_spark: SparklineData::new(),
            disk_spark: SparklineData::new(),
        }
    }
}

impl AppWidget for IOWidget {
    const TITLE: &'static str = "IO";
    fn draw<B: Backend>(&mut self, f: &mut Frame<B>, area: Rect, help_text: &mut Text) {
        let spans = Spans::from(vec![
            Span::raw("The "),
            Span::styled("IO", Style::default().fg(Color::Yellow)),
            Span::raw(" tab shows various I/O stats. The "),
            Span::styled("blue", Style::default().fg(Color::LightCyan)),
            Span::raw(" graph shows all IO (bytes per sec), the"),
            Span::styled("magenta", Style::default().fg(Color::LightMagenta)),
            Span::raw(" graph shows IO ops per sec, and the "),
            Span::styled("green", Style::default().fg(Color::LightGreen)),
            Span::raw(" graph shows disk IO bytes per sec."),
        ]);
        help_text.extend(Text::from(spans));

        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .margin(0)
            .constraints([Constraint::Length(52), Constraint::Min(1)].as_ref())
            .split(area);

        let spark_colors = [Color::LightCyan, Color::LightMagenta, Color::LightGreen];
        let mut text: Vec<Spans> = Vec::new();
        let s = Style::default().fg(Color::Green);
        if let Ok(ref io_d) = self.io_d {
            let io = io_d.latest();
            let prev_io = io_d.previous();
            let duration = io_d.duration();
            let dur_sec = duration.as_millis() as f32 / 1000.0;

            // all IO
            text.push(Spans::from(vec![
                Span::styled("all io read: ", s),
                Span::raw(format!("{: <12}", fmt_bytes(io.rchar, "B"))),
                Span::styled("all io write:", s),
                Span::raw(format!("{: <12}", fmt_bytes(io.wchar, "B"))),
                Span::styled("\u{2503}", Style::default().fg(spark_colors[0])),
            ]));

            let io_read_rate = (io.rchar - prev_io.rchar) as f32 / dur_sec;
            let io_write_rate = (io.wchar - prev_io.wchar) as f32 / dur_sec;

            text.push(Spans::from(vec![
                Span::styled("read rate:   ", s),
                Span::raw(format!("{: <12}", fmt_rate(io_read_rate, "Bps"))),
                Span::styled("write rate:  ", s),
                Span::raw(format!("{: <12}", fmt_rate(io_write_rate, "Bps"))),
                Span::styled("\u{2503}", Style::default().fg(spark_colors[0])),
            ]));

            // syscalls
            text.push(Spans::from(vec![
                Span::styled("read ops:    ", s),
                Span::raw(format!("{: <12}", fmt_bytes(io.syscr, ""))),
                Span::styled("write ops:   ", s),
                Span::raw(format!("{: <12}", fmt_bytes(io.syscw, ""))),
                Span::styled("\u{2503}", Style::default().fg(spark_colors[1])),
            ]));

            let io_rop_rate = (io.syscr - prev_io.syscr) as f32 / dur_sec;
            let io_wop_rate = (io.syscw - prev_io.syscw) as f32 / dur_sec;

            text.push(Spans::from(vec![
                Span::styled("op rate:     ", s),
                Span::raw(format!("{: <12}", fmt_rate(io_rop_rate, "ps"))),
                Span::styled("op rate:     ", s),
                Span::raw(format!("{: <12}", fmt_rate(io_wop_rate, "ps"))),
                Span::styled("\u{2503}", Style::default().fg(spark_colors[1])),
            ]));

            // disk IO
            text.push(Spans::from(vec![
                Span::styled("disk reads:  ", s),
                Span::raw(format!("{: <12}", fmt_bytes(io.read_bytes, "B"))),
                Span::styled("disk writes: ", s),
                Span::raw(format!("{: <12}", fmt_bytes(io.write_bytes, "B"))),
                Span::styled("\u{2503}", Style::default().fg(spark_colors[2])),
            ]));

            let disk_read_rate = (io.read_bytes - prev_io.read_bytes) as f32 / dur_sec;
            let disk_write_rate = (io.write_bytes - prev_io.write_bytes) as f32 / dur_sec;

            text.push(Spans::from(vec![
                Span::styled("disk rate:   ", s),
                Span::raw(format!("{: <12}", fmt_rate(disk_read_rate, "Bps"))),
                Span::styled("disk rate:   ", s),
                Span::raw(format!("{: <12}", fmt_rate(disk_write_rate, "Bps"))),
                Span::styled("\u{2503}", Style::default().fg(spark_colors[2])),
            ]));

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

        let widget = Paragraph::new(text)
            .block(Block::default().borders(Borders::NONE))
            .wrap(Wrap { trim: true });
        f.render_widget(widget, chunks[0]);

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
            let max = std::cmp::max(*max, *data[s..].iter().max().unwrap_or(&1));
            let widget = Sparkline::default()
                .data(&data[s..])
                .max(max)
                .style(Style::default().fg(spark_colors[idx]));
            f.render_widget(widget, spark_chunks[idx]);
        }
    }
    fn update(&mut self, proc: &Process) {
        if self.last_updated.elapsed() > ONE_SECONDS {
            if let Ok(ref mut io_d) = self.io_d {
                io_d.update(proc);

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
                self.disk_spark.push((disk_read_rate + disk_write_rate) as u64);
            }
            self.last_updated = Instant::now();
        }
    }
    fn handle_input(&mut self, _input: Key, _height: u16) -> InputResult {
        InputResult::NeedsRedraw
    }
}
