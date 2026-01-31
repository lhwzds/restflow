use restflow_core::models::{AgentTask, TaskSchedule};

#[test]
fn test_cron_parsing() {
    let schedule = TaskSchedule::Cron {
        expression: "0 */5 * * * *".to_string(),
        timezone: Some("UTC".to_string()),
    };

    let now = chrono::Utc::now().timestamp_millis();
    let next = AgentTask::calculate_next_run(&schedule, now);

    assert!(next.is_some());
    let next = next.unwrap();
    assert!(next > now);
    assert!(next <= now + 5 * 60 * 1000);
}

#[test]
fn test_cron_daily_9am() {
    let schedule = TaskSchedule::Cron {
        expression: "0 0 9 * * *".to_string(),
        timezone: Some("Asia/Shanghai".to_string()),
    };

    let next = AgentTask::calculate_next_run(&schedule, 0);
    assert!(next.is_some());
}

#[test]
fn test_invalid_cron() {
    let schedule = TaskSchedule::Cron {
        expression: "invalid cron".to_string(),
        timezone: None,
    };

    let next = AgentTask::calculate_next_run(&schedule, 0);
    assert!(next.is_none());
}
