use super::send_files;
use super::Machine;
use crate::prelude::*;
use server::pool_data;

#[derive(Default)]
pub(crate) struct PrepareBuild;

pub(crate) struct ClientPrepareBuildState {
    pub(super) conn: transport::Connection<ClientMsg>,
    pub(super) working_dir: WorkingDir,
    pub(super) cancel_addr: SocketAddr,
}

pub(crate) struct ServerPrepareBuildState {
    pub(super) conn: transport::Connection<ServerMsg>,
    pub(super) common: super::Common,
}

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

type ClientReceivingState = send_files::ReceiverState<send_files::BuildingReceiver>;
type ServerSendingState = send_files::SenderState<send_files::BuildingSender>;

impl Machine<PrepareBuild, ClientPrepareBuildState> {
    #[instrument(skip(self), fields(working_dir = %self.state.working_dir))]
    pub(crate) async fn receive_job(
        mut self,
    ) -> Result<Machine<send_files::SendFiles, ClientReceivingState>, (Self, ClientError)> {
        if self.state.working_dir.exists() {
            // TODO: probably some better error handling on this
            self.state
                .working_dir
                .delete_and_create_folders()
                .await
                .ok();
        }

        let msg = self.state.conn.receive_data().await;
        let msg: ServerMsg = throw_error_with_self!(msg, self);

        let job: pool_data::BuildTaskInfo = msg.unwrap_initialize_job();

        let compiling_state = self.into_send_files_state(job).await;
        let machine = Machine::from_state(compiling_state);
        Ok(machine)
    }

    pub(crate) fn into_uninit(self) -> Machine<Uninit, ClientUninitState> {
        let ClientPrepareBuildState {
            conn,
            working_dir,
            cancel_addr,
            ..
        } = self.state;
        let conn = conn.update_state();
        let state = super::uninit::ClientUninitState {
            conn,
            working_dir,
            cancel_addr,
        };
        debug!("moving client prepare build -> uninit");
        Machine::from_state(state)
    }

    pub(crate) async fn into_send_files_state(
        self,
        build_info: pool_data::BuildTaskInfo,
    ) -> send_files::ReceiverState<send_files::BuildingReceiver> {
        debug!("moving client prepare build -> send_files (compiling)");
        let ClientPrepareBuildState {
            conn,
            working_dir,
            cancel_addr,
        } = self.state;

        #[allow(unused_mut)]
        let mut conn = conn.update_state();

        #[cfg(test)]
        assert!(conn.bytes_left().await == 0);

        // TODO: have better function to generate this signature
        let save_location = working_dir.initial_files_folder();

        let extra = send_files::BuildingReceiver {
            working_dir,
            cancel_addr,
            node_meta: pool_data::NodeMetadata::new("SERVER".into(), ([0, 0, 0, 0], 0).into()),
            build_info,
        };

        send_files::ReceiverState {
            conn,
            extra,
            save_location,
        }
    }
}

impl Machine<PrepareBuild, ServerPrepareBuildState> {
    /// fetch a new job from the scheduler and send that job to the child node
    ///
    /// This method is also responsible for creating direcories for the namespace and batch name
    /// that this job will store its results in
    #[instrument(
        skip(self, scheduler_tx), 
        fields(
            node_meta = %self.state.common.node_meta,
        )
    )]
    pub(crate) async fn send_job(
        mut self,
        scheduler_tx: &mut mpsc::Sender<server::JobRequest>,
    ) -> Result<Machine<send_files::SendFiles, ServerSendingState>, (Self, ServerError)> {
        let job = server::node::fetch_new_job(
            scheduler_tx,
            server::JobSetIdentifier::none(),
            &self.state.common.node_meta,
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
                error!(
                    "got execution job on {} when we have not initialized anything. This is a bug",
                    self.state.common.node_meta
                );
                panic!(
                    "got execution job on {} when we have not initialized anything. This is a bug",
                    self.state.common.node_meta
                );
                //
            }
            server::pool_data::FetchedJob::MissedKeepalive => {
                return Err((self, ServerError::MissedKeepalive))
            }
        };

        // tell the node about the compiling job
        let msg = ServerMsg::InitializeJob(build_job.clone());
        let tmp_msg = self.state.conn.transport_data(&msg).await;
        throw_error_with_self!(tmp_msg, self);

        // return Machine<BuildingState, _>
        let compiling_state = self.into_send_files_state(build_job).await;
        let machine = Machine::from_state(compiling_state);
        Ok(machine)
    }

    /// convert back to the uninitialized state
    pub(crate) fn into_uninit(self) -> Machine<Uninit, ServerUninitState> {
        let ServerPrepareBuildState { conn, common, .. } = self.state;
        let conn = conn.update_state();
        let state = super::uninit::ServerUninitState { conn, common };
        debug!(
            "moving {} server prepare build -> uninit",
            state.common.node_meta
        );
        Machine::from_state(state)
    }

    pub(crate) async fn into_send_files_state(
        self,
        //namespace: String,
        //batch_name: String,
        //job_identifier: server::JobSetIdentifier,
        build_info: pool_data::BuildTaskInfo,
    ) -> send_files::SenderState<send_files::BuildingSender> {
        let batch_name = &build_info.batch_name;
        let job_identifier = &build_info.identifier;
        debug!(
            "moving {} server prepare build -> send_files (building) for job set name {batch_name} (ident: {job_identifier})",
            self.state.common.node_meta
        );
        let ServerPrepareBuildState { conn, common } = self.state;
        #[allow(unused_mut)]
        let mut conn = conn.update_state();

        #[cfg(test)]
        assert!(conn.bytes_left().await == 0);

        let extra = send_files::BuildingSender { common, build_info };

        send_files::SenderState { conn, extra }
    }
}

#[derive(Serialize, Deserialize, Unwrap)]
// TODO: also need to send full information to rebuild the next state if required
pub(crate) enum ServerMsg {
    InitializeJob(pool_data::BuildTaskInfo),
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
