use super::pool_data::{JobOrInit, JobResponse, TaskInfo};
use super::storage::{self, StoredJob, StoredJobInit};

use crate::config;
use crate::error::{self, ScheduleError};
use crate::transport;
use derive_more::{Constructor, Display, From};
use serde::{Deserialize, Serialize};

use std::collections::{BTreeMap, BTreeSet};
use std::convert::TryFrom;
use std::fmt;
use std::iter::FromIterator;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub(crate) trait Schedule {
    fn fetch_new_task(
        &mut self,
        current_compiled_job: JobIdentifier,
        node_caps: Arc<Requirements<NodeProvidedCaps>>,
        build_failures: &BTreeSet<JobIdentifier>,
    ) -> JobResponse;

    fn insert_new_batch(&mut self, jobs: storage::OwnedJobSet) -> Result<(), ScheduleError>;

    fn add_job_back(&mut self, job: storage::JobOpt, identifier: JobIdentifier);

    fn finish_job(&mut self, job: JobIdentifier);

    /// fetch all of the batch names and associated jobs that are still running
    fn remaining_jobs(&self) -> Vec<RemainingJobs>;

    /// find a job identifier associated with the name of a certain job
    fn identifier_by_name(&self, batch_name: &str) -> Option<JobIdentifier>;

    fn mark_build_failure(&mut self, failed_ident: JobIdentifier, total_nodes: usize);
}

#[derive(Clone, Ord, PartialEq, Eq, PartialOrd, Copy, Display, Debug)]
pub(crate) enum JobIdentifier {
    #[display(fmt = "{}", _0)]
    Identity(u64),
    #[display(fmt = "Empty")]
    None,
}

impl JobIdentifier {
    pub(crate) fn none() -> Self {
        Self::None
    }
    fn new(ident: u64) -> Self {
        JobIdentifier::Identity(ident)
    }
}

#[derive(Default, Constructor)]
pub(crate) struct GpuPriority {
    map: BTreeMap<JobIdentifier, JobSet>,
    last_identifier: u64,
    base_path: PathBuf,
}

impl GpuPriority {
    /// grab the first job that we have available. It is assumed that this function is only called
    /// if we have ensured that the current jobidentifier has no remaining jobs left
    ///
    /// therefore, this function only returns a job initialization task or empty jobs
    fn take_first_job(
        &mut self,
        node_caps: Arc<Requirements<NodeProvidedCaps>>,
        build_failures: &BTreeSet<JobIdentifier>,
    ) -> JobResponse {
        if let Some((ident, job_set)) = self
            .map
            .iter_mut()
            .filter(|(_ident, job_set)| job_set.has_remaining_jobs())
            // make sure the capabilities of this node match the total capabilities
            .filter(|(_ident, job_set)| node_caps.can_accept_job(&job_set.requirements))
            // make sure that the job we are pulling has not failed to build on this node before
            .filter(|(ident, job_set)| !build_failures.contains(ident))
            .next()
        {
            // TODO: fix this unwrap - its not that good but i dont have a better way to handle
            // it right now
            let init = job_set.init_file().unwrap();
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
    fn job_by_id(&mut self, identifier: JobIdentifier) -> Option<JobResponse> {
        self.map
            .get_mut(&identifier)
            .map(|job_set| {
                // check if the process with the current id has any jobs remaining
                // if it has nothing return None instead of JobResponse::EmptyJobs
                if job_set.has_remaining_jobs() {
                    // TODO: this can panic
                    //
                    debug!(
                        "fetching next job since there are jobs remaining in set {}",
                        identifier
                    );
                    let job = job_set.next_job().unwrap();
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
            .flatten()
    }
}

impl Schedule for GpuPriority {
    fn fetch_new_task(
        &mut self,
        current_compiled_job: JobIdentifier,
        node_caps: Arc<Requirements<NodeProvidedCaps>>,
        build_failures: &BTreeSet<JobIdentifier>,
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
            .filter(|(ident, job_set)| !build_failures.contains(ident))
            .next()
        {
            // if we are already working on this job - we dont need to recompile anything,
            // just send the next iteration of the job out
            if current_compiled_job == *gpu_ident {
                let job = gpu_job_set.next_job().unwrap();
                JobResponse::SetupOrRun(TaskInfo::new(
                    gpu_job_set.namespace.clone(),
                    gpu_job_set.batch_name.clone(),
                    current_compiled_job,
                    JobOrInit::Job(job),
                ))
            } else {
                // TODO: fix this unwrap - there has to be a better way
                // to do this but i have not worked around it right now
                let init = gpu_job_set.init_file().unwrap();
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
        else {
            if let Some(job) = self.job_by_id(current_compiled_job) {
                job
            } else {
                self.take_first_job(node_caps, build_failures)
            }
        }
    }

    fn insert_new_batch(&mut self, jobs: storage::OwnedJobSet) -> Result<(), ScheduleError> {
        let jobs =
            JobSet::from_owned(jobs, &self.base_path).map_err(|e| error::StoreSet::from(e))?;

        self.last_identifier += 1;
        let ident = JobIdentifier::new(self.last_identifier);
        self.map.insert(ident, jobs);

        Ok(())
    }

    fn add_job_back(&mut self, job: storage::JobOpt, identifier: JobIdentifier) {
        if let Some(job_set) = self.map.get_mut(&identifier) {
            job_set.add_errored_job(job, &self.base_path)
        } else {
            error!(
                "job set for job {} was removed from the tree when there was still 
                a job running - this job can no longer be run. This should not happen",
                job.name()
            );
        }
    }

    fn finish_job(&mut self, identifier: JobIdentifier) {
        debug!("called finish job with {}", identifier);

        if let Some(job_set) = self.map.get_mut(&identifier) {
            job_set.job_finished();

            // if there are no more jobs to run, and the final job for this process
            // has just finished - remove the job set from the tree
            if !job_set.has_remaining_jobs() && job_set.currently_running_jobs == 0 {
                info!(
                    "removeing tree with identifier {} from the queue - all jobs have finished 
                    and there are no more running jobs",
                    identifier
                );

                let removed_set = self.map.remove(&identifier).unwrap();

                // since we are done with this job set then we should deallocate the build file
                removed_set.build.delete().ok();

                if let Some(matrix_id) = &removed_set.matrix_user {
                    let matrix_id = matrix_id.clone();

                    info!(
                        "sending message to matrix user {} for batch finish `{}`",
                        matrix_id, removed_set.batch_name
                    );

                    // send the matrix message
                    super::matrix::send_matrix_message(
                        matrix_id,
                        removed_set,
                        super::matrix::Reason::FinishedAll,
                    )
                }
            }
        } else {
            error!(
                "a job set for identifier {} was removed before all 
               of the running jobs for the set were done. This shuould not happen",
                &identifier
            )
        }
    }

    fn remaining_jobs(&self) -> Vec<RemainingJobs> {
        self.map
            .iter()
            .map(|(_, job_set)| job_set.remaining_jobs())
            .collect()
    }

    fn identifier_by_name(&self, batch_name: &str) -> Option<JobIdentifier> {
        self.map
            .iter()
            .filter(|(_, set)| set.batch_name == batch_name)
            .map(|(identifier, _)| *identifier)
            .next()
    }

    fn mark_build_failure(&mut self, failed_ident: JobIdentifier, total_nodes: usize) {
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
                    let matrix_id = matrix_id.clone();

                    info!(
                        "sending message to matrix user {} for finish to `{}`",
                        matrix_id, removed_set.batch_name
                    );

                    super::matrix::send_matrix_message(
                        matrix_id,
                        removed_set,
                        super::matrix::Reason::BuildFailures,
                    )
                }
            }
        } else {
            warn!("Failed to mark job set {} as failing to build for a node since it could not be found in the job set list. This should not happen", failed_ident);
        }
    }
}

#[derive(Constructor, Debug)]
pub(crate) struct JobSet {
    build: StoredJobInit,
    requirements: Requirements<JobRequiredCaps>,
    remaining_jobs: Vec<StoredJob>,
    currently_running_jobs: usize,
    pub(crate) batch_name: String,
    namespace: String,
    matrix_user: Option<matrix_notify::UserId>,
    build_failures: usize,
}

impl JobSet {
    fn has_remaining_jobs(&self) -> bool {
        self.remaining_jobs.len() > 0
    }

    fn next_job(&mut self) -> Option<storage::JobOpt> {
        if let Some(job) = self.remaining_jobs.pop() {
            self.currently_running_jobs += 1;
            job.load_job().ok()
        } else {
            None
        }
    }

    fn init_file(&mut self) -> Result<config::BuildOpts, std::io::Error> {
        self.build.load_build()
    }

    fn add_errored_job(&mut self, job: storage::JobOpt, base_path: &Path) {
        self.currently_running_jobs -= 1;

        match StoredJob::from_opt(job, base_path) {
            Ok(job) => self.remaining_jobs.push(job),
            Err(e) => error!("could not add job back to lazy storage: {}", e),
        }
    }

    fn job_finished(&mut self) {
        if self.currently_running_jobs == 0 {
            warn!("a job from {}'s currently_running_jobs finished, but the value was already zero. This should not happen", &self.batch_name);
        } else {
            self.currently_running_jobs -= 1
        }
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
                .map(|x| x.job_name().to_string())
                .collect(),
            running_jobs: self.currently_running_jobs,
        }
    }

    pub(crate) fn from_owned(
        owned: storage::OwnedJobSet,
        base_path: &Path,
    ) -> Result<Self, std::io::Error> {
        let storage::OwnedJobSet {
            build,
            requirements,
            remaining_jobs,
            currently_running_jobs,
            batch_name,
            matrix_user,
            namespace,
        } = owned;
        let build = StoredJobInit::from_opt(build, base_path)?;
        let remaining_jobs = match remaining_jobs {
            config::JobOpts::Python(python_jobs) => {
                let mut out = vec![];
                for job in python_jobs {
                    let stored_job = StoredJob::from_python(job, base_path)?;
                    out.push(stored_job);
                }
                out
            }
            config::JobOpts::Singularity(python_jobs) => {
                let mut out = vec![];
                for job in python_jobs {
                    let stored_job = StoredJob::from_singularity(job, base_path)?;
                    out.push(stored_job);
                }
                out
            }
        };

        Ok(Self {
            build,
            requirements,
            remaining_jobs,
            currently_running_jobs,
            batch_name,
            matrix_user,
            namespace,
            build_failures: 0,
        })
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub(crate) struct RemainingJobs {
    pub(crate) batch_name: String,
    pub(crate) jobs_left: Vec<String>,
    pub(crate) running_jobs: usize,
}

#[derive(From, Debug, Clone, Deserialize, Serialize)]
#[serde(transparent)]
pub struct Requirements<T> {
    reqs: BTreeSet<Requirement>,
    #[serde(skip)]
    marker: std::marker::PhantomData<T>,
}

impl Requirements<NodeProvidedCaps> {
    pub(crate) fn can_accept_job(&self, job_reqs: &Requirements<JobRequiredCaps>) -> bool {
        self.reqs.is_superset(&job_reqs.reqs)
    }
}

impl<T> FromIterator<Requirement> for Requirements<T> {
    fn from_iter<V>(iter: V) -> Self
    where
        V: IntoIterator<Item = Requirement>,
    {
        Requirements {
            reqs: iter.into_iter().collect(),
            marker: std::marker::PhantomData::<T>,
        }
    }
}

impl Requirements<JobRequiredCaps> {
    fn requires_gpu(&self) -> bool {
        // TODO: can probably make Requirement<T> and work with
        // generics to remove this heap allocation
        self.reqs.contains(&Requirement("gpu".to_string()))
    }
}

impl<T> fmt::Display for Requirements<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let formatted = self
            .reqs
            .iter()
            .map(|x| format!("{}, ", x))
            .collect::<String>();
        write!(f, "{}", formatted)
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct NodeProvidedCaps;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct JobRequiredCaps;

#[derive(From, Ord, Eq, PartialEq, PartialOrd, Debug, Clone, Deserialize, Serialize, Display)]
pub struct Requirement(String);

impl From<&str> for Requirement {
    fn from(x: &str) -> Self {
        Requirement(x.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::storage::OwnedJobSet;

    fn check_init(current_ident: &mut JobIdentifier, expected_ident: u64, response: JobResponse) {
        match response {
            JobResponse::SetupOrRun(task) => {
                task.task.unwrap_job_init();
                assert_eq!(task.identifier, JobIdentifier::Identity(expected_ident));
                *current_ident = task.identifier;
            }
            JobResponse::EmptyJobs => {
                panic!("empty jobs returned when all jobs still present")
            }
        }
    }

    fn check_job(response: JobResponse, expected_job: transport::PythonJob) {
        match response {
            JobResponse::SetupOrRun(task) => {
                assert_eq!(task.task.unwrap_job(), storage::JobOpt::from(expected_job));
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

    fn init(path: &Path) -> (OwnedJobSet, OwnedJobSet, GpuPriority) {
        let scheduler = GpuPriority::new(Default::default(), Default::default(), path.to_owned());

        let jgpu = transport::PythonJob {
            python_file: vec![],
            job_name: "jgpu".into(),
            job_files: vec![],
        };

        let j1 = transport::PythonJob {
            python_file: vec![],
            job_name: "j1".into(),
            job_files: vec![],
        };

        let j2 = transport::PythonJob {
            python_file: vec![],
            job_name: "j2".into(),
            job_files: vec![],
        };

        let cpu_set = OwnedJobSet {
            batch_name: "cpu_jobs".to_string(),
            namespace: "test".to_string(),
            build: transport::PythonJobInit {
                batch_name: "cpu_jobs".to_string(),
                python_setup_file: vec![0],
                additional_build_files: vec![],
            }
            .into(),
            remaining_jobs: vec![j1.clone(), j2.clone()].into(),
            requirements: Requirements {
                reqs: vec!["fortran".into(), "fftw".into()].into_iter().collect(),
                marker: std::marker::PhantomData,
            },
            currently_running_jobs: 0,
            matrix_user: None,
        };

        let gpu_set = OwnedJobSet {
            batch_name: "gpu_jobs".to_string(),
            namespace: "test".to_string(),
            build: transport::PythonJobInit {
                batch_name: "gpu_jobs".to_string(),
                python_setup_file: vec![1],
                additional_build_files: vec![],
            }
            .into(),
            remaining_jobs: vec![jgpu.clone()].into(),
            requirements: Requirements {
                reqs: vec!["gpu".into()].into_iter().collect(),
                marker: std::marker::PhantomData,
            },
            currently_running_jobs: 0,
            matrix_user: None,
        };

        (cpu_set, gpu_set, scheduler)
    }

    #[test]
    /// put both gpu and cpu sets in the queue and expect the gpu sets to be received first
    fn gpu_first() {
        let path = Path::new("gpu_first");
        std::fs::create_dir_all(path).unwrap();

        let (cpu_set, gpu_set, mut scheduler) = init(path);

        let jgpu = gpu_set.remaining_jobs.clone().unwrap_python()[0].clone();
        let j1 = cpu_set.remaining_jobs.clone().unwrap_python()[0].clone();
        let j2 = cpu_set.remaining_jobs.clone().unwrap_python()[1].clone();

        scheduler.insert_new_batch(cpu_set).unwrap();
        scheduler.insert_new_batch(gpu_set).unwrap();

        let caps = Arc::new(Requirements {
            reqs: vec!["fortran".into(), "fftw".into(), "gpu".into()]
                .into_iter()
                .collect(),
            marker: std::marker::PhantomData,
        });

        let mut current_ident = JobIdentifier::none();

        let job = scheduler.fetch_new_task(current_ident, caps.clone());
        check_init(&mut current_ident, 2, job);

        // the next job out of the queue should be jgpu
        let job = scheduler.fetch_new_task(current_ident, caps.clone());
        check_job(job, jgpu);

        // we have exhausted all the gpu jobs, we now expect to setup a cpu job
        let job = scheduler.fetch_new_task(current_ident, caps.clone());
        check_init(&mut current_ident, 1, job);

        // now we expectet it to be j2
        let job = scheduler.fetch_new_task(current_ident, caps.clone());
        check_job(job, j2);

        // now we expect j1
        let job = scheduler.fetch_new_task(current_ident, caps.clone());
        check_job(job, j1);

        // now there are no more jobs left
        let job = scheduler.fetch_new_task(current_ident, caps.clone());
        check_empty(job);

        std::fs::remove_dir_all(path).ok();
    }

    #[test]
    /// start with only cpu tasks, mid way through add gpu tasks and expect a switch
    fn gpu_added_later() {
        let path = Path::new("gpu_added_later");
        std::fs::create_dir_all(path).unwrap();

        let (cpu_set, gpu_set, mut scheduler) = init(path);

        let jgpu = gpu_set.remaining_jobs.clone().unwrap_python()[0].clone();
        let j1 = cpu_set.remaining_jobs.clone().unwrap_python()[0].clone();
        let j2 = cpu_set.remaining_jobs.clone().unwrap_python()[1].clone();

        scheduler.insert_new_batch(cpu_set).unwrap(); // 1

        let caps = Arc::new(Requirements {
            reqs: vec!["fortran".into(), "fftw".into(), "gpu".into()]
                .into_iter()
                .collect(),
            marker: std::marker::PhantomData,
        });

        let mut current_ident = JobIdentifier::none();

        let job = scheduler.fetch_new_task(current_ident, caps.clone());
        check_init(&mut current_ident, 1, job);

        // the next job out of the queue should be jgpu
        let job = scheduler.fetch_new_task(current_ident, caps.clone());
        check_job(job, j2);

        scheduler.insert_new_batch(gpu_set).unwrap(); // 2

        // now that there are gpu jobs available we expect to initialize
        let job = scheduler.fetch_new_task(current_ident, caps.clone());
        check_init(&mut current_ident, 2, job);

        // and now run the gpu job
        let job = scheduler.fetch_new_task(current_ident, caps.clone());
        check_job(job, jgpu);

        // exhaused gpu jobs - now we run cpu job init again
        let job = scheduler.fetch_new_task(current_ident, caps.clone());
        check_init(&mut current_ident, 1, job);

        // now back to cpu - run j1
        let job = scheduler.fetch_new_task(current_ident, caps.clone());
        check_job(job, j1);

        // now there are no more jobs left
        let job = scheduler.fetch_new_task(current_ident, caps.clone());
        check_empty(job);

        std::fs::remove_dir_all(path).unwrap();
    }

    #[test]
    /// start with only cpu tasks, mid way through add gpu tasks and expect a switch
    fn different_caps() {
        let path = Path::new("different_caps");
        std::fs::create_dir_all(path).unwrap();

        let (cpu_set, gpu_set, mut scheduler) = init(path);

        let jgpu = gpu_set.remaining_jobs.clone().unwrap_python()[0].clone();
        let j1 = cpu_set.remaining_jobs.clone().unwrap_python()[0].clone();
        let j2 = cpu_set.remaining_jobs.clone().unwrap_python()[1].clone();

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

        let mut curr_cpu_ident = JobIdentifier::none();
        let mut curr_gpu_ident = JobIdentifier::none();

        // cpu pulls a job - it should only be a cpu job
        let job = scheduler.fetch_new_task(curr_cpu_ident, caps_cpu.clone());
        check_init(&mut curr_cpu_ident, cpu_ident, job);

        // gpu pulls a job - it should be a GPU job
        let job = scheduler.fetch_new_task(curr_gpu_ident, caps_gpu.clone());
        check_init(&mut curr_gpu_ident, gpu_ident, job);

        // gpu job has built - we pull a job with those caps
        let job = scheduler.fetch_new_task(curr_gpu_ident, caps_gpu.clone());
        check_job(job, jgpu);

        // cpu job now pulls its first job
        let job = scheduler.fetch_new_task(curr_cpu_ident, caps_cpu.clone());
        check_job(job, j2);

        // gpu job has finished - it should get an initialization for the gpu job now
        let job = scheduler.fetch_new_task(curr_gpu_ident, caps_gpu.clone());
        check_init(&mut curr_gpu_ident, cpu_ident, job);

        // gpu init has finished - it pulls again and gets j1
        let job = scheduler.fetch_new_task(curr_gpu_ident, caps_gpu.clone());
        check_job(job, j1);

        // cpu task pulls and there is nothing left
        let job = scheduler.fetch_new_task(curr_cpu_ident, caps_cpu.clone());
        check_empty(job);

        std::fs::remove_dir_all(path).unwrap();
    }
}
