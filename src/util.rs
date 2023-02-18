use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt::Display;
use std::sync::mpsc;
use std::thread;

use crossterm::event::{KeyEvent, MouseEvent};
use procfs::{
    process::{all_processes, LimitValue, Process},
    ProcResult,
};
use tui::text::{Span, Spans};

#[derive(Debug)]
pub struct ProcessTreeEntry {
    pub pid: i32,
    pub ppid: i32,
    pub cmdline: String,
    pub num_siblings: u32,
    pub children: Vec<i32>,
}

#[derive(Debug)]
pub struct ProcessTree {
    pub entries: HashMap<i32, ProcessTreeEntry>,
}

impl ProcessTree {
    fn flatten_helper<'a>(
        map: &'a HashMap<i32, ProcessTreeEntry>,
        v: &mut Vec<(u8, &'a ProcessTreeEntry)>,
        pid: i32,
        depth: u8,
    ) {
        assert!(
            map.get(&pid).is_some(),
            "ProcessTree doesn't have an entry for pid {pid}"
        );
        let p = map.get(&pid).unwrap();

        v.push((depth, p));

        for cid in &p.children {
            if let Some(child) = map.get(cid) {
                Self::flatten_helper(map, v, child.pid, depth + 1);
            }
        }
    }

    pub fn flatten(&self) -> Vec<(u8, &ProcessTreeEntry)> {
        let mut v = Vec::with_capacity(self.entries.len());
        Self::flatten_helper(&self.entries, &mut v, 1, 1);

        v
    }
    pub(crate) fn new(focus: Option<(&[i32], &Process)>) -> Result<Self, anyhow::Error> {
        let all = all_processes()?;

        // map from pid to Process
        let mut procs: HashMap<i32, Process> = HashMap::new();

        // also construct a map that records all of the direct child processes
        let mut child_map: HashMap<i32, Vec<i32>> = HashMap::new();

        // map from pid to ProcessTreeEntry, which we'll return
        let mut map: HashMap<i32, ProcessTreeEntry> = HashMap::new();

        for proc in all.flatten() {
            let Ok(proc_stat) = proc.stat() else { continue };
            child_map.entry(proc_stat.ppid).or_default().push(proc.pid);
            procs.insert(proc.pid, proc);
        }

        let root_proc = procs.get(&1).unwrap();
        let mut root = ProcessTreeEntry {
            pid: root_proc.pid,
            ppid: 0,
            cmdline: root_proc
                .cmdline()
                .ok()
                .map_or(root_proc.stat()?.comm, |cmdline| cmdline.join(" ")),
            children: Vec::new(),
            num_siblings: 0,
        };
        build_entry(&mut root, &mut map, &procs, &child_map);
        map.insert(1, root);

        if let Some((parents, focus)) = focus {
            // it's possible that that `focus` isn't alive.  in that case, keep using the previous
            // set of pids_to_keep
            let mut pids_to_keep: Vec<i32> = Vec::from(parents);
            pids_to_keep.push(focus.pid);
            if let Some(child_pids) = child_map.get(&focus.pid) {
                pids_to_keep.extend(child_pids);
            }

            // starting at the focus, keep all parent pids
            let mut focus_pid = focus.pid;

            while let Some(entry) = procs.get(&focus_pid) {
                let proc_stat = entry.stat()?;
                pids_to_keep.push(proc_stat.ppid);
                focus_pid = proc_stat.ppid;
            }

            map.retain(|key, _entry| pids_to_keep.contains(key));
        }

        Ok(ProcessTree { entries: map })
    }
}

fn build_entry(
    entry: &mut ProcessTreeEntry,
    entries: &mut HashMap<i32, ProcessTreeEntry>,
    proc_map: &HashMap<i32, Process>,
    child_map: &HashMap<i32, Vec<i32>>,
) {
    if let Some(child_pids) = child_map.get(&entry.pid) {
        for child_pid in child_pids {
            let p = proc_map.get(child_pid).unwrap();
            let Ok(stat) = p.stat() else {
                continue;
            };
            let mut child_entry = ProcessTreeEntry {
                pid: *child_pid,
                ppid: entry.pid,
                cmdline: p.cmdline().ok().map_or(stat.comm.clone(), |cmdline| cmdline.join(" ")),
                children: Vec::new(),
                num_siblings: child_pids.len() as u32,
            };

            entry.children.push(*child_pid);
            build_entry(&mut child_entry, entries, proc_map, child_map);
            entries.insert(*child_pid, child_entry);
        }
    }
}

pub(crate) fn limit_to_string(limit: &LimitValue) -> Cow<'static, str> {
    match limit {
        LimitValue::Unlimited => Cow::Borrowed("Unlimited"),
        LimitValue::Value(v) => Cow::Owned(format!("{v}")),
    }
}

pub(crate) fn get_numlines_from_spans<'t, I>(spans: I, width: usize) -> usize
where
    I: Iterator<Item = &'t Spans<'t>>,
{
    let mut num_lines = 1;
    for span in spans {
        num_lines += 1 + (span.width() / width);
    }

    num_lines
}

/// Given some text, and a width, try to figure out how many lines it needs
pub(crate) fn get_numlines<'t, I>(i: I, width: usize) -> usize
where
    I: Iterator<Item = &'t Span<'t>>,
{
    let mut cur_line_length = 0;
    let mut num_lines = 1;
    for item in i {
        // we assume that if there is a newline, it will only be at the *end*
        if item.content.ends_with('\n') {
            cur_line_length += item.content.len() - 1;
            num_lines += 1 + (cur_line_length / width);
            cur_line_length = 0;
        } else {
            cur_line_length += item.content.len();
        }
    }
    num_lines += cur_line_length / width;

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
        format!("{:.2}\u{00A0}G{}", b as f64 / 1000.0 / 1000.0 / 1000.0, suffix)
    } else if b > 1000 * 1000 {
        format!("{:.2}\u{00A0}M{}", b as f64 / 1000.0 / 1000.0, suffix)
    } else if b > 1000 {
        format!("{:.2}\u{00A0}K{}", b as f64 / 1000.0, suffix)
    } else {
        format!("{b}\u{00A0}{suffix}")
    }
}

pub(crate) fn fmt_rate(b: f32, suffix: &'static str) -> String {
    if b > 1000.0 * 1000.0 {
        format!("{:.1}\u{00A0}M{}", b / 1000.0 / 1000.0, suffix)
    } else if b > 1000.0 {
        format!("{:.1}\u{00A0}K{}", b / 1000.0, suffix)
    } else {
        format!("{b:.1}\u{00A0}{suffix}")
    }
}

#[derive(Debug)]
pub(crate) enum Event {
    Key(KeyEvent),
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
        thread::Builder::new()
            .name("kbd-reader".to_owned())
            .spawn(move || {
                use crossterm::event::{read, Event};

                loop {
                    let evt = read();
                    if let Err(..) = match evt {
                        Err(..) => return,
                        Ok(Event::Key(e)) => kbd_tx.send(self::Event::Key(e)),
                        Ok(Event::Mouse(m)) => kbd_tx.send(self::Event::Mouse(m)),
                        _ => continue
                        // Ok(Event::Unsupported(bytes)) => match bytes.as_slice() {
                        //     // manual parsing of cursor movement keys in application mode
                        //     [0x1b, 79, 65] => kbd_tx.send(self::Event::Key(Key::Up)),
                        //     [0x1b, 79, 66] => kbd_tx.send(self::Event::Key(Key::Down)),
                        //     [0x1b, 79, 67] => kbd_tx.send(self::Event::Key(Key::Right)),
                        //     [0x1b, 79, 68] => kbd_tx.send(self::Event::Key(Key::Left)),
                        //     _ => continue,
                        // },
                    } {
                        return;
                    }
                }
            })
            .unwrap();

        thread::Builder::new()
            .name("tick".to_owned())
            .spawn(move || loop {
                thread::sleep(std::time::Duration::from_millis(1500));
                if let Err(..) = tx.send(self::Event::Tick) {
                    return;
                }
            })
            .unwrap();

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

    if unsafe { getpwuid_r(uid, &mut pwd, buf.as_mut_ptr(), buf_size, &mut ptr) } == 0 && !ptr.is_null() {
        let name = unsafe { CStr::from_ptr(pwd.pw_name) };
        return name.to_string_lossy().into_owned();
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

    if unsafe { getgrgid_r(gid, &mut pwd, buf.as_mut_ptr(), buf_size, &mut ptr) } == 0 && !ptr.is_null() {
        let name = unsafe { CStr::from_ptr(pwd.gr_name) };
        return name.to_string_lossy().into_owned();
    }

    "???".to_owned()
}

pub(crate) fn get_locks_for_pid(pid: i32) -> ProcResult<Vec<procfs::Lock>> {
    procfs::locks().map(|locks| {
        locks
            .into_iter()
            .filter(|lock| lock.pid == Some(pid))
            .collect::<Vec<_>>()
    })
}

pub(crate) fn get_pipe_pairs() -> HashMap<u64, (ProcessTreeEntry, ProcessTreeEntry)> {
    let mut read_map = HashMap::new();
    let mut write_map = HashMap::new();

    if let Ok(procs) = procfs::process::all_processes() {
        for proc in procs.filter_map(|p| p.ok()) {
            if let Ok(fds) = proc.fd() {
                let proc_stat = proc.stat().unwrap();
                for fd in fds.filter_map(|fd| fd.ok()) {
                    if let procfs::process::FDTarget::Pipe(uid) = fd.target {
                        let pti = ProcessTreeEntry {
                            pid: proc.pid,
                            ppid: proc_stat.ppid,
                            cmdline: proc_stat.comm.clone(),
                            children: Vec::new(),
                            num_siblings: 0,
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

pub(crate) fn get_tcp_table(p: &procfs::process::Process) -> HashMap<u64, procfs::net::TcpNetEntry> {
    let mut map = HashMap::new();

    if let Ok(tcp) = p.tcp() {
        for entry in tcp {
            map.insert(entry.inode, entry);
        }
    }
    if let Ok(tcp) = p.tcp6() {
        for entry in tcp {
            map.insert(entry.inode, entry);
        }
    }

    map
}

pub(crate) fn get_udp_table(p: &procfs::process::Process) -> HashMap<u64, procfs::net::UdpNetEntry> {
    let mut map = HashMap::new();

    if let Ok(udp) = p.udp() {
        for entry in udp {
            map.insert(entry.inode, entry);
        }
    }
    if let Ok(udp) = p.udp6() {
        for entry in udp {
            map.insert(entry.inode, entry);
        }
    }

    map
}

pub(crate) fn get_unix_table(p: &procfs::process::Process) -> HashMap<u64, procfs::net::UnixNetEntry> {
    let mut map = HashMap::new();

    if let Ok(unix) = p.unix() {
        for entry in unix {
            map.insert(entry.inode, entry);
        }
    }

    map
}

#[cfg(test)]
mod tests {
    use tui::text::Span;

    #[test]
    fn test_boxsize() {
        let text = vec![Span::raw("hi\n"), Span::raw("hey")];

        let l = super::get_numlines(text.iter(), 5);
        assert_eq!(l, 2);
    }

    #[test]
    fn test_proc_all_tree() {
        let tree = super::ProcessTree::new(None).unwrap();
        println!("{tree:#?}");
        //let me = procfs::process::Process::myself().unwrap();
        //let all = super::proc_all_tree(Some(&me)).unwrap();
        //println!("{:#?}", all);
    }
}
