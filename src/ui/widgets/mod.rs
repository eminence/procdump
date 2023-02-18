use crossterm::event::KeyEvent;
use procfs::process::Process;
use tui::{backend::Backend, layout::Rect, text::Text, Frame};

use super::InputResult;

pub mod cgroup;
pub mod env;
pub mod files;
pub mod io;
pub mod limit;
pub mod maps;
pub mod mem;
pub mod net;
pub mod task;
pub mod tree;

pub use cgroup::*;
pub use env::*;
pub use files::*;
pub use io::*;
pub use limit::*;
pub use maps::*;
pub use mem::*;
pub use net::*;
pub use task::*;
pub use tree::*;

pub trait AppWidget {
    const TITLE: &'static str;

    fn draw<B: Backend>(&mut self, f: &mut Frame<B>, area: Rect, help_text: &mut Text);
    fn update(&mut self, proc: &Process);
    fn handle_input(&mut self, input: KeyEvent, height: u16) -> InputResult;
}
