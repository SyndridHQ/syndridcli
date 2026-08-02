use super::ConnectionSessionState;
use super::MessageProcessor;
use super::MessageProcessorArgs;
use crate::config_manager::ConfigManager;
use crate::in_process::InProcessServerEvent;
use crate::outgoing_message::ConnectionId;
use crate::outgoing_message::OutgoingEnvelope;
use crate::outgoing_message::OutgoingMessage;
use crate::production_runner::PreparedProductionTurn;
use crate::production_runner::ProductionSessionRuntime;
use crate::production_runner::ProductionTurnAdmissionInput;
use crate::production_runner::ProductionTurnPreparationError;
use crate::production_runner::ProductionTurnRunError;
use crate::production_runner::ProductionTurnRunnerFactory;
use anyhow::Result;
use codex_analytics::AnalyticsEventsClient;
use codex_analytics::AppServerRpcTransport;
use codex_app_server_protocol::ClientInfo;
use codex_app_server_protocol::ClientRequest;
use codex_app_server_protocol::InitializeCapabilities;
use codex_app_server_protocol::InitializeParams;
use codex_app_server_protocol::RequestId;
use codex_app_server_protocol::ServerNotification;
use codex_app_server_protocol::ThreadStartParams;
use codex_app_server_protocol::ThreadStartResponse;
use codex_app_server_protocol::Turn;
use codex_app_server_protocol::TurnCompletedNotification;
use codex_app_server_protocol::TurnItemsView;
use codex_app_server_protocol::TurnStartParams;
use codex_app_server_protocol::TurnStartedNotification;
use codex_app_server_protocol::TurnStatus;
use codex_app_server_protocol::UserInput;
use codex_arg0::Arg0DispatchPaths;
use codex_config::CloudConfigBundleLoader;
use codex_config::LoaderOverrides;
use codex_config::NoopThreadConfigLoader;
use codex_core::config::ConfigBuilder;
use codex_exec_server::EnvironmentManager;
use codex_feedback::CodexFeedback;
use codex_login::AuthManager;
use codex_protocol::protocol::SessionSource;
use pretty_assertions::assert_eq;
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use tempfile::TempDir;
use tokio::sync::Mutex;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio::time::Duration;
use tokio::time::timeout;

const CONNECTION_ID: ConnectionId = ConnectionId(7);
const PRIVATE_PLANNER_SENTINEL: &str = "PRIVATE_PLANNER_SENTINEL";
const PRIVATE_TOOL_SENTINEL: &str = "PRIVATE_TOOL_SENTINEL";

#[derive(Clone, Default)]
struct FakeRuntimeState {
    calls: Arc<AtomicUsize>,
    admissions: Arc<Mutex<Vec<String>>>,
    internal_sentinels: Arc<Mutex<Vec<String>>>,
}

struct FakeActivationRunner {
    state: FakeRuntimeState,
}

impl ProductionTurnRunnerFactory for FakeActivationRunner {
    fn prepare(
        &self,
        input: ProductionTurnAdmissionInput,
        _context: Option<String>,
        events: mpsc::Sender<InProcessServerEvent>,
    ) -> Result<PreparedProductionTurn, ProductionTurnPreparationError> {
        self.state.calls.fetch_add(1, Ordering::SeqCst);
        let turn_id = input.turn_id().to_owned();
        let admission_id = turn_id.clone();
        let thread_id = input.thread_id().to_owned();
        let admissions = Arc::clone(&self.state.admissions);
        let internal_sentinels = Arc::clone(&self.state.internal_sentinels);
        let cancellation = codex_core::ProductionOrchestrationCancellationHandle::new();
        let notification = ServerNotification::TurnCompleted(TurnCompletedNotification {
            thread_id,
            turn: Turn {
                id: turn_id,
                items: Vec::new(),
                items_view: TurnItemsView::NotLoaded,
                status: TurnStatus::Completed,
                error: None,
                started_at: Some(1),
                completed_at: Some(2),
                duration_ms: Some(1),
            },
        });
        let completion = Box::pin(async move {
            admissions.lock().await.push(admission_id);
            internal_sentinels.lock().await.extend([
                PRIVATE_PLANNER_SENTINEL.to_owned(),
                PRIVATE_TOOL_SENTINEL.to_owned(),
            ]);
            events
                .send(InProcessServerEvent::ServerNotification(notification))
                .await
                .map_err(|_| ProductionTurnRunError::EventDestinationClosed)
        });
        Ok(PreparedProductionTurn::new(cancellation, completion))
    }
}

struct ActivationFixture {
    _codex_home: TempDir,
    processor: Arc<MessageProcessor>,
    session: Arc<ConnectionSessionState>,
    outbound_initialized: std::sync::atomic::AtomicBool,
    outgoing_rx: mpsc::Receiver<OutgoingEnvelope>,
    event_bridge: JoinHandle<()>,
    state: FakeRuntimeState,
    workspace: TempDir,
}

impl ActivationFixture {
    async fn new() -> Result<Self> {
        Self::new_with_runtime(true).await
    }

    async fn new_with_runtime(runtime_enabled: bool) -> Result<Self> {
        let codex_home = tempfile::tempdir()?;
        let workspace = tempfile::tempdir()?;
        let config = Arc::new(
            ConfigBuilder::default()
                .codex_home(codex_home.path().to_path_buf())
                .build()
                .await?,
        );
        let auth_manager = AuthManager::shared_from_config(
            config.as_ref(),
            /*enable_codex_api_key_env*/ false,
        )
        .await;
        let config_manager = ConfigManager::new(
            codex_home.path().to_path_buf(),
            Vec::new(),
            LoaderOverrides::default(),
            /*strict_config*/ false,
            CloudConfigBundleLoader::default(),
            Arg0DispatchPaths::default(),
            Arc::new(NoopThreadConfigLoader),
        );
        let analytics_events_client = AnalyticsEventsClient::disabled();
        let (outgoing_tx, outgoing_rx) = mpsc::channel(64);
        let outgoing = Arc::new(crate::outgoing_message::OutgoingMessageSender::new(
            outgoing_tx,
            analytics_events_client.clone(),
        ));
        let (event_tx, mut event_rx) = mpsc::channel(16);
        let bridge_outgoing = Arc::clone(&outgoing);
        let event_bridge = tokio::spawn(async move {
            while let Some(event) = event_rx.recv().await {
                if let InProcessServerEvent::ServerNotification(notification) = event {
                    bridge_outgoing.send_server_notification(notification).await;
                }
            }
        });
        let state = FakeRuntimeState::default();
        let runtime = runtime_enabled.then(|| {
            ProductionSessionRuntime::new(
                "activation-session".to_owned(),
                Arc::new(
                    crate::production_runner::ProductionOrchestrationRuntime::new(
                        Arc::new(FakeActivationRunner {
                            state: state.clone(),
                        }),
                        Arc::new(crate::ObjectiveOnlyProductionTurnContext),
                    ),
                ),
                event_tx,
            )
        });
        let processor = Arc::new(MessageProcessor::new(MessageProcessorArgs {
            outgoing,
            analytics_events_client,
            arg0_paths: Arg0DispatchPaths::default(),
            config,
            config_manager,
            environment_manager: Arc::new(EnvironmentManager::default_for_tests()),
            feedback: CodexFeedback::new(),
            log_db: None,
            state_db: None,
            config_warnings: Vec::new(),
            session_source: SessionSource::VSCode,
            auth_manager,
            installation_id: "11111111-1111-4111-8111-111111111111".to_owned(),
            rpc_transport: AppServerRpcTransport::Stdio,
            remote_control_handle: None,
            plugin_startup_tasks: crate::PluginStartupTasks::Start,
            production_execution_capability:
                crate::ProductionExecutionCapability::SyndridOrchestration,
            production_session_runtime: runtime.map(Arc::new),
        }));
        Ok(Self {
            _codex_home: codex_home,
            processor,
            session: Arc::new(ConnectionSessionState::new()),
            outbound_initialized: std::sync::atomic::AtomicBool::new(false),
            outgoing_rx,
            event_bridge,
            state,
            workspace,
        })
    }

    async fn request_messages(&mut self, request: ClientRequest) -> Vec<OutgoingMessage> {
        let request_id = request.id().clone();
        self.processor
            .process_client_request(
                CONNECTION_ID,
                request,
                Arc::clone(&self.session),
                &self.outbound_initialized,
            )
            .await;
        let mut messages = Vec::new();
        loop {
            let envelope = timeout(Duration::from_secs(2), self.outgoing_rx.recv())
                .await
                .expect("app-server response should arrive before timeout")
                .expect("outgoing channel should remain open");
            let message = match envelope {
                OutgoingEnvelope::ToConnection { message, .. }
                | OutgoingEnvelope::Broadcast { message } => message,
            };
            let response_received = matches!(
                &message,
                OutgoingMessage::Response(response) if response.id == request_id
            ) || matches!(
                &message,
                OutgoingMessage::Error(error) if error.id == request_id
            );
            messages.push(message);
            if response_received {
                return messages;
            }
        }
    }

    async fn initialize_and_start_thread(&mut self) -> Result<String> {
        let _ = self
            .request_messages(ClientRequest::Initialize {
                request_id: RequestId::Integer(1),
                params: InitializeParams {
                    client_info: ClientInfo {
                        name: "activation-harness".to_owned(),
                        title: None,
                        version: "0.1.0".to_owned(),
                    },
                    capabilities: Some(InitializeCapabilities {
                        experimental_api: true,
                        ..Default::default()
                    }),
                },
            })
            .await;
        let messages = self
            .request_messages(ClientRequest::ThreadStart {
                request_id: RequestId::Integer(2),
                params: ThreadStartParams {
                    ephemeral: Some(true),
                    ..Default::default()
                },
            })
            .await;
        let response = response_payload::<ThreadStartResponse>(&messages, 2);
        Ok(response.thread.id)
    }

    async fn start_turn(&mut self, request_id: i64, thread_id: &str) -> Vec<OutgoingMessage> {
        self.request_messages(ClientRequest::TurnStart {
            request_id: RequestId::Integer(request_id),
            params: TurnStartParams {
                thread_id: thread_id.to_owned(),
                input: vec![UserInput::Text {
                    text: format!("activation objective {request_id}"),
                    text_elements: Vec::new(),
                }],
                cwd: Some(self.workspace.path().to_path_buf()),
                ..Default::default()
            },
        })
        .await
    }

    async fn finish(self) {
        self.processor.shutdown_threads().await;
        self.processor.drain_background_tasks().await;
        drop(self.processor);
        self.event_bridge.abort();
        let _ = self.event_bridge.await;
    }
}

fn response_payload<T: serde::de::DeserializeOwned>(
    messages: &[OutgoingMessage],
    request_id: i64,
) -> T {
    messages
        .iter()
        .find_map(|message| match message {
            OutgoingMessage::Response(response)
                if response.id == RequestId::Integer(request_id) =>
            {
                Some(serde_json::from_value(response.result.clone()).expect("valid response"))
            }
            _ => None,
        })
        .expect("response should be present")
}

fn notifications(messages: &[OutgoingMessage]) -> Vec<ServerNotification> {
    messages
        .iter()
        .filter_map(|message| match message {
            OutgoingMessage::AppServerNotification(envelope) => Some(envelope.notification.clone()),
            _ => None,
        })
        .collect()
}

async fn collect_until_terminal(
    receiver: &mut mpsc::Receiver<OutgoingEnvelope>,
    mut messages: Vec<OutgoingMessage>,
) -> Vec<OutgoingMessage> {
    loop {
        let envelope = timeout(Duration::from_secs(2), receiver.recv())
            .await
            .expect("terminal notification should arrive before timeout")
            .expect("outgoing channel should remain open");
        let message = match envelope {
            OutgoingEnvelope::ToConnection { message, .. }
            | OutgoingEnvelope::Broadcast { message } => message,
        };
        let terminal = matches!(
            &message,
            OutgoingMessage::AppServerNotification(envelope)
                if matches!(envelope.notification, ServerNotification::TurnCompleted(_))
        );
        messages.push(message);
        if terminal {
            return messages;
        }
    }
}

#[test]
fn explicit_syndrid_turn_invokes_runtime_and_publishes_terminal_result() -> Result<()> {
    run_on_large_stack(|| {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?
            .block_on(explicit_syndrid_turn_invokes_runtime())
    })
}

async fn explicit_syndrid_turn_invokes_runtime() -> Result<()> {
    let mut fixture = ActivationFixture::new().await?;
    let thread_id = fixture.initialize_and_start_thread().await?;

    let first_messages = fixture.start_turn(3, &thread_id).await;
    let first_messages = collect_until_terminal(&mut fixture.outgoing_rx, first_messages).await;
    let first_notifications = notifications(&first_messages);
    assert_eq!(
        first_notifications
            .iter()
            .filter(|notification| matches!(notification, ServerNotification::TurnStarted(_)))
            .count(),
        1
    );
    assert_eq!(
        first_notifications
            .iter()
            .filter(|notification| matches!(notification, ServerNotification::TurnCompleted(_)))
            .count(),
        1
    );
    let first_turn_id = first_notifications
        .iter()
        .find_map(|notification| match notification {
            ServerNotification::TurnStarted(TurnStartedNotification { turn, .. }) => {
                Some(turn.id.clone())
            }
            _ => None,
        })
        .expect("first production admission should be notified");

    let stale_interrupt = fixture
        .request_messages(ClientRequest::TurnInterrupt {
            request_id: RequestId::Integer(4),
            params: codex_app_server_protocol::TurnInterruptParams {
                thread_id: thread_id.clone(),
                turn_id: first_turn_id.clone(),
            },
        })
        .await;
    assert!(stale_interrupt.iter().any(|message| matches!(
        message,
        OutgoingMessage::Error(error) if error.error.message.contains("no active turn")
    )));

    let second_messages = fixture.start_turn(5, &thread_id).await;
    let second_messages = collect_until_terminal(&mut fixture.outgoing_rx, second_messages).await;
    let second_notifications = notifications(&second_messages);
    assert_eq!(
        second_notifications
            .iter()
            .filter(|notification| matches!(notification, ServerNotification::TurnStarted(_)))
            .count(),
        1
    );
    assert_eq!(
        second_notifications
            .iter()
            .filter(|notification| matches!(notification, ServerNotification::TurnCompleted(_)))
            .count(),
        1
    );
    let second_turn_id = second_notifications
        .iter()
        .find_map(|notification| match notification {
            ServerNotification::TurnStarted(TurnStartedNotification { turn, .. }) => {
                Some(turn.id.clone())
            }
            _ => None,
        })
        .expect("second production admission should be notified");
    assert_ne!(first_turn_id, second_turn_id);
    assert_eq!(fixture.state.calls.load(Ordering::SeqCst), 2);
    assert_eq!(fixture.state.admissions.lock().await.len(), 2);
    assert_eq!(fixture.state.internal_sentinels.lock().await.len(), 4);
    assert!(!first_messages.iter().any(|message| {
        serde_json::to_string(message)
            .expect("notification should serialize")
            .contains(PRIVATE_PLANNER_SENTINEL)
    }));
    assert!(!second_messages.iter().any(|message| {
        serde_json::to_string(message)
            .expect("notification should serialize")
            .contains(PRIVATE_TOOL_SENTINEL)
    }));

    fixture.finish().await;
    Ok(())
}

#[test]
fn explicit_syndrid_without_runtime_stays_unavailable() -> Result<()> {
    run_on_large_stack(|| {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?
            .block_on(explicit_syndrid_without_runtime())
    })
}

async fn explicit_syndrid_without_runtime() -> Result<()> {
    let mut fixture = ActivationFixture::new_with_runtime(false).await?;
    let thread_id = fixture.initialize_and_start_thread().await?;

    let messages = fixture.start_turn(3, &thread_id).await;
    assert!(messages.iter().any(|message| matches!(
        message,
        OutgoingMessage::Error(error) if error.error.message.contains("unavailable")
    )));
    assert_eq!(fixture.state.calls.load(Ordering::SeqCst), 0);
    let notifications = notifications(&messages);
    assert!(!notifications.iter().any(|notification| matches!(
        notification,
        ServerNotification::TurnStarted(_) | ServerNotification::TurnCompleted(_)
    )));

    fixture.finish().await;
    Ok(())
}

fn run_on_large_stack(test: impl FnOnce() -> Result<()> + Send + 'static) -> Result<()> {
    std::thread::Builder::new()
        .name("syndrid-activation-test".to_owned())
        .stack_size(16 * 1024 * 1024)
        .spawn(test)
        .expect("activation test thread should start")
        .join()
        .expect("activation test thread should not panic")
}
