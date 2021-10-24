use super::schedule::JobSet;
use std::convert::TryFrom;

pub(crate) fn send_matrix_message(matrix_id: matrix_notify::UserId, removed_set: JobSet, reason: Reason) {
    // spawn off so that we can use async
    tokio::task::spawn(async move {
        let client =
            match matrix_notify::client(&matrix_notify::ConfigInfo::new().unwrap())
                .await
            {
                Ok(c) => c,
                Err(e) => {
                    error!("failed to create matrix client: {}", e);
                    return;
                }
            };

        let self_id =
            matrix_notify::UserId::try_from("@compute-notify:matrix.org").unwrap();

        let msg = reason.to_message(removed_set.batch_name);

        if let Err(e) =
            matrix_notify::send_text_message(&client, msg, matrix_id, self_id).await
        {
            error!("failed to send message to user on matrix: {}", e);
        }
    });
}

pub(crate) enum Reason {
    FinishedAll,
    BuildFailures
}

impl Reason {
    fn to_message(&self, job_name: String) -> String {
        match self {
            &Self::FinishedAll => format!("your distributed compute job {} has finished all its jobs", job_name),
            &Self::BuildFailures=> format!("your distributed compute job {} failed to build on every node and was cancelled", job_name),
        }
    }
}
