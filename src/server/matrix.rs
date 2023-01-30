use crate::prelude::*;

pub(crate) fn send_matrix_message(
    matrix_id: matrix_notify::OwnedUserId,
    batch_name: String,
    reason: Reason,
    matrix_data: MatrixData,
) {
    // spawn off so that we can use async
    tokio::task::spawn(async move {
        let msg = reason.to_message(batch_name);

        if let Err(e) = matrix_notify::send_text_message(
            &matrix_data.client,
            msg,
            &matrix_id,
            &matrix_data.self_id,
        )
        .await
        {
            error!("failed to send message to user on matrix: {}", e);
        }
    });
}

pub(crate) enum Reason {
    FinishedAll,
    BuildFailures,
}

impl Reason {
    fn to_message(&self, job_name: String) -> String {
        match self {
            Self::FinishedAll => format!(
                "your distributed compute job {} has finished all its jobs",
                job_name
            ),
            Self::BuildFailures => format!(
                "your distributed compute job {} failed to build on every node and was cancelled",
                job_name
            ),
        }
    }
}

#[derive(Clone)]
pub(crate) struct MatrixData {
    client: Arc<matrix_notify::Client>,
    self_id: matrix_notify::OwnedUserId,
}

impl MatrixData {
    pub(crate) async fn from_config(
        config: matrix_notify::ConfigInfo,
    ) -> Result<Self, matrix_notify::Error> {
        let client = matrix_notify::client(&config).await?;
        let client = Arc::new(client);

        Ok(Self {
            client,
            self_id: config.matrix_id,
        })
    }
}
