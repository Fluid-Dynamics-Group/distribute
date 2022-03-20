use super::Machine;
use crate::prelude::*;

pub(crate) struct PrepareBuild;
pub(crate) struct ClientPrepareBuildState {
    conn: transport::FollowerConnection<ClientMsg>,
}

pub(crate) struct ServerPrepareBuildState {
    conn: transport::ServerConnection<ServerMsg>,
    common: super::Common,
}
use super::compiling::{Building, ClientBuildingState, ServerBuildingState};

#[derive(thiserror::Error, Debug, From)]
pub(crate) enum Error {
    #[error("{0}")]
    TcpConnection(error::TcpConnection),
    #[error("Node failed the keepalive check")]
    MissedKeepalive,
}

impl Machine<PrepareBuild, ClientPrepareBuildState> {
    async fn receive_job(mut self) -> Result<Machine<Building, ClientBuildingState>, Error> {
        let msg = self.state.conn.receive_data().await?;

        let job: config::BuildOpts = msg.unwrap_initialize_job();

        // TODO: save the job in the next state
        todo!()
    }
}

impl Machine<PrepareBuild, ServerPrepareBuildState> {
    /// fetch a new job from the scheduler and send that job to the child node
    async fn receive_job(
        mut self,
        scheduler_tx: &mut mpsc::Sender<server::JobRequest>,
    ) -> Result<Machine<Building, ServerBuildingState>, Error> {

        let job = server::node::fetch_new_job(
            scheduler_tx,
            server::JobIdentifier::none(),
            &self.state.common.node_name,
            &self.state.conn.addr,
            &self.state.common.keepalive_addr,
            self.state.common.capabilities.clone(),
            self.state.common.errored_jobs.clone()
        ).await;

        // TODO: specify the query function that we only receive BuildTaskInfo
        //       and then we wont have the possibility of erroring here
        let build_job : server::pool_data::BuildTaskInfo= match job {
            server::pool_data::FetchedJob::Build(build) => build,
            server::pool_data::FetchedJob::Run(_run) => {
                error!("got execution job on {} / {} when we have not initialized anything. This is a bug", self.state.common.node_name, self.state.conn.addr );
                panic!("got execution job on {} / {} when we have not initialized anything. This is a bug", self.state.common.node_name, self.state.conn.addr );
                //
            }
            server::pool_data::FetchedJob::MissedKeepalive => return Err(Error::MissedKeepalive)
        };

        let msg = ServerMsg::InitializeJob(build_job.task);
        self.state.conn.transport_data(&msg).await?;

        todo!()
    }
}

#[derive(Serialize, Deserialize, Unwrap)]
// TODO: also need to send full information to rebuild the next state if required
pub(crate) enum ServerMsg {
    InitializeJob(config::BuildOpts),
}

#[derive(Debug)]
enum FlatServerMsg {}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub(crate) enum ClientMsg {}

#[derive(Debug)]
enum FlatClientMsg {}

impl transport::AssociatedMessage for ServerMsg {
    type Receive = ClientMsg;
}

impl transport::AssociatedMessage for ClientMsg {
    type Receive = ServerMsg;
}
