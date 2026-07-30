//! Internal composition boundary for one future embedded Syndrid turn.
//!
//! This module deliberately is not connected to turn admission. It composes the already
//! validated core contracts so Phase 7G6C can activate one trusted production surface without
//! adding another provider, lifecycle, observation, or transcript authority.

use codex_app_server::OrchestrationTranscriptContext;
use codex_app_server::translate_orchestration_result;
use codex_app_server_protocol::ServerNotification;
use codex_core::LiveOrchestrationCoordinator;
use codex_core::LiveOrchestrationError;
use codex_core::OrchestrationTurnResult;
use codex_core::OrchestrationTurnResultBuilder;
use codex_core::ProductionApprovedToolAdapter;
use codex_core::ProductionFinalDeliverableProducer;
use codex_core::ProductionOrchestrationInput;
use codex_core::ProductionOrchestrationLifecycle;
use codex_core::ProductionOrchestrationLifecycleError;
use codex_core::ProductionOrchestrationRequestBuilder;
use codex_core::ProductionRequestError;
use codex_core::ProductionRoleDispatchError;
use codex_core::ProductionRoleDispatcher;
use codex_core::RoleActivation;
use codex_core::RoutingConnectionDirectory;
use codex_core::RoutingProfileRegistry;
use codex_core::RoutingRole;
use codex_core::SessionExecutionPolicyState;
use codex_core::UserFacingResponseError;
use tokio::sync::mpsc;

use crate::AppServerEvent;
use crate::spawn_observation_bridge;

#[derive(Debug)]
pub(crate) enum CoordinatorError {
    Coordinator(LiveOrchestrationError),
}

impl std::fmt::Display for CoordinatorError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Coordinator(error) => write!(formatter, "coordinator failed: {error}"),
        }
    }
}

impl std::error::Error for CoordinatorError {}

/// Errors returned by the internal production-turn composition boundary.
#[derive(Debug)]
pub(crate) enum ProductionOrchestrationTurnRunnerError {
    InvalidInput(&'static str),
    Request(ProductionRequestError),
    RouteDispatch(ProductionRoleDispatchError),
    RouteMismatch(RoutingRole),
    Lifecycle(ProductionOrchestrationLifecycleError<CoordinatorError>),
    FinalDeliverable(UserFacingResponseError),
    EventChannelClosed,
}

impl std::fmt::Display for ProductionOrchestrationTurnRunnerError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidInput(field) => write!(formatter, "production input is invalid: {field}"),
            Self::Request(error) => write!(formatter, "production request is invalid: {error}"),
            Self::RouteDispatch(error) => write!(formatter, "role route is unavailable: {error}"),
            Self::RouteMismatch(role) => write!(formatter, "role route is inconsistent for {role}"),
            Self::Lifecycle(error) => write!(formatter, "production lifecycle failed: {error}"),
            Self::FinalDeliverable(error) => write!(
                formatter,
                "final deliverable exceeds its bounded response contract ({} of {} bytes)",
                error.actual_bytes, error.max_bytes,
            ),
            Self::EventChannelClosed => formatter.write_str("app-server event channel is closed"),
        }
    }
}

impl std::error::Error for ProductionOrchestrationTurnRunnerError {}

/// Inputs captured once for an internal production orchestration run.
pub(crate) struct ProductionOrchestrationTurnRunnerInput {
    pub builder: ProductionOrchestrationRequestBuilder,
    pub input: ProductionOrchestrationInput,
    pub policy_state: SessionExecutionPolicyState,
    pub dispatcher: ProductionRoleDispatcher,
    pub profiles: RoutingProfileRegistry,
    pub connections: RoutingConnectionDirectory,
    pub tool_adapter: ProductionApprovedToolAdapter,
    pub transcript_context: OrchestrationTranscriptContext,
}

/// Composes immutable request, role, tool, lifecycle, observation, and transcript state.
pub(crate) struct ProductionOrchestrationTurnRunner {
    request: codex_core::LiveOrchestrationRequest,
    policy_state: SessionExecutionPolicyState,
    dispatcher: ProductionRoleDispatcher,
    profiles: RoutingProfileRegistry,
    connections: RoutingConnectionDirectory,
    _tool_adapter: ProductionApprovedToolAdapter,
    transcript_context: OrchestrationTranscriptContext,
}

/// The result and transcript notifications produced by one completed internal run.
#[derive(Clone, Debug)]
pub(crate) struct ProductionOrchestrationTurnCompletion {
    pub result: OrchestrationTurnResult,
    pub notifications: Vec<ServerNotification>,
}

impl ProductionOrchestrationTurnRunner {
    /// Creates a runner from immutable, already trusted and validated state.
    pub(crate) fn new(
        runner_input: ProductionOrchestrationTurnRunnerInput,
    ) -> Result<Self, ProductionOrchestrationTurnRunnerError> {
        let ProductionOrchestrationTurnRunnerInput {
            builder,
            input,
            policy_state,
            dispatcher,
            profiles,
            connections,
            tool_adapter,
            transcript_context,
        } = runner_input;
        if input.run_id != transcript_context.turn_id {
            return Err(ProductionOrchestrationTurnRunnerError::InvalidInput(
                "turn_id",
            ));
        }
        if tool_adapter.workspace_root() != Some(input.workspace_root.as_path()) {
            return Err(ProductionOrchestrationTurnRunnerError::InvalidInput(
                "workspace_root",
            ));
        }
        for role in [
            RoutingRole::Main,
            RoutingRole::Planner,
            RoutingRole::Executor,
            RoutingRole::Verifier,
            RoutingRole::Repair,
        ] {
            if builder.policy().role(role).activation == RoleActivation::Disabled {
                continue;
            }
            let expected = builder
                .provider_route(role)
                .map_err(ProductionOrchestrationTurnRunnerError::Request)?;
            let actual = dispatcher
                .route(role)
                .map_err(ProductionOrchestrationTurnRunnerError::RouteDispatch)?;
            if actual != &expected {
                return Err(ProductionOrchestrationTurnRunnerError::RouteMismatch(role));
            }
        }
        let request = builder
            .build(input)
            .map_err(ProductionOrchestrationTurnRunnerError::Request)?;
        Ok(Self {
            request,
            policy_state,
            dispatcher,
            profiles,
            connections,
            _tool_adapter: tool_adapter,
            transcript_context,
        })
    }

    /// Runs the composed contracts and sends the final transcript notifications to the existing
    /// in-process app-server event channel. No production turn calls this method yet.
    pub(crate) async fn run(
        self,
        events: mpsc::Sender<AppServerEvent>,
    ) -> Result<ProductionOrchestrationTurnCompletion, ProductionOrchestrationTurnRunnerError> {
        let (observation_sink, observation_receiver) = codex_core::observation_channel();
        let coordinator =
            LiveOrchestrationCoordinator::new(self.dispatcher, self.profiles, self.connections)
                .with_observation_sink(observation_sink);
        let mut request = self.request;
        let policy_state = self.policy_state;
        let run_id = request.run_id.clone();
        let mut lifecycle =
            ProductionOrchestrationLifecycle::spawn(run_id, move |cancellation| async move {
                request.cancellation = cancellation;
                coordinator
                    .run(&policy_state, request)
                    .await
                    .map_err(CoordinatorError::Coordinator)
            });
        lifecycle.attach_observation_bridge(spawn_observation_bridge(
            observation_receiver,
            events.clone(),
        ));
        let outcome = lifecycle
            .complete()
            .await
            .map_err(ProductionOrchestrationTurnRunnerError::Lifecycle)?;
        let response = ProductionFinalDeliverableProducer::from_outcome(&outcome)
            .map_err(ProductionOrchestrationTurnRunnerError::FinalDeliverable)?;
        let result = OrchestrationTurnResultBuilder::build(&outcome, Some(response));
        let notifications = translate_orchestration_result(&result, &self.transcript_context);
        for notification in &notifications {
            events
                .send(AppServerEvent::ServerNotification(notification.clone()))
                .await
                .map_err(|_| ProductionOrchestrationTurnRunnerError::EventChannelClosed)?;
        }
        Ok(ProductionOrchestrationTurnCompletion {
            result,
            notifications,
        })
    }
}
