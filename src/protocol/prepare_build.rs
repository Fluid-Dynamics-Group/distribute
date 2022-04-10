use super::Machine;
use crate::prelude::*;

#[derive(Default)]
pub(crate) struct PrepareBuild;

pub(crate) struct ClientPrepareBuildState {
    pub(in super) conn: transport::Connection<ClientMsg>,
}

pub(crate) struct ServerPrepareBuildState {
    pub(in super) conn: transport::Connection<ServerMsg>,
    pub(in super) common: super::Common,
}

use super::compiling::{Building, ClientBuildingState, ServerBuildingState};
use super::uninit::{ClientUninitState, ServerUninitState, Uninit};

#[derive(thiserror::Error, Debug, From)]
// TODO: the server can error for a different reason than the client. If the server senses
// a keepalive connection has failed, then it kills the connection. However, the client sees
// this as a tcp error. Tcp errors have a special case handling in the client impl (they expect the
// server to recreate the connection), however - the client will await a new connection from the
// server instead of operating on the old TCP connection. The server should then sense that the
// TCP connection has closed, but the result of this behavior is not clear at the moment
pub(crate) enum ClientError {
    #[error("{0}")]
    TcpConnection(error::TcpConnection),
}

#[derive(thiserror::Error, Debug, From)]
pub(crate) enum ServerError {
    #[error("{0}")]
    TcpConnection(error::TcpConnection),
    #[error("Node failed the keepalive check")]
    MissedKeepalive,
}

impl Machine<PrepareBuild, ClientPrepareBuildState> {
    pub(crate) async fn receive_job(
        mut self,
    ) -> Result<Machine<Building, ClientBuildingState>, (Self, ClientError)> {
        let msg = self.state.conn.receive_data().await;
        let msg: ServerMsg = throw_error_with_self!(msg, self);

        let job: transport::BuildOpts = msg.unwrap_initialize_job();

        // TODO: save the job in the next state
        todo!()
    }

    pub(crate) fn to_uninit(self) -> Machine<Uninit, ClientUninitState> {
        todo!()
    }
}

impl Machine<PrepareBuild, ServerPrepareBuildState> {
    /// fetch a new job from the scheduler and send that job to the child node
    pub(crate) async fn send_job(
        mut self,
        scheduler_tx: &mut mpsc::Sender<server::JobRequest>,
    ) -> Result<Machine<Building, ServerBuildingState>, (Self, ServerError)> {
        let job = server::node::fetch_new_job(
            scheduler_tx,
            server::JobIdentifier::none(),
            &self.state.common.node_name,
            &self.state.common.main_transport_addr,
            &self.state.common.keepalive_addr,
            self.state.common.capabilities.clone(),
            self.state.common.errored_jobs.clone(),
        )
        .await;

        // TODO: specify the query function that we only receive BuildTaskInfo
        //       and then we wont have the possibility of erroring here
        let build_job: server::pool_data::BuildTaskInfo = match job {
            server::pool_data::FetchedJob::Build(build) => build,
            server::pool_data::FetchedJob::Run(_run) => {
                error!("got execution job on {} / {} when we have not initialized anything. This is a bug", self.state.common.node_name, self.state.common.main_transport_addr);
                panic!("got execution job on {} / {} when we have not initialized anything. This is a bug", self.state.common.node_name, self.state.common.main_transport_addr);
                //
            }
            server::pool_data::FetchedJob::MissedKeepalive => {
                return Err((self, ServerError::MissedKeepalive))
            }
        };

        let msg = ServerMsg::InitializeJob(build_job.task);
        let tmp_msg = self.state.conn.transport_data(&msg).await;
        throw_error_with_self!(tmp_msg, self);

        todo!()
    }

    /// convert back to the uninitialized state
    pub(crate) fn to_uninit(self) -> Machine<Uninit, ServerUninitState> {
        todo!()
    }
}

#[derive(Serialize, Deserialize, Unwrap)]
// TODO: also need to send full information to rebuild the next state if required
pub(crate) enum ServerMsg {
    InitializeJob(transport::BuildOpts),
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
