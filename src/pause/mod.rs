mod unix;

use crate::{
    cli,
    error::{self, Error, PauseError},
};

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

pub async fn pause(args: cli::Pause) -> Result<(), Error> {
    let duration = parse_time_input(&args.duration)?;

    // ensure that the specified duration is not longer than the max duration (4 hours)
    if duration > Duration::from_secs(60 * 60 * 4) {
        return Err(Error::from(PauseError::DurationTooLong));
    }

    // set up a signal handler so we can do an early exit correctly if
    // the user sends SIGINT w/ Ctrl-C
    let term = Arc::new(AtomicBool::new(false));
    signal_hook::flag::register(signal_hook::consts::SIGINT, Arc::clone(&term))
        .map_err(error::UnixError::from)
        .map_err(error::PauseError::from)?;

    // these two commands are used for running the python
    // procs / apptainer containers
    let to_pause = ProcessSet::new(
        // `Apptainer runtime parent` is also possible here
        // these are really just pulled from htop
        &[
            "python3 run.py",
            "apptainer run --app distribute",
            "/bin/sh /scif/apps/distribute",
        ],
        duration,
    );

    let paused = to_pause.pause().map_err(PauseError::from)?;
    paused.sleep_to_instant(term);
    paused.resume().map_err(PauseError::from)?;

    Ok(())
}

/// parse a duration string (like 1h30m10s) into a Duration
fn parse_time_input(mut input_str: &str) -> Result<Duration, error::PauseError> {
    let mut duration = Duration::from_secs(0);

    // while the input is not empty...
    while !input_str.is_empty() {
        let (num, unit, remaining) = slice_until_unit(input_str)?;

        let seconds = match unit {
            'h' => num * 60 * 60,
            'm' => num * 60,
            's' => num,
            x => return Err(error::PauseError::InvalidCharacter(x)),
        };

        duration += Duration::from_secs(seconds);

        input_str = remaining;
    }

    Ok(duration)
}

/// parse a portion of a duration string (Ex: 1h) into the number (1), time control character (h)
/// and return any remaining characters in the input string
fn slice_until_unit(input_str: &str) -> Result<(u64, char, &str), error::PauseError> {
    // find the number of utf8 characters that are numbers at the start of the input
    let slice_length = input_str
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .fold(0, |acc, x| acc + x.len_utf8());

    // slice the numbers from the string and parse them
    let num_slice = input_str
        .get(0..slice_length)
        .ok_or(error::PauseError::BadStringSlice)?;
    let num = num_slice.parse().unwrap();

    // grab the time control character that appears directly after the number
    let next_char = input_str
        .chars()
        .nth(slice_length)
        .ok_or(error::PauseError::BadStringSlice)?;

    // fetch any remaining characters in the string
    let remaining = input_str.get(slice_length + 1..).unwrap_or("");

    Ok((num, next_char, remaining))
}

struct ProcessSet<STATE> {
    commands_to_pause: &'static [&'static str],
    duration: Duration,
    state: STATE,
}

impl ProcessSet<NotYetPaused> {
    fn new(commands: &'static [&'static str], duration: Duration) -> Self {
        Self {
            commands_to_pause: commands,
            duration,
            state: NotYetPaused,
        }
    }

    fn pause(self) -> Result<ProcessSet<Paused>, error::UnixError> {
        let root_procs = unix::scan_proc(self.commands_to_pause)?;
        let all_procs = unix::find_all_child_procs(root_procs);
        println!("pausing {} matching procs", all_procs.len());
        unix::pause_procs(&all_procs);

        Ok(ProcessSet {
            commands_to_pause: self.commands_to_pause,
            duration: self.duration,
            state: Paused { procs: all_procs },
        })
    }
}

impl ProcessSet<Paused> {
    fn sleep_to_instant(&self, term: Arc<AtomicBool>) {
        println!("sleeping until time");
        let mut now = Instant::now();
        let ending_instant = Instant::now() + self.duration;

        while !term.load(Ordering::Relaxed) && now < ending_instant {
            let dur = Duration::from_millis(100);
            std::thread::sleep(dur);
            now += dur;
        }
    }

    fn resume(self) -> Result<Resumed, error::UnixError> {
        println!("resuming");
        unix::resume_procs(&self.state.procs);

        Ok(Resumed {})
    }
}

struct NotYetPaused;

struct Paused {
    procs: Vec<unix::RunningProcess>,
}

struct Resumed;

#[test]
fn slice_check_1() {
    let input = "1h";
    let expected = (1, 'h', "");
    let out = slice_until_unit(input).unwrap();
    assert_eq!(expected, out);
}

#[test]
fn slice_check_2() {
    let input = "1h30m";
    let expected = (1, 'h', "30m");
    let out = slice_until_unit(input).unwrap();
    assert_eq!(expected, out);
}

#[test]
fn full_input_parse_1() {
    let input = "1h30m";
    let expected = Duration::from_secs(60 * 60 + 30 * 60);
    let out = parse_time_input(input).unwrap();
    assert_eq!(expected, out);
}

#[test]
fn full_input_parse_2() {
    let input = "1h";
    let expected = Duration::from_secs(60 * 60);
    let out = parse_time_input(input).unwrap();
    assert_eq!(expected, out);
}

#[test]
fn full_input_parse_3() {
    let input = "90m";
    let expected = Duration::from_secs(90 * 60);
    let out = parse_time_input(input).unwrap();
    assert_eq!(expected, out);
}

#[test]
fn full_input_parse_4() {
    let input = "3600s";
    let expected = Duration::from_secs(3600);
    let out = parse_time_input(input).unwrap();
    assert_eq!(expected, out);
}
