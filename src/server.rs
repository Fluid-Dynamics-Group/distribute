use crate::{cli, config, error, error::Error, status, transport};
use std::net::SocketAddr;
use std::path::PathBuf;
use tokio::io::{AsyncWrite, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;

pub async fn server_command(server: cli::Server) -> Result<(), Error> {
    let nodes = config::load_config::<config::Nodes>(&server.nodes_file)?;
    let jobs = config::load_config::<config::Jobs>(&server.jobs_file)?;

    if server.save_path.exists() {
        std::fs::remove_dir_all(&server.save_path)
            .map_err(|e| error::ServerError::from(error::RemoveDirError::new(e, server.save_path.clone())))?;
    }

    std::fs::create_dir_all(&server.save_path)
        .map_err(|e| error::ServerError::from(error::CreateDirError::new(e, server.save_path.clone())))?;

    // start by checking the status of each node - if one of the nodes is not ready
    // then something is wrong

    log::info!("checking connection status of each node");
    let connections = status::status_check_nodes(&nodes.nodes).await?;

    let (request_job, job_pool_holder) = mpsc::channel(100);

    info!("loading job information from files");
    let loaded_jobs = jobs.load_jobs().await.map_err(error::ServerError::from)?;

    info!("loading build information from files");
    let loaded_build = jobs.load_build().await.map_err(error::ServerError::from)?;

    // spawn off a job pool that we can query from different tasks

    info!("starting job pool task");
    let handle = JobPool::new(loaded_build, loaded_jobs, job_pool_holder).spawn();

    let mut handles = vec![handle];

    // spawn off each node connection to its own task
    for server_connection in connections {
        info!("starting NodeConnection for {}", server_connection.addr);
        let handle = NodeConnection::new(
            server_connection,
            request_job.clone(),
            server.save_path.clone(),
        )
        .spawn();
        handles.push(handle);
    }

    futures::future::join_all(handles).await;

    Ok(())
}

#[derive(derive_more::Constructor)]
struct JobPool {
    build_file: transport::JobInit,
    remaining_jobs: Vec<transport::Job>,
    receive_requests: mpsc::Receiver<JobRequest>,
}

impl JobPool {
    fn spawn(mut self) -> JoinHandle<()> {
        tokio::task::spawn(async move {
            while let Some(new_req) = self.receive_requests.recv().await {
                let new_req = match new_req {
                    JobRequest::NewJob(x) => x,
                    JobRequest::DeadNode(job) => {
                        self.remaining_jobs.push(job);
                        continue;
                    }
                };

                if new_req.is_initialized {
                    if let Some(job) = self.pop_job().await {
                        new_req.tx.send(JobResponse::RunAnotherJob(job)).ok();
                    } else {
                        new_req.tx.send(JobResponse::EmptyJobs).ok();
                    }
                } else {
                    // we need to initialize this set of jobs - this is the first time
                    // seeing this on the set of nodes

                    new_req
                        .tx
                        .send(JobResponse::SetupNewJob(self.build_file.clone()))
                        .ok();
                }
            }
        })
    }

    async fn pop_job(&mut self) -> Option<transport::Job> {
        if self.remaining_jobs.len() > 0 {
            let job_data = self.remaining_jobs.remove(self.remaining_jobs.len() - 1);
            Some(job_data)
        } else {
            None
        }
    }
}

enum JobResponse {
    SetupNewJob(transport::JobInit),
    RunAnotherJob(transport::Job),
    EmptyJobs,
}

#[derive(derive_more::From)]
enum JobRequest {
    NewJob(NewJobRequest),
    DeadNode(transport::Job),
}

struct NewJobRequest {
    tx: oneshot::Sender<JobResponse>,
    is_initialized: bool,
}

#[derive(derive_more::Constructor)]
struct NodeConnection {
    conn: transport::ServerConnection,
    request_job_tx: mpsc::Sender<JobRequest>,
    save_path: PathBuf,
}

impl NodeConnection {
    fn spawn(mut self) -> JoinHandle<()> {
        tokio::task::spawn(async move {
            // start by fetching build instructions
            let (tx, rx) = oneshot::channel();
            self.request_job_tx
                .send(JobRequest::from(NewJobRequest {
                    tx,
                    is_initialized: false,
                }))
                .await
                .ok()
                .unwrap();
            let build_instructions = if let JobResponse::SetupNewJob(build) = rx.await.unwrap() {
                build
            } else {
                panic!("build instructions were not returned from uninit request. this should not happen")
            };

            // send the build instructions from the client
            self.conn
                .transport_data(&transport::RequestFromServer::from(build_instructions))
                .await
                .unwrap();

            // TODO: check what this response is
            let client_response = self.conn.receive_data().await.unwrap();

            // now we just pull jobs from the server until we are done
            while let Ok(JobResponse::RunAnotherJob(new_job)) = self.fetch_new_job().await {
                info!("launching new job for node {}", self.addr());

                let server_request = transport::RequestFromServer::from(new_job);
                let send_response = self.conn.transport_data(&server_request).await;

                if let Err(e) = send_response {
                    error!("error for node at {}: {}", self.addr(), e);
                    let new_job = server_request.unwrap_assign_job();
                    self.request_job_tx
                        .send(JobRequest::DeadNode(new_job))
                        .await
                        .ok()
                        .unwrap();
                }

                // check what the response to the job was
                match self.handle_client_response().await {
                    Ok(response) => match response {
                        NodeNextStep::RequestNextJob => continue,
                        NodeNextStep::ClientError => {
                            error!("error occured for client job in {}", self.addr());
                        }
                    },
                    Err(e) => {
                        //
                        error!("error reading from tcp connection: {}", e);
                    }
                }
            }

            info!("closing connection for node {}", self.addr());
        })
    }

    async fn fetch_new_job(&mut self) -> Result<JobResponse, mpsc::error::SendError<JobRequest>> {
        let (tx, rx) = oneshot::channel();
        self.request_job_tx
            .send(JobRequest::from(NewJobRequest {
                tx,
                is_initialized: true,
            }))
            .await?;

        Ok(rx.await.unwrap())
    }

    fn addr(&self) -> &std::net::SocketAddr {
        &self.conn.addr
    }

    /// check what the client responded to the job request (not init) that we sent them
    ///
    /// save any files that they sent us and return a simplified
    /// version of the response they had
    async fn handle_client_response(&mut self) -> Result<NodeNextStep, Error> {
        loop {
            let response = self.conn.receive_data().await?;

            match response {
                transport::ClientResponse::StatusCheck(_) => todo!(),
                transport::ClientResponse::RequestNewJob(_) => {
                    return Ok(NodeNextStep::RequestNextJob)
                }
                transport::ClientResponse::SendFile(send_file) => {
                    // we need to store this file
                    let save_location = self.save_path.join(send_file.file_path);
                    info!("saving solver file to {:?}", save_location);
                    if send_file.is_file {
                        // TODO: fix these unwraps
                        let file = tokio::fs::File::create(&save_location)
                            .await
                            .map_err(|error| error::WriteFile::from((error, save_location.clone())))
                            .map_err(|e| error::ServerError::from(e))?;
                        let mut file = tokio::io::BufWriter::new(file);

                        file.write(&send_file.bytes).await.unwrap();
                    } else {
                        // just create the directory
                        tokio::fs::create_dir(&save_location)
                            .await
                            .map_err(|error| {
                                error::CreateDirError::from((error, save_location.clone()))
                            })
                            .map_err(|e| error::ServerError::from(e))?;
                    }
                }
                _ => unimplemented!(),
            }
        }
    }
}

enum NodeNextStep {
    RequestNextJob,
    ClientError,
}
