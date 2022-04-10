use super::Machine;
use crate::prelude::*;

#[derive(Default)]
pub(crate) struct PrepareBuild;

pub(crate) struct ClientPrepareBuildState {
    pub(super) conn: transport::Connection<ClientMsg>,
    pub(super) working_dir: PathBuf,
}

pub(crate) struct ServerPrepareBuildState {
    pub(super) conn: transport::Connection<ServerMsg>,
    pub(super) common: super::Common,
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

        let compiling_state = self.into_compiling_state(job);
        let machine = Machine::from_state(compiling_state);
        Ok(machine)
    }

    pub(crate) fn to_uninit(self) -> Machine<Uninit, ClientUninitState> {
        todo!()
    }

    pub(crate) fn into_compiling_state(
        self,
        build_opt: transport::BuildOpts,
    ) -> super::compiling::ClientBuildingState {
        let ClientPrepareBuildState { conn, working_dir } = self.state;
        let conn = conn.update_state();
        super::compiling::ClientBuildingState {
            build_opt,
            conn,
            working_dir,
        }
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

        // pull some variables from the task info so we can store them
        // here
        let namespace = build_job.namespace.clone();
        let batch_name = build_job.batch_name.clone();
        let job_identifier = build_job.identifier;

        // tell the node about the compiling job
        let msg = ServerMsg::InitializeJob(build_job.task);
        let tmp_msg = self.state.conn.transport_data(&msg).await;
        throw_error_with_self!(tmp_msg, self);

        // return Machine<BuildingState, _>
        let compiling_state = self.into_compiling_state(namespace, batch_name, job_identifier);
        let machine = Machine::from_state(compiling_state);
        Ok(machine)
    }

    /// convert back to the uninitialized state
    pub(crate) fn to_uninit(self) -> Machine<Uninit, ServerUninitState> {
        todo!()
    }

    pub(crate) fn into_compiling_state(
        self,
        namespace: String,
        batch_name: String,
        job_identifier: server::JobIdentifier,
    ) -> super::compiling::ServerBuildingState {
        let ServerPrepareBuildState { conn, common } = self.state;
        let conn = conn.update_state();
        super::compiling::ServerBuildingState {
            conn,
            common,
            batch_name,
            namespace,
            job_identifier,
        }
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
