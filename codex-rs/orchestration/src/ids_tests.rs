use super::*;
use pretty_assertions::assert_eq;

#[test]
fn identifiers_validate_and_display() {
    let workflow = WorkflowId::new("workflow-1").expect("valid workflow id");
    let task = TaskId::new("task-1").expect("valid task id");
    assert_eq!(workflow.as_str(), "workflow-1");
    assert_eq!(task.to_string(), "task-1");
    assert_eq!(WorkflowId::new(""), Err(IdentifierError::Empty));
    assert_eq!(TaskId::new("x".repeat(129)), Err(IdentifierError::TooLong));
}
