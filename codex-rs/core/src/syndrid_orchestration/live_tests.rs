use super::TerminalSnapshot;
use super::live::SequentialRunner;
use super::live::SequentialRuntime;
use super::live::StageAssignment;
use super::live::bounded_stage_output;
use codex_orchestration::AgentId;
use codex_orchestration::AgentRole;
use codex_orchestration::BoundedText;
use codex_orchestration::DataQuality;
use codex_orchestration::EffortRoute;
use codex_orchestration::ForecastConfidence;
use codex_orchestration::ModelRoute;
use codex_orchestration::PermissionEnvelope;
use codex_orchestration::RouteSource;
use codex_orchestration::RouteStatus;
use codex_orchestration::SequentialWorkflow;
use codex_orchestration::StageCorrelation;
use codex_orchestration::StageInput;
use codex_orchestration::StructuredHandoff;
use codex_orchestration::TaskId;
use codex_orchestration::WorkAccess;
use codex_orchestration::WorkflowId;
use codex_orchestration_adapter::AdapterError;
use codex_orchestration_adapter::AdapterErrorKind;
use codex_orchestration_adapter::Retryability;
use codex_orchestration_adapter::RuntimeAgentId;
use codex_orchestration_adapter::SpawnChildRequest;
use codex_orchestration_adapter::SpawnChildResult;
use codex_protocol::openai_models::ReasoningEffort;
use codex_protocol::protocol::AgentStatus;
use pretty_assertions::assert_eq;
use std::sync::Mutex;

fn workflow() -> SequentialWorkflow {
    SequentialWorkflow::new(
        WorkflowId::new("workflow-1").expect("workflow id"),
        TaskId::new("task-1").expect("task id"),
        PermissionEnvelope::new(WorkAccess::Writer, WorkAccess::Writer, WorkAccess::Writer)
            .expect("permission ceiling"),
    )
    .expect("fixed stages")
}

fn handoff() -> StructuredHandoff {
    StructuredHandoff::new(
        WorkflowId::new("workflow-1").expect("workflow id"),
        TaskId::new("task-1").expect("task id"),
        AgentId::new("root-agent").expect("agent id"),
        AgentRole::Planner,
        bounded("initial task"),
        bounded("objective"),
        bounded("scope"),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        ForecastConfidence::High,
        Vec::new(),
        bounded("plan"),
        Vec::new(),
        DataQuality::Exact,
    )
}

fn bounded(value: &str) -> BoundedText {
    BoundedText::new(value).expect("bounded text")
}

fn route() -> (ModelRoute, EffortRoute) {
    (
        ModelRoute {
            requested: Some("model".to_string()),
            resolved: Some("model".to_string()),
            source: RouteSource::Policy,
            status: RouteStatus::Resolved,
            data_quality: DataQuality::Derived,
        },
        EffortRoute {
            requested: Some(ReasoningEffort::Low),
            resolved: Some(ReasoningEffort::Low),
            source: RouteSource::Policy,
            status: RouteStatus::Resolved,
            data_quality: DataQuality::Derived,
        },
    )
}

fn assignment(role: AgentRole, access: WorkAccess, name: &str) -> StageAssignment {
    let (model_route, effort_route) = route();
    StageAssignment {
        agent_id: AgentId::new(name).expect("agent id"),
        role,
        access,
        permissions: PermissionEnvelope::new(WorkAccess::Writer, WorkAccess::Writer, access)
            .expect("permissions"),
        model_route,
        effort_route,
    }
}

fn planner_input() -> StageInput {
    StageInput {
        correlation: StageCorrelation {
            workflow_id: WorkflowId::new("workflow-1").expect("workflow id"),
            task_id: TaskId::new("task-1").expect("task id"),
            stage_id: codex_orchestration::StageId::new("planner").expect("stage id"),
        },
        role: AgentRole::Planner,
        access: WorkAccess::ReadOnly,
        permissions: PermissionEnvelope::new(
            WorkAccess::Writer,
            WorkAccess::Writer,
            WorkAccess::ReadOnly,
        )
        .expect("permissions"),
        handoff: handoff(),
    }
}

fn assignments() -> [StageAssignment; 5] {
    [
        assignment(AgentRole::Planner, WorkAccess::ReadOnly, "planner-agent"),
        assignment(AgentRole::Executor, WorkAccess::Writer, "executor-agent"),
        assignment(AgentRole::Verifier, WorkAccess::ReadOnly, "verifier-agent"),
        assignment(AgentRole::Executor, WorkAccess::Writer, "repair-agent"),
        assignment(
            AgentRole::Verifier,
            WorkAccess::ReadOnly,
            "final-verifier-agent",
        ),
    ]
}

fn runtime_id(value: &str) -> RuntimeAgentId {
    RuntimeAgentId::new(value).expect("runtime id")
}

fn adapter_error() -> AdapterError {
    AdapterError::new(
        AdapterErrorKind::RuntimeUnavailable,
        bounded("runtime failure"),
        Retryability::NotRetryable,
        DataQuality::Exact,
    )
}

struct FakeRuntime {
    spawn_results: Mutex<Vec<Result<SpawnChildResult, AdapterError>>>,
    terminal_results: Mutex<Vec<Result<TerminalSnapshot, AdapterError>>>,
    requests: Mutex<Vec<SpawnChildRequest>>,
    events: Mutex<Vec<&'static str>>,
}

impl FakeRuntime {
    fn new(
        spawn_results: Vec<Result<SpawnChildResult, AdapterError>>,
        terminal_results: Vec<Result<TerminalSnapshot, AdapterError>>,
    ) -> Self {
        Self {
            spawn_results: Mutex::new(spawn_results),
            terminal_results: Mutex::new(terminal_results),
            requests: Mutex::new(Vec::new()),
            events: Mutex::new(Vec::new()),
        }
    }
}

impl SequentialRuntime for FakeRuntime {
    fn spawn_child<'a>(
        &'a self,
        request: SpawnChildRequest,
    ) -> super::live::RuntimeFuture<'a, SpawnChildResult> {
        self.requests.lock().expect("requests lock").push(request);
        self.events.lock().expect("events lock").push("spawn");
        let result = self.spawn_results.lock().expect("spawn lock").remove(0);
        Box::pin(async move { result })
    }

    fn wait_for_terminal<'a>(
        &'a self,
        _runtime_id: RuntimeAgentId,
        _workflow_id: &'a WorkflowId,
        _task_id: &'a TaskId,
        _agent_id: &'a AgentId,
    ) -> super::live::RuntimeFuture<'a, TerminalSnapshot> {
        self.events.lock().expect("events lock").push("wait");
        let result = self
            .terminal_results
            .lock()
            .expect("terminal lock")
            .remove(0);
        Box::pin(async move { result })
    }
}

fn spawn_result(agent_id: &str, runtime_name: &str) -> SpawnChildResult {
    SpawnChildResult {
        workflow_id: WorkflowId::new("workflow-1").expect("workflow id"),
        task_id: TaskId::new("task-1").expect("task id"),
        agent_id: AgentId::new(agent_id).expect("agent id"),
        runtime_id: runtime_id(runtime_name),
    }
}

fn snapshot(runtime_name: &str, status: AgentStatus) -> TerminalSnapshot {
    TerminalSnapshot {
        runtime_id: runtime_id(runtime_name),
        status,
    }
}

fn successful_runtime() -> FakeRuntime {
    FakeRuntime::new(
        vec![
            Ok(spawn_result("planner-agent", "planner-runtime")),
            Ok(spawn_result("executor-agent", "executor-runtime")),
            Ok(spawn_result("verifier-agent", "verifier-runtime")),
            Ok(spawn_result("repair-agent", "repair-runtime")),
            Ok(spawn_result(
                "final-verifier-agent",
                "final-verifier-runtime",
            )),
        ],
        vec![
            Ok(snapshot(
                "planner-runtime",
                AgentStatus::Completed(Some("plan".to_string())),
            )),
            Ok(snapshot(
                "executor-runtime",
                AgentStatus::Completed(Some("done".to_string())),
            )),
            Ok(snapshot(
                "verifier-runtime",
                AgentStatus::Completed(Some("ACCEPT".to_string())),
            )),
            Ok(snapshot(
                "repair-runtime",
                AgentStatus::Completed(Some("repaired".to_string())),
            )),
            Ok(snapshot(
                "final-verifier-runtime",
                AgentStatus::Completed(Some("ACCEPT".to_string())),
            )),
        ],
    )
}

fn repair_runtime(final_status: AgentStatus) -> FakeRuntime {
    FakeRuntime::new(
        vec![
            Ok(spawn_result("planner-agent", "planner-runtime")),
            Ok(spawn_result("executor-agent", "executor-runtime")),
            Ok(spawn_result("verifier-agent", "verifier-runtime")),
            Ok(spawn_result("repair-agent", "repair-runtime")),
            Ok(spawn_result(
                "final-verifier-agent",
                "final-verifier-runtime",
            )),
        ],
        vec![
            Ok(snapshot(
                "planner-runtime",
                AgentStatus::Completed(Some("plan".to_string())),
            )),
            Ok(snapshot(
                "executor-runtime",
                AgentStatus::Completed(Some("done".to_string())),
            )),
            Ok(snapshot(
                "verifier-runtime",
                AgentStatus::Completed(Some("REJECT\nfix issue".to_string())),
            )),
            Ok(snapshot(
                "repair-runtime",
                AgentStatus::Completed(Some("repaired".to_string())),
            )),
            Ok(snapshot("final-verifier-runtime", final_status)),
        ],
    )
}

async fn run(runtime: &FakeRuntime) -> SequentialWorkflow {
    let mut runner = SequentialRunner::new(runtime, workflow());
    runner.run(planner_input(), assignments()).await
}

#[tokio::test]
async fn sequential_workflow_is_strict_and_has_one_writer() {
    let runtime = successful_runtime();
    let mut runner = SequentialRunner::new(&runtime, workflow());
    let final_workflow = runner.run(planner_input(), assignments()).await;
    assert_eq!(
        final_workflow.state(),
        codex_orchestration::SequentialWorkflowState::Succeeded
    );
    assert_eq!(runtime.requests.lock().expect("requests lock").len(), 3);
    let requests = runtime.requests.lock().expect("requests lock");
    assert_eq!(requests[0].access(), WorkAccess::ReadOnly);
    assert_eq!(requests[1].access(), WorkAccess::Writer);
    assert_eq!(requests[2].access(), WorkAccess::ReadOnly);
    assert_eq!(
        runtime.events.lock().expect("events lock").as_slice(),
        ["spawn", "wait", "spawn", "wait", "spawn", "wait"]
    );
}

#[test]
fn terminal_output_policy_rejects_missing_empty_oversized_and_failed_results() {
    let assignment = assignment(AgentRole::Planner, WorkAccess::ReadOnly, "planner-agent");
    let correlation = planner_input().correlation;
    for status in [
        AgentStatus::Completed(None),
        AgentStatus::Completed(Some(" \n\t".to_string())),
        AgentStatus::Errored("native error".to_string()),
        AgentStatus::Shutdown,
        AgentStatus::NotFound,
        AgentStatus::Interrupted,
    ] {
        assert!(
            bounded_stage_output(
                snapshot("runtime", status),
                correlation.clone(),
                &assignment,
                None,
            )
            .is_err()
        );
    }
    let oversized = "x".repeat(codex_orchestration::MAX_HANDOFF_TEXT_BYTES + 1);
    assert!(
        bounded_stage_output(
            snapshot("runtime", AgentStatus::Completed(Some(oversized))),
            correlation,
            &assignment,
            None,
        )
        .is_err()
    );
}

#[tokio::test]
async fn spawn_failure_completes_active_stage_and_stops() {
    let runtime = FakeRuntime::new(vec![Err(adapter_error())], Vec::new());
    let mut runner = SequentialRunner::new(&runtime, workflow());
    let result = runner.run(planner_input(), assignments()).await;
    assert_eq!(
        result.state(),
        codex_orchestration::SequentialWorkflowState::Failed
    );
    assert_eq!(result.active_stage(), None);
    assert_eq!(runtime.requests.lock().expect("requests lock").len(), 1);
}

#[tokio::test]
async fn terminal_wait_failure_completes_active_stage_and_stops() {
    let runtime = FakeRuntime::new(
        vec![Ok(spawn_result("planner-agent", "planner-runtime"))],
        vec![Err(adapter_error())],
    );
    let mut runner = SequentialRunner::new(&runtime, workflow());
    let result = runner.run(planner_input(), assignments()).await;
    assert_eq!(
        result.state(),
        codex_orchestration::SequentialWorkflowState::Failed
    );
    assert_eq!(result.active_stage(), None);
    assert_eq!(runtime.requests.lock().expect("requests lock").len(), 1);
}

#[tokio::test]
async fn mismatched_runtime_and_spawn_correlation_fail_without_later_spawns() {
    let runtime = FakeRuntime::new(
        vec![Ok(SpawnChildResult {
            workflow_id: WorkflowId::new("other-workflow").expect("workflow id"),
            task_id: TaskId::new("task-1").expect("task id"),
            agent_id: AgentId::new("planner-agent").expect("agent id"),
            runtime_id: runtime_id("planner-runtime"),
        })],
        Vec::new(),
    );
    let mut runner = SequentialRunner::new(&runtime, workflow());
    let result = runner.run(planner_input(), assignments()).await;
    assert_eq!(
        result.state(),
        codex_orchestration::SequentialWorkflowState::Failed
    );
    assert_eq!(runtime.requests.lock().expect("requests lock").len(), 1);
}

#[tokio::test]
async fn terminal_result_runtime_mismatch_is_rejected() {
    let runtime = FakeRuntime::new(
        vec![Ok(spawn_result("planner-agent", "planner-runtime"))],
        vec![Ok(snapshot(
            "stale-runtime",
            AgentStatus::Completed(Some("plan".to_string())),
        ))],
    );
    let mut runner = SequentialRunner::new(&runtime, workflow());
    let result = runner.run(planner_input(), assignments()).await;
    assert_eq!(
        result.state(),
        codex_orchestration::SequentialWorkflowState::Failed
    );
    assert_eq!(runtime.requests.lock().expect("requests lock").len(), 1);
}

#[tokio::test]
async fn verifier_rejection_runs_exactly_one_repair_cycle() {
    let runtime = repair_runtime(AgentStatus::Completed(Some("ACCEPT".to_string())));
    let result = run(&runtime).await;
    assert_eq!(
        result.state(),
        codex_orchestration::SequentialWorkflowState::Succeeded
    );
    assert_eq!(runtime.requests.lock().expect("requests lock").len(), 5);
    assert_eq!(
        runtime.events.lock().expect("events lock").as_slice(),
        [
            "spawn", "wait", "spawn", "wait", "spawn", "wait", "spawn", "wait", "spawn", "wait"
        ]
    );
    let requests = runtime.requests.lock().expect("requests lock");
    assert_eq!(requests[0].access(), WorkAccess::ReadOnly);
    assert_eq!(requests[1].access(), WorkAccess::Writer);
    assert_eq!(requests[2].access(), WorkAccess::ReadOnly);
    assert_eq!(requests[3].access(), WorkAccess::Writer);
    assert_eq!(requests[4].access(), WorkAccess::ReadOnly);
}

#[tokio::test]
async fn final_verifier_rejection_is_terminal() {
    let runtime = repair_runtime(AgentStatus::Completed(Some(
        "REJECT\nstill broken".to_string(),
    )));
    let result = run(&runtime).await;
    assert_eq!(
        result.state(),
        codex_orchestration::SequentialWorkflowState::Failed
    );
    assert_eq!(result.active_stage(), None);
    assert_eq!(runtime.requests.lock().expect("requests lock").len(), 5);
}

#[tokio::test]
async fn malformed_repair_feedback_stops_before_repair_spawn() {
    let runtime = FakeRuntime::new(
        vec![
            Ok(spawn_result("planner-agent", "planner-runtime")),
            Ok(spawn_result("executor-agent", "executor-runtime")),
            Ok(spawn_result("verifier-agent", "verifier-runtime")),
        ],
        vec![
            Ok(snapshot(
                "planner-runtime",
                AgentStatus::Completed(Some("plan".to_string())),
            )),
            Ok(snapshot(
                "executor-runtime",
                AgentStatus::Completed(Some("done".to_string())),
            )),
            Ok(snapshot(
                "verifier-runtime",
                AgentStatus::Completed(Some("REJECT".to_string())),
            )),
        ],
    );
    let result = run(&runtime).await;
    assert_eq!(
        result.state(),
        codex_orchestration::SequentialWorkflowState::Failed
    );
    assert_eq!(result.active_stage(), None);
    assert_eq!(runtime.requests.lock().expect("requests lock").len(), 3);
}

#[tokio::test]
async fn oversized_repair_feedback_stops_before_repair_spawn() {
    let feedback = "x".repeat(codex_orchestration::MAX_HANDOFF_TEXT_BYTES + 1);
    let runtime = FakeRuntime::new(
        vec![
            Ok(spawn_result("planner-agent", "planner-runtime")),
            Ok(spawn_result("executor-agent", "executor-runtime")),
            Ok(spawn_result("verifier-agent", "verifier-runtime")),
        ],
        vec![
            Ok(snapshot(
                "planner-runtime",
                AgentStatus::Completed(Some("plan".to_string())),
            )),
            Ok(snapshot(
                "executor-runtime",
                AgentStatus::Completed(Some("done".to_string())),
            )),
            Ok(snapshot(
                "verifier-runtime",
                AgentStatus::Completed(Some(format!("REJECT\n{feedback}"))),
            )),
        ],
    );
    let result = run(&runtime).await;
    assert_eq!(
        result.state(),
        codex_orchestration::SequentialWorkflowState::Failed
    );
    assert_eq!(runtime.requests.lock().expect("requests lock").len(), 3);
}

#[tokio::test]
async fn repair_spawn_failure_stops_without_final_verifier() {
    let runtime = FakeRuntime::new(
        vec![
            Ok(spawn_result("planner-agent", "planner-runtime")),
            Ok(spawn_result("executor-agent", "executor-runtime")),
            Ok(spawn_result("verifier-agent", "verifier-runtime")),
            Err(adapter_error()),
        ],
        vec![
            Ok(snapshot(
                "planner-runtime",
                AgentStatus::Completed(Some("plan".to_string())),
            )),
            Ok(snapshot(
                "executor-runtime",
                AgentStatus::Completed(Some("done".to_string())),
            )),
            Ok(snapshot(
                "verifier-runtime",
                AgentStatus::Completed(Some("REJECT\nfix".to_string())),
            )),
        ],
    );
    let result = run(&runtime).await;
    assert_eq!(
        result.state(),
        codex_orchestration::SequentialWorkflowState::Failed
    );
    assert_eq!(result.active_stage(), None);
    assert_eq!(runtime.requests.lock().expect("requests lock").len(), 4);
}

#[tokio::test]
async fn repair_wait_failure_stops_without_final_verifier() {
    let runtime = FakeRuntime::new(
        vec![
            Ok(spawn_result("planner-agent", "planner-runtime")),
            Ok(spawn_result("executor-agent", "executor-runtime")),
            Ok(spawn_result("verifier-agent", "verifier-runtime")),
            Ok(spawn_result("repair-agent", "repair-runtime")),
        ],
        vec![
            Ok(snapshot(
                "planner-runtime",
                AgentStatus::Completed(Some("plan".to_string())),
            )),
            Ok(snapshot(
                "executor-runtime",
                AgentStatus::Completed(Some("done".to_string())),
            )),
            Ok(snapshot(
                "verifier-runtime",
                AgentStatus::Completed(Some("REJECT\nfix".to_string())),
            )),
            Err(adapter_error()),
        ],
    );
    let result = run(&runtime).await;
    assert_eq!(
        result.state(),
        codex_orchestration::SequentialWorkflowState::Failed
    );
    assert_eq!(result.active_stage(), None);
    assert_eq!(runtime.requests.lock().expect("requests lock").len(), 4);
}

#[tokio::test]
async fn final_verifier_spawn_failure_is_terminal() {
    let runtime = repair_runtime(AgentStatus::Completed(Some("ACCEPT".to_string())));
    runtime.spawn_results.lock().expect("spawn lock").pop();
    runtime
        .spawn_results
        .lock()
        .expect("spawn lock")
        .push(Err(adapter_error()));
    let result = run(&runtime).await;
    assert_eq!(
        result.state(),
        codex_orchestration::SequentialWorkflowState::Failed
    );
    assert_eq!(result.active_stage(), None);
    assert_eq!(runtime.requests.lock().expect("requests lock").len(), 5);
}

#[tokio::test]
async fn final_verifier_wait_failure_is_terminal() {
    let runtime = repair_runtime(AgentStatus::Completed(Some("ACCEPT".to_string())));
    runtime
        .terminal_results
        .lock()
        .expect("terminal lock")
        .pop();
    runtime
        .terminal_results
        .lock()
        .expect("terminal lock")
        .push(Err(adapter_error()));
    let result = run(&runtime).await;
    assert_eq!(
        result.state(),
        codex_orchestration::SequentialWorkflowState::Failed
    );
    assert_eq!(result.active_stage(), None);
    assert_eq!(runtime.requests.lock().expect("requests lock").len(), 5);
}

#[tokio::test]
async fn malformed_repair_output_stops_before_final_verifier() {
    let runtime = repair_runtime(AgentStatus::Completed(Some("ACCEPT".to_string())));
    let mut terminals = runtime.terminal_results.lock().expect("terminal lock");
    terminals[3] = Ok(snapshot("repair-runtime", AgentStatus::Completed(None)));
    drop(terminals);
    let result = run(&runtime).await;
    assert_eq!(
        result.state(),
        codex_orchestration::SequentialWorkflowState::Failed
    );
    assert_eq!(result.active_stage(), None);
    assert_eq!(runtime.requests.lock().expect("requests lock").len(), 4);
}

#[test]
fn repair_output_policy_rejects_missing_empty_oversized_and_failed_results() {
    let repair = assignment(AgentRole::Executor, WorkAccess::Writer, "repair-agent");
    let correlation = StageCorrelation {
        workflow_id: WorkflowId::new("workflow-1").expect("workflow id"),
        task_id: TaskId::new("task-1").expect("task id"),
        stage_id: codex_orchestration::StageId::new("repair_executor").expect("stage id"),
    };
    for status in [
        AgentStatus::Completed(None),
        AgentStatus::Completed(Some(" \n\t".to_string())),
        AgentStatus::Errored("native error".to_string()),
    ] {
        assert!(
            bounded_stage_output(
                snapshot("repair-runtime", status),
                correlation.clone(),
                &repair,
                None,
            )
            .is_err()
        );
    }
    let oversized = "x".repeat(codex_orchestration::MAX_HANDOFF_TEXT_BYTES + 1);
    assert!(
        bounded_stage_output(
            snapshot("repair-runtime", AgentStatus::Completed(Some(oversized))),
            correlation,
            &repair,
            None,
        )
        .is_err()
    );
}

#[tokio::test]
async fn repair_correlation_mismatch_stops_immediately() {
    let runtime = repair_runtime(AgentStatus::Completed(Some("ACCEPT".to_string())));
    let mut spawns = runtime.spawn_results.lock().expect("spawn lock");
    spawns[3] = Ok(SpawnChildResult {
        workflow_id: WorkflowId::new("wrong-workflow").expect("workflow id"),
        task_id: TaskId::new("task-1").expect("task id"),
        agent_id: AgentId::new("repair-agent").expect("agent id"),
        runtime_id: runtime_id("repair-runtime"),
    });
    drop(spawns);
    let result = run(&runtime).await;
    assert_eq!(
        result.state(),
        codex_orchestration::SequentialWorkflowState::Failed
    );
    assert_eq!(result.active_stage(), None);
    assert_eq!(runtime.requests.lock().expect("requests lock").len(), 4);
}
