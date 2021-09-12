use crate::{
    cli,
    client::EXEC_GROUP_ID,
    error::{self, Error, PauseError},
    transport,
};

use std::net::{Ipv4Addr, SocketAddr};
use std::time::{Duration, Instant};

pub(crate) async fn pause(args: cli::Pause) -> Result<(), Error> {
    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, args.port));

    let duration = parse_time_input(&args.duration)?;

    // ensure that the specified duration is not longer than the max duration (4 hours)
    if duration > Duration::from_secs(60 * 60 * 4) {
        return Err(Error::from(PauseError::DurationTooLong));
    }

    // emulate a server connection here since the host client process
    // only expects messsages from a "server"
    let mut conn = transport::ServerConnection::new(addr).await?;

    //let request = transport::PauseExecution::new(duration);
    //let wrapped_request = transport::RequestFromServer::from(request);
    //conn.transport_data(&wrapped_request).await?;

    Ok(())
}

/// parse a duration string (like 1h30m10s) into a Duration
fn parse_time_input(mut input_str: &str) -> Result<Duration, error::PauseError> {
    let mut duration = Duration::from_secs(0);
    while input_str.len() > 0 {
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

pub(super) struct PauseProcessArbiter {
    unpause_instant: Option<Instant>,
    rx: std::sync::mpsc::Receiver<Option<Instant>>,
}

impl PauseProcessArbiter {
    /// Sending a None unpauses the execution
    /// Sending a Some(instant) will pause the underlying process until
    /// that instant
    pub(super) fn new() -> (Self, std::sync::mpsc::Sender<Option<Instant>>) {
        // we use std channels here because there is no easy way to check
        // if there is a value in the `Receiver` with tokio channels
        let (tx, rx) = std::sync::mpsc::channel();
        (
            Self {
                unpause_instant: None,
                rx,
            },
            tx,
        )
    }

    pub(super) fn spawn(mut self) -> tokio::task::JoinHandle<()> {
        tokio::task::spawn(async move {
            if let Ok(sleep_update) = self.rx.try_recv() {
                match sleep_update {
                    // request to set a pause time in the future, we pause now
                    Some(future_instant) => {
                        self.unpause_instant = Some(future_instant);
                        self.pause_execution();
                    }
                    // resume right away
                    None => {
                        self.unpause_execution();
                    }
                }
            }

            if let Some(instant) = self.unpause_instant {
                if Instant::now() > instant {
                    self.unpause_execution();
                    self.unpause_instant = None;
                }
            }

            tokio::time::sleep(Duration::from_secs(10)).await;
        })
    }

    // pause the execution of all processes using the specified groupid
    fn pause_execution(&self) {
        let signal = nix::sys::signal::Signal::SIGSTOP;
        let process_id = nix::unistd::Pid::from_raw(EXEC_GROUP_ID as i32);
        if let Err(e) = nix::sys::signal::kill(process_id, signal) {
            error!(
                "error when pausing group process (id {}): {}",
                EXEC_GROUP_ID, e
            );
        }
    }

    // pause the execution of all processes using the specified groupid
    fn unpause_execution(&self) {
        let signal = nix::sys::signal::Signal::SIGCONT;
        let process_id = nix::unistd::Pid::from_raw(EXEC_GROUP_ID as i32);
        if let Err(e) = nix::sys::signal::kill(process_id, signal) {
            error!(
                "error when resuming group process (id {}): {}",
                EXEC_GROUP_ID, e
            );
        }
    }
}

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
