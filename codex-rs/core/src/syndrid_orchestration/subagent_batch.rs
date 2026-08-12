use super::subagent::SubagentError;
use super::subagent::SubagentOutcome;
use super::subagent::SubagentProvider;
use super::subagent::SubagentRequest;
use super::subagent::SubagentRuntime;
use super::subagent::SubagentStatus;
use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;
use tokio::sync::Mutex;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

pub const SUBAGENT_BATCH_MAX_CONCURRENCY: usize = 2;
pub const SUBAGENT_BATCH_DEFAULT_MAX_TASKS: usize = 8;
pub const SUBAGENT_BATCH_MAX_COMPLETION_AUDIT: usize = 128;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SubagentFailurePolicy {
    ContinueIndependent,
    CancelRemaining,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SubagentResultOrdering {
    InputOrder,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SubagentConcurrencyPolicy {
    pub max_tasks: usize,
    pub max_concurrency: usize,
    pub batch_timeout: Duration,
    pub max_provider_turns: usize,
    pub max_tool_calls: usize,
    pub max_tool_output_bytes: usize,
    pub failure_policy: SubagentFailurePolicy,
    pub result_ordering: SubagentResultOrdering,
}

impl Default for SubagentConcurrencyPolicy {
    fn default() -> Self {
        Self {
            max_tasks: SUBAGENT_BATCH_DEFAULT_MAX_TASKS,
            max_concurrency: 1,
            batch_timeout: Duration::from_secs(120),
            max_provider_turns: 64,
            max_tool_calls: 128,
            max_tool_output_bytes: 1024 * 1024,
            failure_policy: SubagentFailurePolicy::ContinueIndependent,
            result_ordering: SubagentResultOrdering::InputOrder,
        }
    }
}

#[derive(Clone)]
pub struct SubagentTask {
    pub request: SubagentRequest,
    pub timeout_override: Option<Duration>,
}

impl fmt::Debug for SubagentTask {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SubagentTask")
            .field("task_id_bytes", &self.request.task_id.len())
            .field("timeout_override", &self.timeout_override)
            .finish()
    }
}

pub struct SubagentBatchRequest {
    pub tasks: Vec<SubagentTask>,
    pub policy: SubagentConcurrencyPolicy,
    pub cancellation: CancellationToken,
}

impl fmt::Debug for SubagentBatchRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SubagentBatchRequest")
            .field("task_count", &self.tasks.len())
            .field("policy", &self.policy)
            .field("cancellation_cancelled", &self.cancellation.is_cancelled())
            .finish()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SubagentTaskState {
    Queued,
    Running,
    Completed,
    Failed,
    Cancelled,
    TimedOut,
    BudgetExhausted,
    NotStarted,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SubagentBatchStatus {
    Created,
    Validating,
    Running,
    Cancelling,
    Completed,
    PartiallyFailed,
    Failed,
    Cancelled,
    TimedOut,
    BudgetExhausted,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SubagentTaskOutcome {
    pub task_id: String,
    pub state: SubagentTaskState,
    pub outcome: Option<SubagentOutcome>,
    pub error: Option<SubagentError>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SubagentBatchOutcome {
    pub status: SubagentBatchStatus,
    pub total_task_count: usize,
    pub started_task_count: usize,
    pub completed_task_count: usize,
    pub failed_task_count: usize,
    pub cancelled_task_count: usize,
    pub timed_out_task_count: usize,
    pub not_started_task_count: usize,
    pub peak_observed_concurrency: usize,
    pub configured_concurrency: usize,
    pub aggregate_provider_turns: usize,
    pub aggregate_tool_calls: usize,
    pub aggregate_tool_output_bytes: usize,
    pub elapsed: Duration,
    pub tasks: Vec<SubagentTaskOutcome>,
    pub completion_audit: Vec<String>,
    pub failure_policy: SubagentFailurePolicy,
    pub budget_exhausted: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SubagentBatchError {
    EmptyBatch,
    InvalidConcurrency,
    TaskCountExceeded,
    DuplicateTaskId,
    InvalidTask(SubagentError),
    TimeoutOverrideExceeded,
    InvalidPolicy,
}

impl fmt::Display for SubagentBatchError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            Self::EmptyBatch => "subagent batch is empty",
            Self::InvalidConcurrency => "subagent batch concurrency is invalid",
            Self::TaskCountExceeded => "subagent batch task count exceeds its limit",
            Self::DuplicateTaskId => "subagent batch contains a duplicate task ID",
            Self::InvalidTask(error) => return error.fmt(formatter),
            Self::TimeoutOverrideExceeded => "task timeout exceeds batch timeout",
            Self::InvalidPolicy => "subagent batch policy is invalid",
        };
        formatter.write_str(text)
    }
}

impl std::error::Error for SubagentBatchError {}

#[derive(Clone, Copy, Default)]
struct AggregateReservation {
    provider_turns: usize,
    tool_calls: usize,
    tool_output_bytes: usize,
}

impl AggregateReservation {
    fn reserve(&mut self, request: &SubagentRequest, policy: &SubagentConcurrencyPolicy) -> bool {
        let budget = request.tool_policy.budget();
        let provider_turns = self
            .provider_turns
            .saturating_add(budget.max_provider_turns);
        let tool_calls = self.tool_calls.saturating_add(budget.max_tool_calls);
        let tool_output_bytes = self
            .tool_output_bytes
            .saturating_add(budget.max_aggregate_tool_output_bytes);
        if provider_turns > policy.max_provider_turns
            || tool_calls > policy.max_tool_calls
            || tool_output_bytes > policy.max_tool_output_bytes
        {
            return false;
        }
        self.provider_turns = provider_turns;
        self.tool_calls = tool_calls;
        self.tool_output_bytes = tool_output_bytes;
        true
    }
}

struct ValidatedTask {
    task: SubagentTask,
}

pub struct SubagentBatchRuntime<P> {
    runtime: Arc<SubagentRuntime<P>>,
}

impl<P> SubagentBatchRuntime<P> {
    pub fn new(runtime: SubagentRuntime<P>) -> Self {
        Self {
            runtime: Arc::new(runtime),
        }
    }
}

impl<P: SubagentProvider + 'static> SubagentBatchRuntime<P> {
    pub async fn run(
        &self,
        request: SubagentBatchRequest,
    ) -> Result<SubagentBatchOutcome, SubagentBatchError> {
        let started = Instant::now();
        let validated = self.validate(&request)?;
        let total = validated.len();
        let mut slots = (0..total)
            .map(|index| SubagentTaskOutcome {
                task_id: validated[index].task.request.task_id.clone(),
                state: SubagentTaskState::Queued,
                outcome: None,
                error: None,
            })
            .collect::<Vec<_>>();
        let cancellation = request.cancellation.clone();
        let semaphore = Arc::new(Semaphore::new(request.policy.max_concurrency));
        let reservations = Arc::new(Mutex::new(AggregateReservation::default()));
        let peak = Arc::new(Mutex::new(0usize));
        let active = Arc::new(Mutex::new(0usize));
        let mut join_set = JoinSet::new();
        let mut task_indices = HashMap::new();
        let mut next = 0usize;
        let mut completion_audit = Vec::new();
        let mut cancelled = false;
        let mut timed_out = false;
        let mut budget_exhausted = false;

        loop {
            while !cancelled
                && !cancellation.is_cancelled()
                && next < total
                && join_set.len() < request.policy.max_concurrency
            {
                if !reservations
                    .lock()
                    .await
                    .reserve(&validated[next].task.request, &request.policy)
                {
                    budget_exhausted = true;
                    cancelled = true;
                    break;
                }
                let index = next;
                let task = validated[index].task.clone();
                let permit = semaphore.clone().acquire_owned().await.unwrap();
                let runtime = self.runtime.clone();
                let batch_cancellation = cancellation.clone();
                let active_count = active.clone();
                let peak_count = peak.clone();
                slots[index].state = SubagentTaskState::Running;
                next += 1;
                let task_handle = join_set.spawn(async move {
                    let _permit = permit;
                    let task_cancellation = batch_cancellation.child_token();
                    let mut task_request = task.request;
                    task_request.cancellation = task_cancellation;
                    if let Some(timeout) = task.timeout_override {
                        task_request.timeout = timeout;
                    }
                    let current = {
                        let mut active = active_count.lock().await;
                        *active += 1;
                        *active
                    };
                    {
                        let mut peak = peak_count.lock().await;
                        *peak = (*peak).max(current);
                    }
                    let result = runtime.run_subagent(task_request).await;
                    *active_count.lock().await -= 1;
                    result
                });
                task_indices.insert(task_handle.id(), index);
            }

            if join_set.is_empty() {
                break;
            }
            let joined = if timed_out || cancelled || cancellation.is_cancelled() {
                cancellation.cancel();
                join_set.join_next_with_id().await
            } else {
                match tokio::time::timeout(
                    request
                        .policy
                        .batch_timeout
                        .saturating_sub(started.elapsed()),
                    join_set.join_next_with_id(),
                )
                .await
                {
                    Ok(joined) => joined,
                    Err(_) => {
                        timed_out = true;
                        cancellation.cancel();
                        join_set.join_next_with_id().await
                    }
                }
            };
            let Some(joined) = joined else { break };
            match joined {
                Ok((task_id, Ok(outcome))) => {
                    let Some(index) = task_indices.remove(&task_id) else {
                        continue;
                    };
                    let mut state = task_state(outcome.status);
                    if timed_out && state == SubagentTaskState::Cancelled {
                        state = SubagentTaskState::TimedOut;
                    }
                    if matches!(
                        state,
                        SubagentTaskState::Failed | SubagentTaskState::BudgetExhausted
                    ) && request.policy.failure_policy == SubagentFailurePolicy::CancelRemaining
                    {
                        cancelled = true;
                        cancellation.cancel();
                    }
                    if state == SubagentTaskState::BudgetExhausted {
                        budget_exhausted = true;
                    }
                    completion_audit.push(slots[index].task_id.clone());
                    slots[index].state = state;
                    slots[index].outcome = Some(outcome);
                }
                Ok((task_id, Err(error))) => {
                    let Some(index) = task_indices.remove(&task_id) else {
                        continue;
                    };
                    completion_audit.push(slots[index].task_id.clone());
                    slots[index].state = SubagentTaskState::Failed;
                    slots[index].error = Some(error);
                    if request.policy.failure_policy == SubagentFailurePolicy::CancelRemaining {
                        cancelled = true;
                        cancellation.cancel();
                    }
                }
                Err(error) => {
                    let Some(index) = task_indices.remove(&error.id()) else {
                        continue;
                    };
                    completion_audit.push(slots[index].task_id.clone());
                    if error.is_cancelled() && (timed_out || cancellation.is_cancelled()) {
                        slots[index].state = if timed_out {
                            SubagentTaskState::TimedOut
                        } else {
                            SubagentTaskState::Cancelled
                        };
                    } else {
                        slots[index].state = SubagentTaskState::Failed;
                        slots[index].error = Some(SubagentError::JoinFailure);
                        if request.policy.failure_policy == SubagentFailurePolicy::CancelRemaining {
                            cancelled = true;
                            cancellation.cancel();
                        }
                    }
                }
            }
            if timed_out || cancellation.is_cancelled() {
                cancelled = true;
            }
        }

        let final_state = if timed_out {
            SubagentBatchStatus::TimedOut
        } else if budget_exhausted {
            SubagentBatchStatus::BudgetExhausted
        } else if cancellation.is_cancelled() || cancelled {
            SubagentBatchStatus::Cancelled
        } else if slots
            .iter()
            .any(|slot| slot.state == SubagentTaskState::Failed)
        {
            SubagentBatchStatus::PartiallyFailed
        } else {
            SubagentBatchStatus::Completed
        };
        for slot in &mut slots {
            if slot.state == SubagentTaskState::Queued {
                slot.state = SubagentTaskState::NotStarted;
            }
        }
        completion_audit.truncate(SUBAGENT_BATCH_MAX_COMPLETION_AUDIT);
        let aggregate_provider_turns = slots
            .iter()
            .filter_map(|slot| slot.outcome.as_ref())
            .map(|outcome| outcome.provider_turns)
            .sum();
        let aggregate_tool_calls = slots
            .iter()
            .filter_map(|slot| slot.outcome.as_ref())
            .map(|outcome| outcome.tool_calls)
            .sum();
        let aggregate_tool_output_bytes = slots
            .iter()
            .filter_map(|slot| slot.outcome.as_ref())
            .flat_map(|outcome| outcome.tool_audit.iter())
            .map(|audit| audit.output_bytes)
            .sum();
        Ok(SubagentBatchOutcome {
            status: final_state,
            total_task_count: total,
            started_task_count: slots
                .iter()
                .filter(|slot| slot.state != SubagentTaskState::NotStarted)
                .count(),
            completed_task_count: slots
                .iter()
                .filter(|slot| slot.state == SubagentTaskState::Completed)
                .count(),
            failed_task_count: slots
                .iter()
                .filter(|slot| slot.state == SubagentTaskState::Failed)
                .count(),
            cancelled_task_count: slots
                .iter()
                .filter(|slot| slot.state == SubagentTaskState::Cancelled)
                .count(),
            timed_out_task_count: slots
                .iter()
                .filter(|slot| slot.state == SubagentTaskState::TimedOut)
                .count(),
            not_started_task_count: slots
                .iter()
                .filter(|slot| slot.state == SubagentTaskState::NotStarted)
                .count(),
            peak_observed_concurrency: *peak.lock().await,
            configured_concurrency: request.policy.max_concurrency,
            aggregate_provider_turns,
            aggregate_tool_calls,
            aggregate_tool_output_bytes,
            elapsed: started.elapsed(),
            tasks: slots,
            completion_audit,
            failure_policy: request.policy.failure_policy,
            budget_exhausted,
        })
    }

    fn validate(
        &self,
        request: &SubagentBatchRequest,
    ) -> Result<Vec<ValidatedTask>, SubagentBatchError> {
        if request.tasks.is_empty() {
            return Err(SubagentBatchError::EmptyBatch);
        }
        if request.policy.max_tasks == 0
            || request.policy.max_concurrency == 0
            || request.policy.max_concurrency > SUBAGENT_BATCH_MAX_CONCURRENCY
            || request.policy.batch_timeout.is_zero()
            || request.policy.max_provider_turns == 0
            || request.policy.max_tool_calls == 0
            || request.policy.max_tool_output_bytes == 0
        {
            return Err(SubagentBatchError::InvalidPolicy);
        }
        if request.tasks.len() > request.policy.max_tasks {
            return Err(SubagentBatchError::TaskCountExceeded);
        }
        let mut ids = std::collections::HashSet::new();
        let mut validated = Vec::with_capacity(request.tasks.len());
        for task in &request.tasks {
            if !ids.insert(task.request.task_id.clone()) {
                return Err(SubagentBatchError::DuplicateTaskId);
            }
            if task.request.timeout > request.policy.batch_timeout
                || task
                    .timeout_override
                    .is_some_and(|timeout| timeout > request.policy.batch_timeout)
            {
                return Err(SubagentBatchError::TimeoutOverrideExceeded);
            }
            if task.request.cancellation.is_cancelled() {
                return Err(SubagentBatchError::InvalidTask(
                    SubagentError::CancelledBeforeStart,
                ));
            }
            self.runtime
                .validate_for_batch(&task.request)
                .map_err(SubagentBatchError::InvalidTask)?;
            if task.request.tool_policy.budget().max_provider_turns
                > request.policy.max_provider_turns
                || task.request.tool_policy.budget().max_tool_calls > request.policy.max_tool_calls
                || task
                    .request
                    .tool_policy
                    .budget()
                    .max_aggregate_tool_output_bytes
                    > request.policy.max_tool_output_bytes
            {
                return Err(SubagentBatchError::InvalidPolicy);
            }
            validated.push(ValidatedTask { task: task.clone() });
        }
        Ok(validated)
    }
}

fn task_state(status: SubagentStatus) -> SubagentTaskState {
    match status {
        SubagentStatus::Completed | SubagentStatus::CompletedWithTruncation => {
            SubagentTaskState::Completed
        }
        SubagentStatus::Cancelled => SubagentTaskState::Cancelled,
        SubagentStatus::TimedOut => SubagentTaskState::TimedOut,
        SubagentStatus::BudgetExhausted => SubagentTaskState::BudgetExhausted,
        _ => SubagentTaskState::Failed,
    }
}
