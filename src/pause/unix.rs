use nix::sys::signal;
use std::fs;
use std::path::Path;

//let procs = scan_proc("python3 ./script.py").unwrap();

//let active_procs = find_all_child_procs(procs);

//println!("number of active procs: {}", active_procs.len());
//println!("pausing all the matched procs");

type Error = crate::error::UnixError;

pub(crate) fn pause_procs(active_procs: &[RunningProcess]) {
    send_signal_to_procs(active_procs, signal::Signal::SIGSTOP);
}

pub(crate) fn resume_procs(paused_procs: &[RunningProcess]) {
    send_signal_to_procs(paused_procs, signal::Signal::SIGCONT);
}

/// send a chosen signal to a set of processes
fn send_signal_to_procs(procs_to_pause: &[RunningProcess], signal: signal::Signal) {
    for proc in procs_to_pause {
        let pid = nix::unistd::Pid::from_raw(proc.pid.try_into().unwrap());
        if let Err(e) = signal::kill(pid, signal) {
            println!(
                "Could not send signal to PID {}, are you root? (error: {}) - command: {}",
                proc.pid, e, proc.cmd
            );
        }
    }
}

#[derive(Debug)]
pub(crate) struct ProcessGroups {
    parent_procs: Vec<RunningProcess>,
    other_procs: Vec<RunningProcess>,
}

#[derive(Debug, Eq)]
pub(crate) struct RunningProcess {
    pid: u32,
    cmd: String,
    ppid: u32,
}

impl std::cmp::PartialEq for RunningProcess {
    fn eq(&self, other: &Self) -> bool {
        self.pid == other.pid
    }
}

impl PartialOrd for RunningProcess {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.pid.cmp(&other.pid))
    }
}

/// iterate through all the processes in `/proc` and
/// return a `ProcessGroup` that represents the input `commands`
pub(crate) fn scan_proc(commands: &[&str]) -> Result<ProcessGroups, Error> {
    let procs = fs::read_dir("/proc")
        .unwrap()
        .filter_map(|x| x.ok())
        .filter_map(|x| {
            if x.file_type().unwrap().is_dir() {
                Some(x.path())
            } else {
                None
            }
        });

    let mut parent_procs = Vec::new();
    let mut other_procs = Vec::new();

    for path in procs {
        let filename = path.file_name().expect("missing file name for path");

        if let Ok(pid) = filename.to_string_lossy().parse::<u32>() {
            // based on the path to the procses
            match process_from_path(&path, pid) {
                Ok(proc) => {
                    let mut found_command = false;

                    for command in commands {
                        // if the process command is the same string that we are looking
                        // for then this process is a parent process that we need to store
                        if proc.cmd.contains(command) {
                            found_command = true;
                            break;
                        }
                    }

                    // we have to have a separate statement here
                    // for borrow checker rules
                    if found_command {
                        parent_procs.push(proc);
                    } else {
                        other_procs.push(proc)
                    }
                }
                Err(_e) => {
                    // the process was destroyed - we cant do anything about it
                }
            }
        }
    }

    let groups = ProcessGroups {
        parent_procs,
        other_procs,
    };

    Ok(groups)
}

/// wrapper function to pull all the information from a procses at a given /proc/{pid}
/// to a `RunningProcess` struct
fn process_from_path(path: &Path, pid: u32) -> Result<RunningProcess, Error> {
    // find the command that started this process
    let cmd = path.join("cmdline");
    let out = fs::read(cmd)?;
    let cmd = remove_null_characters(out);

    // find the ppid of the process
    let stat_file = path.join("stat");
    let out = fs::read(stat_file)?;
    let stat_string = remove_null_characters(out);
    let ppid = parse_ppid_from_stat(&stat_string);

    Ok(RunningProcess { pid, cmd, ppid })
}

/// iterates through the parent process(es)
/// and checks if any of the processes in `other_procs` have a parent
/// id equal to the process ID of one of the parents. If it does then add it
/// to the list of parent processes so that we can pause them all together
pub(crate) fn find_all_child_procs(procs: ProcessGroups) -> Vec<RunningProcess> {
    let ProcessGroups {
        mut parent_procs,
        other_procs,
    } = procs;

    for proc in other_procs {
        if check_parent_pid(proc.ppid, &parent_procs) {
            parent_procs.push(proc)
        }
    }

    parent_procs
}

/// iterate through a vector and check if there are any process
/// with a PID that match a given processes' PPID
fn check_parent_pid(ppid: u32, parent_procs: &[RunningProcess]) -> bool {
    parent_procs.iter().any(|x| x.pid == ppid)
}

/// Pull the parent PID from a process
///
/// from `man 5 proc`, the PPID is stored in the 4th position
/// the second position is enclosed in ()
fn parse_ppid_from_stat(stat: &str) -> u32 {
    stat.chars()
        // makes the assumption that there are no nested parenthesis 
        // in the second argument
        .skip_while(|c| *c != ')')
        // the third argument is a letter, to skip everything until
        // we hit a number
        .skip_while(|c| !c.is_ascii_digit())
        // and then take everything until we no longer have digits
        .take_while(|c| c.is_ascii_digit())
        .collect::<String>()
        .parse()
        .unwrap_or_else(|_| panic!("4th argument of /proc/pid/stat was not a number, or we have a bad bad parsing case. Full stat line: {}", stat))
}

/// removes the null characters in `/proc/{pid}/` files and replaces them
/// with spaces so that we can parse them
fn remove_null_characters(mut bytes: Vec<u8>) -> String {
    bytes.iter_mut().for_each(|x| {
        if *x == 0 {
            *x = 32
        }
    });

    String::from_utf8(bytes).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stat_1() {
        let text = "4086716 (Web Content) S 424619 424619 721581 34822 424619 4194560 269321 0 17250 0 1628 397 0 0 20 0 20 0 80335308 2691227648 39388 18446744073709551615 94349686466720 94349686871552 140726211757264 0 0 0 0 69634 1082133752 0 0 0 17 0 0 0 15 0 0 94349686883616 94349686883712 94349717884928 140726211762837 140726211763031 140726211763031 140726211768287 0";
        let out = parse_ppid_from_stat(text);
        let exp = 424619;
        assert_eq!(out, exp)
    }

    #[test]
    fn stat_2() {
        let text = "4154863 (fish) S 449 4154863 4154863 34845 4154863 4194304 3472 10692 15 248 11 1 31 58 20 0 1 0 80749771 240672768 1391 18446744073709551615 93880539279360 93880541275320 140722853215552 0 0 0 0 2625540 135356435 0 0 0 17 3 0 0 1 0 0 93880541280896 93880541306984 93880573100032 140722853217448 140722853217453 140722853217453 140722853220330 0 ";
        let out = parse_ppid_from_stat(text);
        let exp = 449;
        assert_eq!(out, exp)
    }

    #[test]
    fn stat_3() {
        let text = "3109616 (tmux: client) S 3109150 3109616 3109150 34835 3109616 4194304 257 0 8 0 0 0 0 0 20 0 1 0 74065439 9003008 765 18446744073709551615 94465777778688 94465778344549 140735058596688 0 0 0 0 3674116 134433283 0 0 0 17 2 0 0 1 0 0 94465778553392 94465778624968 94465805963264 140735058605564 140735058605573 140735058605573 140735058608106 0 ";
        let out = parse_ppid_from_stat(text);
        let exp = 3109150;
        assert_eq!(out, exp)
    }

    #[test]
    fn stat_4() {
        let text = "1994701 (UVM deferred re) S 2 0 0 0 -1 2129984 0 0 0 0 0 0 0 0 20 0 1 0 12935285 0 0 18446744073709551615 0 0 0 0 0 0 0 2147483647 0 0 0 0 17 3 0 0 0 0 0 0 0 0 0 0 0 0 0";
        let out = parse_ppid_from_stat(text);
        let exp = 2;
        assert_eq!(out, exp)
    }

    #[test]
    fn parent_id_exists() {
        let parents = &[RunningProcess {
            pid: 1,
            ppid: 0,
            cmd: "".to_string(),
        }];

        let ppid = 1;
        let out = check_parent_pid(ppid, parents);

        assert_eq!(out, true);
    }
}
