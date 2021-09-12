use super::job_pool::JobResponse;
use crate::transport;
use derive_more::Constructor;

pub(crate) trait Schedule {
    fn fetch_new_task(&mut self, current_compiled_job: JobIdentifier) -> JobResponse;

    fn insert_new_batch(&mut self);

    fn add_job_back(&mut self, job: transport::Job, identifier: JobIdentifier);
}

#[derive(Clone)]
pub(crate) struct JobIdentifier {
    //
}

impl JobIdentifier {
    pub(crate) fn none() -> Self {
        Self {}
    }
}

#[derive(Constructor)]
pub(crate) struct GpuPriority {}

impl Schedule for GpuPriority {
    fn fetch_new_task(&mut self, current_compiled_job: JobIdentifier) -> JobResponse {
        todo!()
    }

    fn insert_new_batch(&mut self) {
        todo!()
    }

    fn add_job_back(&mut self, job: transport::Job, identifier: JobIdentifier) {
        todo!()
    }
}
