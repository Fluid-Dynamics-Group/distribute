use super::matrix;
use super::pool_data::{JobOrInit, JobResponse, NodeMetadata, TaskInfo};

use crate::config::{self, requirements};
use crate::error::{self, ScheduleError};

use crate::prelude::*;

use std::collections::{BTreeMap, BTreeSet};

use std::sync::Arc;

pub(crate) trait Schedule {
    fn fetch_new_task(
        &mut self,
        current_compiled_job: JobSetIdentifier,
        node_caps: Arc<requirements::Requirements<requirements::NodeProvidedCaps>>,
        build_failures: &BTreeSet<JobSetIdentifier>,
        node_meta: NodeMetadata,
    ) -> JobResponse;

    fn insert_new_batch(
        &mut self,
        jobs: config::Jobs<config::common::HashedFile>,
    ) -> Result<(), ScheduleError>;

    fn add_job_back(&mut self, job: config::Job, identifier: JobSetIdentifier);

    fn finish_job(&mut self, job: JobSetIdentifier, job_name: &str);

    /// fetch all of the batch names and associated jobs that are still running
    fn remaining_jobs(&self) -> Vec<RemainingJobs>;

    /// find a job identifier associated with the name of a certain job
    fn identifier_by_name(&self, batch_name: &str) -> Option<JobSetIdentifier>;

    fn mark_build_failure(&mut self, failed_ident: JobSetIdentifier, total_nodes: usize);

    fn cancel_batch(&mut self, ident: JobSetIdentifier);
}

#[derive(Clone, Ord, PartialEq, Eq, PartialOrd, Copy, Display, Debug, Serialize, Deserialize)]
/// Identify a batch of jobs
pub(crate) enum JobSetIdentifier {
    #[display(fmt = "{}", _0)]
    Identity(u64),
    #[display(fmt = "Empty")]
    None,
}

impl JobSetIdentifier {
    pub(crate) fn none() -> Self {
        Self::None
    }

    fn new(ident: u64) -> Self {
        JobSetIdentifier::Identity(ident)
    }
}

#[derive(Constructor)]
pub(crate) struct GpuPriority {
    map: BTreeMap<JobSetIdentifier, JobSet>,
    last_identifier: u64,
    // location where temp files are stored before they are sent off to scheduled
    // jobs
    base_path: PathBuf,
    matrix: Option<matrix::MatrixData>,
}

impl GpuPriority {
    /// grab the first job that we have available. It is assumed that this function is only called
    /// if we have ensured that the current jobidentifier has no remaining jobs left
    ///
    /// therefore, this function only returns a job initialization task or empty jobs
    fn take_first_job(
        &mut self,
        node_caps: Arc<requirements::Requirements<requirements::NodeProvidedCaps>>,
        build_failures: &BTreeSet<JobSetIdentifier>,
    ) -> JobResponse {
        if let Some((ident, job_set)) = self
            .map
            .iter_mut()
            .filter(|(_ident, job_set)| job_set.has_remaining_jobs())
            // make sure the capabilities of this node match the total capabilities
            .filter(|(_ident, job_set)| node_caps.can_accept_job(&job_set.requirements))
            // make sure that the job we are pulling has not failed to build on this node before
            .find(|(ident, _job_set)| !build_failures.contains(ident))
        {
            let init = job_set.init_file();

            JobResponse::SetupOrRun(TaskInfo::new(
                job_set.namespace.clone(),
                job_set.batch_name.clone(),
                *ident,
                JobOrInit::JobInit(init),
            ))
        } else {
            JobResponse::EmptyJobs
        }
    }

    /// try to fetch a job from  the currently compiled proc
    /// if there are no jobs available for this process then we return None
    /// and allow the calling function to figure out how to find the next process
    fn job_by_id(
        &mut self,
        identifier: JobSetIdentifier,
        build_failures: &BTreeSet<JobSetIdentifier>,
        node_meta: &NodeMetadata,
    ) -> Option<JobResponse> {
        // make sure that any job we pull in this function
        // is actually buildable by the node requesting it
        if build_failures.contains(&identifier) {
            return None;
        }

        self.map.get_mut(&identifier).and_then(|job_set| {
            // check if the process with the current id has any jobs remaining
            // if it has nothing return None instead of JobResponse::EmptyJobs
            if job_set.has_remaining_jobs() {
                // TODO: this can panic
                //
                debug!(
                    "fetching next job since there are jobs remaining in set {}",
                    identifier
                );
                let job = job_set.next_job(node_meta).unwrap();
                Some(JobResponse::SetupOrRun(TaskInfo::new(
                    job_set.namespace.clone(),
                    job_set.batch_name.clone(),
                    identifier,
                    JobOrInit::Job(job),
                )))
            } else {
                None
            }
        })
    }
}

impl Schedule for GpuPriority {
    fn fetch_new_task(
        &mut self,
        current_compiled_job: JobSetIdentifier,
        node_caps: Arc<requirements::Requirements<requirements::NodeProvidedCaps>>,
        build_failures: &BTreeSet<JobSetIdentifier>,
        node_meta: NodeMetadata,
    ) -> JobResponse {
        // go through our entire job set and see if there is a gpu job
        if let Some((gpu_ident, gpu_job_set)) = self
            .map
            .iter_mut()
            // fetch the first job that requires a gpu
            .filter(|(_ident, job_set)| job_set.requirements.requires_gpu())
            // make sure that this job set actually has some jobs left
            .filter(|(_ident, job_set)| job_set.has_remaining_jobs())
            // make sure the capabilities of this node can accept this job
            .filter(|(_ident, job_set)| node_caps.can_accept_job(&job_set.requirements))
            // make sure that the job we are pulling has not failed to build on this node before
            .find(|(ident, _job_set)| !build_failures.contains(ident))
        {
            // if we are already working on this job - we dont need to recompile anything,
            // just send the next iteration of the job out
            if current_compiled_job == *gpu_ident {
                let job = gpu_job_set.next_job(&node_meta).unwrap();
                JobResponse::SetupOrRun(TaskInfo::new(
                    gpu_job_set.namespace.clone(),
                    gpu_job_set.batch_name.clone(),
                    current_compiled_job,
                    JobOrInit::Job(job),
                ))
            } else {
                let init = gpu_job_set.init_file();
                JobResponse::SetupOrRun(TaskInfo::new(
                    gpu_job_set.namespace.clone(),
                    gpu_job_set.batch_name.clone(),
                    *gpu_ident,
                    JobOrInit::JobInit(init),
                ))
            }
        }
        // we dont have any gpu jobs to run, just pull the first item from the map that has some
        // jobs left and we will run those
        else if let Some(job) = self.job_by_id(current_compiled_job, build_failures, &node_meta) {
            job
        } else {
            self.take_first_job(node_caps, build_failures)
        }
    }

    fn insert_new_batch(
        &mut self,
        jobs: config::Jobs<config::common::HashedFile>,
    ) -> Result<(), ScheduleError> {
        let jobs = JobSet::from_config(jobs).map_err(error::StoreSet::from)?;

        self.last_identifier += 1;
        let ident = JobSetIdentifier::new(self.last_identifier);
        self.map.insert(ident, jobs);

        Ok(())
    }

    fn add_job_back(&mut self, job: config::Job, identifier: JobSetIdentifier) {
        if let Some(job_set) = self.map.get_mut(&identifier) {
            job_set.add_errored_job(job)
        } else {
            error!(
                "job set for job {} was removed from the tree when there was still 
                a job running - this job can no longer be run. This should not happen",
                job.name()
            );
        }
    }

    fn finish_job(&mut self, identifier: JobSetIdentifier, job_name: &str) {
        debug!("called finish job with ident {}", identifier);

        if let Some(job_set) = self.map.get_mut(&identifier) {
            job_set.job_finished(job_name);

            // if there are no more jobs to run, and the final job for this process
            // has just finished - remove the job set from the tree
            if !job_set.has_remaining_jobs() && job_set.running_jobs.is_empty() {
                info!(
                    "removing job set with identifier {} from the queue - all jobs have finished 
                    and there are no more running jobs",
                    identifier
                );

                let removed_set = self.map.remove(&identifier).unwrap();

                // since we are done with this job set then we should deallocate the build file
                removed_set.build.delete_files().ok();

                if let Some(matrix_id) = &removed_set.matrix_user {
                    if let Some(matrix) = &self.matrix {
                        let matrix_id = matrix_id.clone();

                        info!(
                            "sending message to matrix user {} for batch finish `{}`",
                            matrix_id, removed_set.batch_name
                        );

                        // send the matrix message
                        super::matrix::send_matrix_message(
                            matrix_id,
                            removed_set.batch_name,
                            super::matrix::Reason::FinishedAll,
                            matrix.clone(),
                        )
                    }
                }
            }
        } else {
            error!(
                "a job set for identifier {} was removed before all 
               of the running jobs for the set were done. This should not happen",
                &identifier
            )
        }
    }

    fn remaining_jobs(&self) -> Vec<RemainingJobs> {
        self.map
            .values()
            .map(|job_set| job_set.remaining_jobs())
            .collect()
    }

    fn identifier_by_name(&self, batch_name: &str) -> Option<JobSetIdentifier> {
        self.map
            .iter()
            .filter(|(_, set)| set.batch_name == batch_name)
            .map(|(identifier, _)| *identifier)
            .next()
    }

    fn mark_build_failure(&mut self, failed_ident: JobSetIdentifier, total_nodes: usize) {
        if let Some((_ident, mut job_set)) = self
            .map
            .iter_mut()
            .find(|(identifier, _)| **identifier == failed_ident)
        {
            job_set.build_failures += 1;

            // if we have failed to build on every since node
            if job_set.build_failures == total_nodes {
                let removed_set = self.map.remove(&failed_ident).unwrap();

                info!(
                    "removing job set {} since it failed to build on every node",
                    removed_set.batch_name
                );

                if let Some(matrix_id) = &removed_set.matrix_user {
                    if let Some(matrix) = &self.matrix {
                        let matrix_id = matrix_id.clone();

                        info!(
                            "sending message to matrix user {} for finish to `{}`",
                            matrix_id, removed_set.batch_name
                        );

                        super::matrix::send_matrix_message(
                            matrix_id,
                            removed_set.batch_name,
                            super::matrix::Reason::BuildFailures,
                            matrix.clone(),
                        )
                    }
                }
            }
        } else {
            warn!("Failed to mark job set {} as failing to build for a node since it could not be found in the job set list. This should not happen", failed_ident);
        }
    }

    /// Remove any remaining jobs in the queue for a job set
    ///
    /// Since nodes are expected to report an end to a job in the same manner that they
    /// end jobs normally, they are expected to report back with their own `.finish_job()` calls,
    /// so the overall job set must remain in the pool.
    ///
    /// Once all cancellations have been filed the job set will be dropped in another function call
    fn cancel_batch(&mut self, ident: JobSetIdentifier) {
        info!("cancelling batch for identifier {}", ident);

        if let Some(job_set) = self.map.get_mut(&ident) {
            info!(
                "cancellation for identifier {ident} corresponds to batch name {}",
                job_set.batch_name
            );

            if let Err(e) = job_set.clear_remaining_jobs() {
                error!(
                    "failed to clear some of the remaining jobs after cancellation: {}",
                    e
                )
            }
        } else {
            warn!(
                "could not find batch with the identifier {} in cancellation request",
                ident
            );
        }
    }
}

#[derive(Debug)]
#[cfg_attr(test, derive(Clone))]
pub(crate) struct JobSet {
    build: config::Init,
    requirements: requirements::Requirements<requirements::JobRequiredCaps>,
    remaining_jobs: Vec<config::Job>,
    running_jobs: Vec<RunningJob>,
    pub(crate) batch_name: String,
    namespace: String,
    matrix_user: Option<matrix_notify::OwnedUserId>,
    build_failures: usize,
}

impl JobSet {
    fn has_remaining_jobs(&self) -> bool {
        !self.remaining_jobs.is_empty()
    }

    fn next_job(&mut self, node_meta: &NodeMetadata) -> Option<config::Job> {
        if let Some(job) = self.remaining_jobs.pop() {
            let running_job_desc = RunningJob::new(job.name().to_string(), node_meta.clone());
            self.running_jobs.push(running_job_desc);

            debug!(
                "calling next_job() -> currently running jobs is now length {}",
                self.running_jobs.len()
            );

            Some(job)
        } else {
            None
        }
    }

    fn init_file(&mut self) -> config::Init {
        self.build.clone()
    }

    fn add_errored_job(&mut self, job: config::Job) {
        debug!("called add_errored_job() on corresponding job set, removing job name {} from running set list", job.name());
        self.remove_running_job_by_name(job.name());
        self.remaining_jobs.push(job)
    }

    fn remove_running_job_by_name(&mut self, job_name: &str) {
        if let Some(idx) = self
            .running_jobs
            .iter()
            .enumerate()
            .filter(|(_, running_job)| running_job.job_name == job_name)
            .map(|(idx, _)| idx)
            .next()
        {
            self.running_jobs.remove(idx);
            debug!(
                "calling removing job of name {job_name} from current queue -> currently running jobs is now {}",
                self.running_jobs.len()
            );
        } else {
            error!("failed to find job name {} in the set of running jobs for this node. Running job names are currently: {:?}", 
                job_name, self.running_jobs.iter().map(|job| &job.job_name).collect::<Vec<_>>());
        }
    }

    fn job_finished(&mut self, job_name: &str) {
        debug!("called job_finished() on corresponding job set, removing job name {job_name} from running set list");
        self.remove_running_job_by_name(job_name);
    }

    pub(crate) fn name(&self) -> &str {
        &self.batch_name
    }

    fn remaining_jobs(&self) -> RemainingJobs {
        RemainingJobs {
            batch_name: self.batch_name.clone(),
            jobs_left: self
                .remaining_jobs
                .iter()
                .map(|x| x.name().to_string())
                .collect(),
            running_jobs: self
                .running_jobs
                .iter()
                .map(|x| x.to_duration_information())
                .collect(),
        }
    }

    fn clear_remaining_jobs(&mut self) -> Result<(), std::io::Error> {
        for job in self.remaining_jobs.drain(..) {
            // loading the job deletes the previous files after reading it into memory
            // TODO: impl Drop destructors for Lazy_ files so that we can just drop the whole
            // thing here and be sure that it will always run
            //
            // also - there is no reason to load this data into memory if we are just throwing it
            // out
            job.delete_files()?;
        }

        debug!(
            "length of remaining jobs after clearing them all with drain: len: {}, cap: {}",
            self.remaining_jobs.len(),
            self.remaining_jobs.capacity()
        );

        Ok(())
    }

    pub(crate) fn from_config(
        config: config::Jobs<config::common::HashedFile>,
    ) -> Result<Self, std::io::Error> {
        let namespace = config.namespace();
        let batch_name = config.batch_name();
        let requirements = config.capabilities().clone();
        let matrix_user = config.matrix_user();

        let build = config::Init::from(&config);

        let remaining_jobs = match config {
            config::Jobs::Python(python) => python
                .description()
                .jobs()
                .into_iter()
                .map(Clone::clone)
                .map(config::Job::from)
                .collect::<Vec<_>>(),
            config::Jobs::Apptainer(apptainer) => apptainer
                .description()
                .jobs()
                .into_iter()
                .map(Clone::clone)
                .map(config::Job::from)
                .collect::<Vec<_>>(),
        };

        Ok(Self {
            build,
            requirements,
            remaining_jobs,
            running_jobs: vec![],
            batch_name,
            matrix_user,
            namespace,
            build_failures: 0,
        })
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct RemainingJobs {
    pub batch_name: String,
    pub jobs_left: Vec<String>,
    pub running_jobs: Vec<RunningJobDuration>,
}

#[derive(Clone, Debug)]
pub struct RunningJob {
    job_name: String,
    /// information on the node we are running on
    node_meta: NodeMetadata,
    start_time: std::time::Instant,
}

impl RunningJob {
    fn new(job_name: String, node_meta: NodeMetadata) -> Self {
        Self {
            job_name,
            node_meta,
            start_time: std::time::Instant::now(),
        }
    }

    fn to_duration_information(&self) -> RunningJobDuration {
        let Self {
            job_name,
            node_meta,
            start_time,
        } = self.clone();
        RunningJobDuration {
            job_name,
            node_meta,
            duration: start_time.elapsed(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct RunningJobDuration {
    pub(crate) job_name: String,
    /// information on the node we are running on
    pub(crate) node_meta: NodeMetadata,
    pub(crate) duration: std::time::Duration,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::requirements::Requirements;

    use config::common::{File, HashedFile};
    use config::Jobs;

    fn check_init(
        current_ident: &mut JobSetIdentifier,
        expected_ident: u64,
        response: JobResponse,
    ) {
        match response {
            JobResponse::SetupOrRun(task) => {
                task.task.unwrap_job_init();
                assert_eq!(task.identifier, JobSetIdentifier::Identity(expected_ident));
                *current_ident = task.identifier;
            }
            JobResponse::EmptyJobs => {
                dbg!(&response);
                panic!("empty jobs returned when all jobs still present")
            }
        }
    }

    fn check_job(response: JobResponse, expected_job: config::python::Job<HashedFile>) {
        match response {
            JobResponse::SetupOrRun(task) => {
                assert_eq!(task.task.unwrap_job().name(), expected_job.name());
            }
            JobResponse::EmptyJobs => panic!("empty jobs returned when all jobs still present"),
        }
    }

    fn check_empty(response: JobResponse) {
        match response {
            JobResponse::EmptyJobs => (),
            _ => panic!("expected empty jobs for response"),
        }
    }

    fn init(path: &Path) -> (Jobs<HashedFile>, Jobs<HashedFile>, GpuPriority) {
        let scheduler = GpuPriority::new(
            Default::default(),
            Default::default(),
            path.to_owned(),
            None,
        );

        let jgpu = config::python::Job::new("jgpu".into(), File::new_relative("some_path"), vec![]);
        let j1 = config::python::Job::new("j1".into(), File::new_relative("some_path"), vec![]);
        let j2 = config::python::Job::new("j2".into(), File::new_relative("some_path"), vec![]);

        let build = config::python::Initialize::new(
            config::common::File::new_relative("build_file"),
            vec![],
        );

        let cpu_caps = vec!["fortran".into(), "fftw".into()].into_iter().collect();
        let gpu_caps = vec!["gpu".into()].into_iter().collect();

        let cpu_meta = config::Meta::new("cpu_jobs".into(), "namespace".into(), None, cpu_caps);
        let gpu_meta = config::Meta::new("gpu_jobs".into(), "namespace".into(), None, gpu_caps);

        let cpu_description = config::python::Description::new(build.clone(), vec![j1, j2]);
        let gpu_description = config::python::Description::new(build.clone(), vec![jgpu]);

        let cpu_config = config::Jobs::from(config::PythonConfig::new(cpu_meta, cpu_description))
            .hashed()
            .unwrap()
            .into();

        let gpu_config = config::Jobs::from(config::PythonConfig::new(gpu_meta, gpu_description))
            .hashed()
            .unwrap()
            .into();

        (cpu_config, gpu_config, scheduler)
    }

    #[test]
    /// put both gpu and cpu sets in the queue and expect the gpu sets to be received first
    fn gpu_first() {
        let path = Path::new("gpu_first");
        std::fs::create_dir_all(path).unwrap();
        let node_meta = NodeMetadata::test_name();

        let (cpu_set, gpu_set, mut scheduler) = init(path);

        let jgpu = gpu_set.clone().unwrap_python().description().jobs()[0].clone();
        let j1 = cpu_set.clone().unwrap_python().description().jobs()[0].clone();
        let j2 = cpu_set.clone().unwrap_python().description().jobs()[1].clone();

        scheduler.insert_new_batch(cpu_set).unwrap();
        scheduler.insert_new_batch(gpu_set).unwrap();

        let caps = Arc::new(Requirements {
            reqs: vec!["fortran".into(), "fftw".into(), "gpu".into()]
                .into_iter()
                .collect(),
            marker: std::marker::PhantomData,
        });

        let mut current_ident = JobSetIdentifier::none();

        let build_failures = Default::default();

        let job = scheduler.fetch_new_task(
            current_ident,
            caps.clone(),
            &build_failures,
            node_meta.clone(),
        );
        check_init(&mut current_ident, 2, job);

        // the next job out of the queue should be jgpu
        let job = scheduler.fetch_new_task(
            current_ident,
            caps.clone(),
            &build_failures,
            node_meta.clone(),
        );
        check_job(job, jgpu);

        // we have exhausted all the gpu jobs, we now expect to setup a cpu job
        let job = scheduler.fetch_new_task(
            current_ident,
            caps.clone(),
            &build_failures,
            node_meta.clone(),
        );
        check_init(&mut current_ident, 1, job);

        // now we expectet it to be j2
        let job = scheduler.fetch_new_task(
            current_ident,
            caps.clone(),
            &build_failures,
            node_meta.clone(),
        );
        check_job(job, j2);

        // now we expect j1
        let job = scheduler.fetch_new_task(
            current_ident,
            caps.clone(),
            &build_failures,
            node_meta.clone(),
        );
        check_job(job, j1);

        // now there are no more jobs left
        let job = scheduler.fetch_new_task(
            current_ident,
            caps.clone(),
            &build_failures,
            node_meta.clone(),
        );
        check_empty(job);

        std::fs::remove_dir_all(path).ok();
    }

    #[test]
    /// start with only cpu tasks, mid way through add gpu tasks and expect a switch
    fn gpu_added_later() {
        let path = Path::new("gpu_added_later");
        std::fs::create_dir_all(path).unwrap();
        let node_meta = NodeMetadata::test_name();

        let (cpu_set, gpu_set, mut scheduler) = init(path);

        let jgpu = gpu_set.clone().unwrap_python().description().jobs()[0].clone();
        let j1 = cpu_set.clone().unwrap_python().description().jobs()[0].clone();
        let j2 = cpu_set.clone().unwrap_python().description().jobs()[1].clone();

        scheduler.insert_new_batch(cpu_set).unwrap(); // 1

        let caps = Arc::new(Requirements {
            reqs: vec!["fortran".into(), "fftw".into(), "gpu".into()]
                .into_iter()
                .collect(),
            marker: std::marker::PhantomData,
        });

        let build_failures = Default::default();

        let mut current_ident = JobSetIdentifier::none();

        let job = scheduler.fetch_new_task(
            current_ident,
            caps.clone(),
            &build_failures,
            node_meta.clone(),
        );
        check_init(&mut current_ident, 1, job);

        // the next job out of the queue should be jgpu
        let job = scheduler.fetch_new_task(
            current_ident,
            caps.clone(),
            &build_failures,
            node_meta.clone(),
        );
        check_job(job, j2);

        scheduler.insert_new_batch(gpu_set).unwrap(); // 2

        // now that there are gpu jobs available we expect to initialize
        let job = scheduler.fetch_new_task(
            current_ident,
            caps.clone(),
            &build_failures,
            node_meta.clone(),
        );
        check_init(&mut current_ident, 2, job);

        // and now run the gpu job
        let job = scheduler.fetch_new_task(
            current_ident,
            caps.clone(),
            &build_failures,
            node_meta.clone(),
        );
        check_job(job, jgpu);

        // exhaused gpu jobs - now we run cpu job init again
        let job = scheduler.fetch_new_task(
            current_ident,
            caps.clone(),
            &build_failures,
            node_meta.clone(),
        );
        check_init(&mut current_ident, 1, job);

        // now back to cpu - run j1
        let job = scheduler.fetch_new_task(
            current_ident,
            caps.clone(),
            &build_failures,
            node_meta.clone(),
        );
        check_job(job, j1);

        // now there are no more jobs left
        let job = scheduler.fetch_new_task(
            current_ident,
            caps.clone(),
            &build_failures,
            node_meta.clone(),
        );
        check_empty(job);

        std::fs::remove_dir_all(path).unwrap();
    }

    #[test]
    /// start with only cpu tasks, mid way through add gpu tasks and expect a switch
    fn different_caps() {
        let path = Path::new("different_caps");
        std::fs::create_dir_all(path).unwrap();

        let (cpu_set, gpu_set, mut scheduler) = init(path);

        let jgpu = gpu_set.clone().unwrap_python().description().jobs()[0].clone();
        let j1 = cpu_set.clone().unwrap_python().description().jobs()[0].clone();
        let j2 = cpu_set.clone().unwrap_python().description().jobs()[1].clone();

        let node_meta = NodeMetadata::test_name();

        let cpu_ident = 1;
        let gpu_ident = 2;
        scheduler.insert_new_batch(cpu_set).unwrap(); // 1
        scheduler.insert_new_batch(gpu_set).unwrap(); // 2

        let caps_gpu = Arc::new(Requirements {
            reqs: vec!["fortran".into(), "fftw".into(), "gpu".into()]
                .into_iter()
                .collect(),
            marker: std::marker::PhantomData,
        });
        let caps_cpu = Arc::new(Requirements {
            reqs: vec!["fortran".into(), "fftw".into()].into_iter().collect(),
            marker: std::marker::PhantomData,
        });

        let mut curr_cpu_ident = JobSetIdentifier::none();
        let mut curr_gpu_ident = JobSetIdentifier::none();

        let build_failures = Default::default();

        // cpu pulls a job - it should only be a cpu job
        let job = scheduler.fetch_new_task(
            curr_cpu_ident,
            caps_cpu.clone(),
            &build_failures,
            node_meta.clone(),
        );
        check_init(&mut curr_cpu_ident, cpu_ident, job);

        // gpu pulls a job - it should be a GPU job
        let job = scheduler.fetch_new_task(
            curr_gpu_ident,
            caps_gpu.clone(),
            &build_failures,
            node_meta.clone(),
        );
        check_init(&mut curr_gpu_ident, gpu_ident, job);

        // gpu job has built - we pull a job with those caps
        let job = scheduler.fetch_new_task(
            curr_gpu_ident,
            caps_gpu.clone(),
            &build_failures,
            node_meta.clone(),
        );
        check_job(job, jgpu);

        // cpu job now pulls its first job
        let job = scheduler.fetch_new_task(
            curr_cpu_ident,
            caps_cpu.clone(),
            &build_failures,
            node_meta.clone(),
        );
        check_job(job, j2);

        // gpu job has finished - it should get an initialization for the gpu job now
        let job = scheduler.fetch_new_task(
            curr_gpu_ident,
            caps_gpu.clone(),
            &build_failures,
            node_meta.clone(),
        );
        check_init(&mut curr_gpu_ident, cpu_ident, job);

        // gpu init has finished - it pulls again and gets j1
        let job = scheduler.fetch_new_task(
            curr_gpu_ident,
            caps_gpu.clone(),
            &build_failures,
            node_meta.clone(),
        );
        check_job(job, j1);

        // cpu task pulls and there is nothing left
        let job = scheduler.fetch_new_task(
            curr_cpu_ident,
            caps_cpu.clone(),
            &build_failures,
            node_meta.clone(),
        );
        check_empty(job);

        std::fs::remove_dir_all(path).unwrap();
    }

    #[test]
    /// dont let the scheduler send a build / run job to a node that
    /// has failed to build the task
    fn dont_send_failed_builds() {
        let path = Path::new("dont_send_failed_builds");
        std::fs::create_dir_all(path).unwrap();
        let node_meta = NodeMetadata::test_name();

        let (cpu_set, _gpu_set, mut scheduler) = init(path);
        scheduler.insert_new_batch(cpu_set).unwrap();

        dbg!(&scheduler.map);

        let mut build_failures = BTreeSet::new();

        let caps = Arc::new(Requirements {
            reqs: vec!["fortran".into(), "fftw".into()].into_iter().collect(),
            marker: std::marker::PhantomData,
        });

        let mut current_ident = JobSetIdentifier::none();

        let job = scheduler.fetch_new_task(
            current_ident,
            caps.clone(),
            &build_failures,
            node_meta.clone(),
        );
        check_init(&mut current_ident, 1, job);

        // now we tell them that we failed to build the job
        // BUT, the total number of nodes available to run the
        // job is 2, so the jobset should not get removed
        build_failures.insert(current_ident);
        scheduler.mark_build_failure(current_ident, 2);

        // ask for a new job, it should be a build request to the next job
        let job = scheduler.fetch_new_task(
            current_ident,
            caps.clone(),
            &build_failures,
            node_meta.clone(),
        );
        assert_eq!(job, JobResponse::EmptyJobs);

        std::fs::remove_dir_all(path).ok();
    }

    #[test]
    /// remove a jobset from the
    fn remove_set_if_no_build() {
        let path = Path::new("remove_set_if_no_build");
        std::fs::create_dir_all(path).unwrap();

        let (cpu_set, _gpu_set, mut scheduler) = init(path);
        scheduler.insert_new_batch(cpu_set).unwrap();

        let build_failures = BTreeSet::new();
        let node_meta = NodeMetadata::test_name();

        let caps = Arc::new(Requirements {
            reqs: vec!["fortran".into(), "fftw".into()].into_iter().collect(),
            marker: std::marker::PhantomData,
        });

        let mut current_ident = JobSetIdentifier::none();

        let job = scheduler.fetch_new_task(current_ident, caps.clone(), &build_failures, node_meta);
        check_init(&mut current_ident, 1, job);

        // now we tell them that we failed to build the job
        // since there is only 1 job available to build the
        // node then the sceduler should remove the job set from
        // the queue
        scheduler.mark_build_failure(current_ident, 1);

        // make sure that it was in fact removed correctly
        assert_eq!(scheduler.map.len(), 0);

        std::fs::remove_dir_all(path).ok();
    }
}
