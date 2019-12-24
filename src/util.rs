use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt::Display;
use std::sync::mpsc;
use std::thread;

use procfs::process::{all_processes, LimitValue, Process};
use termion::event::{Key, MouseEvent};
use tui::widgets::Text;

pub(crate) struct ProcTreeInfo {
    pub pid: i32,
    ppid: i32,
    pub cmdline: String,
}

/// Returns a list of parent processes, and a list of direct child processes
pub(crate) fn proc_tree(proc: &Process) -> (Vec<ProcTreeInfo>, Vec<ProcTreeInfo>) {
    let all = all_processes().unwrap();

    let mut map = HashMap::new();

    let mut children = Vec::new();
    for p in all {
        let pti = ProcTreeInfo {
            pid: p.stat.pid,
            ppid: p.stat.ppid,
            cmdline: p
                .cmdline()
                .ok()
                .map_or(p.stat.comm, |cmdline| cmdline.join(" ")),
        };
        if p.stat.ppid == proc.stat.pid {
            children.push(pti);
        } else {
            map.insert(p.stat.pid, pti);
        }
    }

    let mut parents = Vec::new();
    let mut ppid = proc.stat.ppid;
    while let Some(parent) = map.remove(&ppid) {
        let new_parent = parent.ppid;
        parents.push(parent);
        if ppid == 1 {
            break;
        }
        ppid = new_parent;
    }
    parents.reverse();

    (parents, children)
}

pub(crate) fn limit_to_string(limit: &LimitValue) -> Cow<'static, str> {
    match limit {
        LimitValue::Unlimited => Cow::Borrowed("Unlimited"),
        LimitValue::Value(v) => Cow::Owned(format!("{}", v)),
    }
}

/// Given some text, and a width, try to figure out how many lines it needs
pub(crate) fn get_numlines<'t, I>(i: I, width: usize) -> usize
where
    I: Iterator<Item = &'t Text<'t>>,
{
    let mut cur_line_length = 0;
    let mut num_lines = 1;
    for item in i {
        let cow = match item {
            Text::Raw(cow) => cow,
            Text::Styled(cow, _) => cow,
        };

        // we assume that if there is a newline, it will only be at the *end*
        if cow.ends_with('\n') {
            cur_line_length += cow.len() - 1;
            num_lines += 1 + (cur_line_length / width as usize);
            cur_line_length = 0;
        } else {
            cur_line_length += cow.len();
        }
    }
    num_lines += cur_line_length / width as usize;

    num_lines
}

pub(crate) fn fmt_time(dt: chrono::DateTime<chrono::offset::Local>) -> impl Display {
    use chrono::offset::Local;
    let now = Local::now();

    if dt > now {
        // the date is in the future, so display the full thing: Jan 1 2019 15:44:15
        dt.format("%b %-d %Y %T")
    } else {
        let d = now - dt;
        if d < chrono::Duration::hours(12) {
            // just display the time
            dt.format("%T")
        } else if d < chrono::Duration::days(60) {
            // display month and day, but omit year
            dt.format("%b %-d %T")
        } else {
            dt.format("%b %-d %Y %T")
        }
    }
}

pub(crate) fn fmt_bytes(b: u64, suffix: &'static str) -> String {
    if b > 1000 * 1000 * 1000 {
        format!("{:.2} G{}", b as f64 / 1000.0 / 1000.0 / 1000.0, suffix)
    } else if b > 1000 * 1000 {
        format!("{:.2} M{}", b as f64 / 1000.0 / 1000.0, suffix)
    } else if b > 1000 {
        format!("{:.2} K{}", b as f64 / 1000.0, suffix)
    } else {
        format!("{} {}", b, suffix)
    }
}

pub(crate) fn fmt_rate(b: f32, suffix: &'static str) -> String {
    if b > 1000.0 * 1000.0 {
        format!("{:.1} M{}", b / 1000.0 / 1000.0, suffix)
    } else if b > 1000.0 {
        format!("{:.1} K{}", b / 1000.0, suffix)
    } else {
        format!("{:.1} {}", b, suffix)
    }
}

#[derive(Debug)]
pub(crate) enum Event {
    Key(Key),
    Mouse(MouseEvent),
    Tick,
}

pub(crate) struct Events {
    pub rx: mpsc::Receiver<Event>,
}

impl Events {
    pub fn new() -> Events {
        // spawn a thread to handle keyboard input
        let (tx, rx) = mpsc::channel();
        let kbd_tx = tx.clone();
        thread::spawn(move || {
            use termion::event::Event as TEvent;
            use termion::input::TermRead;
            let stdin = std::io::stdin();
            for evt in stdin.events() {
                if let Err(..) = match evt {
                    Err(..) => return,
                    Ok(TEvent::Key(k)) => kbd_tx.send(self::Event::Key(k)),
                    Ok(TEvent::Mouse(m)) => kbd_tx.send(self::Event::Mouse(m)),
                    Ok(TEvent::Unsupported(..)) => continue,
                } {
                    return;
                }
            }
        });

        thread::spawn(move || loop {
            thread::sleep(std::time::Duration::from_millis(1500));
            if let Err(..) = tx.send(self::Event::Tick) {
                return;
            }
        });

        Events { rx }
    }
}

pub(crate) fn lookup_username(uid: u32) -> String {
    use libc::{getpwuid_r, passwd, sysconf, _SC_GETPW_R_SIZE_MAX};
    use std::ffi::CStr;
    use std::mem::zeroed;

    let buf_size = match unsafe { sysconf(_SC_GETPW_R_SIZE_MAX) } {
        x if x <= 0 => {
            // make some something that we think will be big enough
            1024
        }
        x => x as usize,
    };

    let mut buf = vec![0; buf_size];
    let mut pwd: passwd = unsafe { zeroed() };

    let mut ptr = std::ptr::null_mut::<passwd>();

    if unsafe { getpwuid_r(uid, &mut pwd, buf.as_mut_ptr(), buf_size, &mut ptr) } == 0 {
        if !ptr.is_null() {
            let name = unsafe { CStr::from_ptr(pwd.pw_name) };
            return name.to_string_lossy().into_owned();
        }
    }

    "???".to_owned()
}

pub(crate) fn lookup_groupname(gid: u32) -> String {
    use libc::{getgrgid_r, group, sysconf, _SC_GETGR_R_SIZE_MAX};
    use std::ffi::CStr;
    use std::mem::zeroed;

    let buf_size = match unsafe { sysconf(_SC_GETGR_R_SIZE_MAX) } {
        x if x <= 0 => {
            // make some something that we think will be big enough
            1024
        }
        x => x as usize,
    };

    let mut buf = vec![0; buf_size];
    let mut pwd: group = unsafe { zeroed() };

    let mut ptr = std::ptr::null_mut::<group>();

    if unsafe { getgrgid_r(gid, &mut pwd, buf.as_mut_ptr(), buf_size, &mut ptr) } == 0 {
        if !ptr.is_null() {
            let name = unsafe { CStr::from_ptr(pwd.gr_name) };
            return name.to_string_lossy().into_owned();
        }
    }

    "???".to_owned()
}

pub(crate) fn get_pipe_pairs() -> HashMap<u32, (ProcTreeInfo, ProcTreeInfo)> {
    let mut read_map = HashMap::new();
    let mut write_map = HashMap::new();

    if let Ok(procs) = procfs::process::all_processes() {
        for proc in procs {
            if let Ok(fds) = proc.fd() {
                for fd in fds {
                    if let procfs::process::FDTarget::Pipe(uid) = fd.target {
                        let pti = ProcTreeInfo {
                            pid: proc.pid,
                            ppid: proc.stat.ppid,
                            cmdline: proc.stat.comm.clone(),
                        };
                        if fd.mode().contains(procfs::process::FDPermissions::READ) {
                            read_map.insert(uid, pti);
                        } else if fd.mode().contains(procfs::process::FDPermissions::WRITE) {
                            write_map.insert(uid, pti);
                        }
                    }
                }
            }
        }
    }

    let mut map = HashMap::new();
    for (uid, read_pti) in read_map.drain() {
        if let Some(write_pti) = write_map.remove(&uid) {
            map.insert(uid, (read_pti, write_pti));
        }
    }

    map
}

pub(crate) fn get_tcp_table() -> HashMap<u32, procfs::net::TcpNetEntry> {
    let mut map = HashMap::new();

    if let Ok(tcp) = procfs::net::tcp() {
        for entry in tcp {
            map.insert(entry.inode, entry);
        }
    }
    if let Ok(tcp) = procfs::net::tcp6() {
        for entry in tcp {
            map.insert(entry.inode, entry);
        }
    }

    map
}

pub(crate) fn get_udp_table() -> HashMap<u32, procfs::net::UdpNetEntry> {
    let mut map = HashMap::new();

    if let Ok(udp) = procfs::net::udp() {
        for entry in udp {
            map.insert(entry.inode, entry);
        }
    }
    if let Ok(udp) = procfs::net::udp6() {
        for entry in udp {
            map.insert(entry.inode, entry);
        }
    }

    map
}

pub(crate) fn get_unix_table() -> HashMap<u32, procfs::net::UnixNetEntry> {
    let mut map = HashMap::new();

    if let Ok(unix) = procfs::net::unix() {
        for entry in unix {
            map.insert(entry.inode, entry);
        }
    }

    map
}

#[cfg(test)]
mod tests {
    use tui::widgets::Text;

    #[test]
    fn test_boxsize() {
        let text = vec![Text::raw("hi\n"), Text::raw("hey")];

        let l = super::get_numlines(text.iter(), 5);
        assert_eq!(l, 2);
    }
}
