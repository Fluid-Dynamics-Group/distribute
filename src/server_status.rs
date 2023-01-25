use crate::{
    cli,
    error::{self, Error},
    prelude::*,
    transport,
};

use std::net::SocketAddr;
use std::time::Duration;

/// check that all the nodes are up *and* the versions match. returns `true` if all nodes are
/// healthy w/ version matches
pub async fn server_status(args: cli::ServerStatus) -> Result<(), Error> {
    let job_list = get_current_jobs(&args).await?;

    for batch in job_list {
        println!("{}", batch.batch_name);

        for job in batch.jobs_left {
            println!("\t-{}", job);
        }

        println!("\t:jobs running now:");

        for job in batch.running_jobs {
            let runtime = HourMinuteSecond::from(job.duration);

            println!("\t\t{} ({}): {runtime}", job.job_name, job.node_meta);
        }
    }

    Ok(())
}

#[derive(Display, PartialEq, Eq, Debug)]
#[display(fmt = "{hours:02}h:{minutes:02}m:{seconds:02}s")]
struct HourMinuteSecond {
    hours: u64,
    minutes: u64,
    seconds: u64,
}

impl From<Duration> for HourMinuteSecond {
    fn from(dur: Duration) -> Self {
        let seconds = dur.as_secs();
        let minutes = seconds / 60;
        let hours = minutes / 60;

        let minutes = if hours == 0 {
            minutes
        } else {
            minutes % (hours * 60)
        };

        let seconds_above_60 = ((hours * 60) + minutes) * 60;

        let seconds = if seconds_above_60 == 0 {
            seconds
        } else {
            seconds % seconds_above_60
        };

        Self {
            hours,
            minutes,
            seconds,
        }
    }
}

pub async fn get_current_jobs(
    args: &cli::ServerStatus,
) -> Result<Vec<crate::server::RemainingJobs>, Error> {
    let addr = SocketAddr::from((args.ip, args.port));

    let mut conn = transport::Connection::new(addr).await?;

    conn.transport_data(&transport::UserMessageToServer::QueryJobNames)
        .await?;

    match conn.receive_data().await {
        Ok(transport::ServerResponseToUser::JobNames(x)) => Ok(x),
        Ok(x) => Err(Error::from(error::StatusError::NotQueryJobs(x))),
        Err(e) => Err(e).map_err(Error::from),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hour_minute_zero() {
        let hours = 0;
        let minutes = 0;
        let seconds = 10;

        let total_seconds = seconds + (60 * (minutes + (60 * hours)));
        let duration = Duration::from_secs(total_seconds);

        let expected = HourMinuteSecond {
            hours,
            minutes,
            seconds,
        };
        let actual = HourMinuteSecond::from(duration);

        assert_eq!(expected, actual);
    }

    #[test]
    fn hour_zero() {
        let hours = 0;
        let minutes = 5;
        let seconds = 10;

        let total_seconds = seconds + (60 * (minutes + (60 * hours)));
        let duration = Duration::from_secs(total_seconds);

        let expected = HourMinuteSecond {
            hours,
            minutes,
            seconds,
        };
        let actual = HourMinuteSecond::from(duration);

        assert_eq!(expected, actual);
    }

    #[test]
    fn minute_zero() {
        let hours = 1;
        let minutes = 0;
        let seconds = 10;

        let total_seconds = seconds + (60 * (minutes + (60 * hours)));
        let duration = Duration::from_secs(total_seconds);

        let expected = HourMinuteSecond {
            hours,
            minutes,
            seconds,
        };
        let actual = HourMinuteSecond::from(duration);

        assert_eq!(expected, actual);
    }

    #[test]
    fn second_zero() {
        let hours = 1;
        let minutes = 5;
        let seconds = 0;

        let total_seconds = seconds + (60 * (minutes + (60 * hours)));
        let duration = Duration::from_secs(total_seconds);

        let expected = HourMinuteSecond {
            hours,
            minutes,
            seconds,
        };
        let actual = HourMinuteSecond::from(duration);

        assert_eq!(expected, actual);
    }

    #[test]
    fn all_nonzero() {
        let hours = 1;
        let minutes = 20;
        let seconds = 5;

        let total_seconds = seconds + (60 * (minutes + (60 * hours)));
        let duration = Duration::from_secs(total_seconds);

        let expected = HourMinuteSecond {
            hours,
            minutes,
            seconds,
        };
        let actual = HourMinuteSecond::from(duration);

        assert_eq!(expected, actual);
        let fmt = format!("{actual}");
        assert_eq!("01h:20m:05s", fmt);
    }
}
