use serde::{Deserialize, Serialize};
#[cfg(test)]
use std::path::Path;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};

use crate::{switcher_config_data_dir, write_bytes_atomic};

const MAX_RESET_LOGS: usize = 200;
const MAX_JAVASCRIPT_TIMESTAMP_MS: i64 = 8_640_000_000_000_000;
static RESET_STATE_LOCK: Mutex<()> = Mutex::new(());

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ScheduledReset {
    pub(crate) id: String,
    pub(crate) account_id: String,
    pub(crate) account_label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) reset_credit_id: Option<String>,
    pub(crate) scheduled_at: i64,
    pub(crate) status: ResetStatus,
    pub(crate) created_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) started_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) finished_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) error: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(crate) enum ResetStatus {
    Scheduled,
    Running,
    Completed,
    Failed,
    Missed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ResetLog {
    pub(crate) id: String,
    pub(crate) account_id: String,
    pub(crate) account_label: String,
    #[serde(rename = "type")]
    pub(crate) reset_type: ResetLogType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) reset_credit_id: Option<String>,
    pub(crate) occurred_at: i64,
    pub(crate) result: ResetLogResult,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) error: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(crate) enum ResetLogType {
    Immediate,
    Scheduled,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(crate) enum ResetLogResult {
    Success,
    Failed,
    Missed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ResetState {
    #[serde(default)]
    pub(crate) scheduled_resets: Vec<ScheduledReset>,
    #[serde(default)]
    pub(crate) logs: Vec<ResetLog>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ResetClaim {
    pub(crate) state: ResetState,
    pub(crate) task: Option<ScheduledReset>,
}

pub(crate) struct ResetStateStore {
    path: PathBuf,
}

impl Default for ResetStateStore {
    fn default() -> Self {
        Self::new(switcher_config_data_dir().join("reset-state.json"))
    }
}

impl ResetStateStore {
    pub(crate) fn new(path: PathBuf) -> Self {
        Self { path }
    }

    #[cfg(test)]
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    pub(crate) fn load(&self) -> Result<ResetState, String> {
        let _guard = lock_reset_state()?;
        self.load_unlocked()
    }

    pub(crate) fn initialize(&self, now_ms: i64) -> Result<ResetState, String> {
        let _guard = lock_reset_state()?;
        let state = self.load_unlocked()?;
        let normalized = normalize_on_startup(state.clone(), now_ms);
        self.save_unlocked(&normalized)
    }

    #[cfg(test)]
    pub(crate) fn save(&self, state: &ResetState) -> Result<ResetState, String> {
        let _guard = lock_reset_state()?;
        self.save_unlocked(state)
    }

    pub(crate) fn create_scheduled_reset(
        &self,
        task: ScheduledReset,
        now_ms: i64,
    ) -> Result<ResetState, String> {
        self.mutate(|state| {
            if task.status != ResetStatus::Scheduled {
                return Err("新预约任务状态必须为 scheduled".to_string());
            }
            if task.scheduled_at <= now_ms {
                return Err("预约时间必须晚于当前时间".to_string());
            }
            if state.scheduled_resets.iter().any(|current| {
                current.account_id == task.account_id && is_active_status(current.status)
            }) {
                return Err("该账号已有预约重置".to_string());
            }
            state.scheduled_resets.push(task);
            Ok(())
        })
        .map(|(state, ())| state)
    }

    pub(crate) fn update_scheduled_reset(
        &self,
        schedule_id: &str,
        scheduled_at: i64,
        now_ms: i64,
    ) -> Result<ResetState, String> {
        self.mutate(|state| {
            if scheduled_at <= now_ms {
                return Err("预约时间必须晚于当前时间".to_string());
            }
            let task = state
                .scheduled_resets
                .iter_mut()
                .find(|task| task.id == schedule_id)
                .ok_or_else(|| "预约任务不存在或已无法修改".to_string())?;
            if task.status != ResetStatus::Scheduled || task.scheduled_at <= now_ms {
                return Err("预约已开始执行或已无法修改".to_string());
            }
            task.scheduled_at = scheduled_at;
            Ok(())
        })
        .map(|(state, ())| state)
    }

    pub(crate) fn cancel_scheduled_reset(
        &self,
        schedule_id: &str,
        occurred_at: i64,
        log_id: String,
    ) -> Result<ResetState, String> {
        self.mutate(|state| {
            let position = state
                .scheduled_resets
                .iter()
                .position(|task| task.id == schedule_id && task.status == ResetStatus::Scheduled)
                .ok_or_else(|| "预约任务不存在或已无法取消".to_string())?;
            let task = state.scheduled_resets.remove(position);
            state.logs.push(ResetLog {
                id: log_id,
                account_id: task.account_id,
                account_label: task.account_label,
                reset_type: ResetLogType::Scheduled,
                reset_credit_id: task.reset_credit_id,
                occurred_at,
                result: ResetLogResult::Cancelled,
                error: None,
            });
            Ok(())
        })
        .map(|(state, ())| state)
    }

    pub(crate) fn claim_scheduled_reset(
        &self,
        schedule_id: &str,
        now_ms: i64,
    ) -> Result<ResetClaim, String> {
        let (state, task) = self.mutate(|state| {
            let Some(task) = state.scheduled_resets.iter_mut().find(|task| {
                task.id == schedule_id
                    && task.status == ResetStatus::Scheduled
                    && task.scheduled_at <= now_ms
            }) else {
                return Ok(None);
            };
            task.status = ResetStatus::Running;
            task.started_at = Some(now_ms);
            Ok(Some(task.clone()))
        })?;
        Ok(ResetClaim { state, task })
    }

    pub(crate) fn finish_scheduled_reset(
        &self,
        schedule_id: &str,
        finished_at: i64,
        result: ResetLogResult,
        error: Option<String>,
        log_id: String,
    ) -> Result<ResetState, String> {
        if !matches!(result, ResetLogResult::Success | ResetLogResult::Failed) {
            return Err("预约执行结果只允许 success 或 failed".to_string());
        }
        self.mutate(|state| {
            let position = state
                .scheduled_resets
                .iter()
                .position(|task| task.id == schedule_id && task.status == ResetStatus::Running)
                .ok_or_else(|| "预约任务不存在或未处于执行中".to_string())?;
            let task = state.scheduled_resets.remove(position);
            state.logs.push(ResetLog {
                id: log_id,
                account_id: task.account_id,
                account_label: task.account_label,
                reset_type: ResetLogType::Scheduled,
                reset_credit_id: task.reset_credit_id,
                occurred_at: finished_at,
                result,
                error,
            });
            Ok(())
        })
        .map(|(state, ())| state)
    }

    pub(crate) fn append_log(&self, log: ResetLog) -> Result<ResetState, String> {
        self.mutate(|state| {
            state.logs.push(log);
            Ok(())
        })
        .map(|(state, ())| state)
    }

    pub(crate) fn delete_log(&self, log_id: &str) -> Result<ResetState, String> {
        self.mutate(|state| {
            let position = state
                .logs
                .iter()
                .position(|log| log.id == log_id)
                .ok_or_else(|| "重置日志不存在".to_string())?;
            state.logs.remove(position);
            Ok(())
        })
        .map(|(state, ())| state)
    }

    pub(crate) fn clear_logs(&self) -> Result<ResetState, String> {
        self.mutate(|state| {
            state.logs.clear();
            Ok(())
        })
        .map(|(state, ())| state)
    }

    fn mutate<T>(
        &self,
        mutation: impl FnOnce(&mut ResetState) -> Result<T, String>,
    ) -> Result<(ResetState, T), String> {
        let _guard = lock_reset_state()?;
        let mut state = self.load_unlocked()?;
        let result = mutation(&mut state)?;
        let saved = self.save_unlocked(&state)?;
        Ok((saved, result))
    }

    fn load_unlocked(&self) -> Result<ResetState, String> {
        if !self.path.exists() {
            return Ok(ResetState::default());
        }
        let content = std::fs::read_to_string(&self.path)
            .map_err(|error| format!("读取重置状态失败 ({}): {}", self.path.display(), error))?;
        let state: ResetState = serde_json::from_str(&content)
            .map_err(|error| format!("解析重置状态失败 ({}): {}", self.path.display(), error))?;
        validate_state(&state)
            .map_err(|error| format!("重置状态无效 ({}): {}", self.path.display(), error))?;
        Ok(state)
    }

    fn save_unlocked(&self, state: &ResetState) -> Result<ResetState, String> {
        let mut normalized = state.clone();
        validate_state(&normalized)?;
        normalized
            .scheduled_resets
            .retain(|task| is_active_status(task.status));
        normalized.logs.sort_by_key(|log| log.occurred_at);
        if normalized.logs.len() > MAX_RESET_LOGS {
            let drop_count = normalized.logs.len() - MAX_RESET_LOGS;
            normalized.logs.drain(0..drop_count);
        }
        let content = serde_json::to_vec_pretty(&normalized)
            .map_err(|error| format!("序列化重置状态失败: {}", error))?;
        write_bytes_atomic(&self.path, &content)
            .map_err(|error| format!("写入重置状态失败 ({}): {}", self.path.display(), error))?;
        Ok(normalized)
    }
}

pub(crate) fn normalize_on_startup(mut state: ResetState, now_ms: i64) -> ResetState {
    let mut missed_logs = Vec::new();
    let mut active_tasks = Vec::new();
    for mut task in std::mem::take(&mut state.scheduled_resets) {
        let missed = task.status == ResetStatus::Running
            || (task.status == ResetStatus::Scheduled && task.scheduled_at <= now_ms);
        if !missed && task.status == ResetStatus::Scheduled {
            active_tasks.push(task);
            continue;
        }
        if !missed {
            continue;
        }
        task.status = ResetStatus::Missed;
        task.finished_at = Some(now_ms);
        task.error = Some("应用未运行，任务未执行".to_string());
        missed_logs.push(ResetLog {
            id: format!("missed-{}-{}", task.id, now_ms),
            account_id: task.account_id.clone(),
            account_label: task.account_label.clone(),
            reset_type: ResetLogType::Scheduled,
            reset_credit_id: task.reset_credit_id.clone(),
            occurred_at: now_ms,
            result: ResetLogResult::Missed,
            error: task.error.clone(),
        });
    }
    state.scheduled_resets = active_tasks;
    state.logs.extend(missed_logs);
    state
}

fn is_active_status(status: ResetStatus) -> bool {
    matches!(status, ResetStatus::Scheduled | ResetStatus::Running)
}

fn validate_state(state: &ResetState) -> Result<(), String> {
    for task in &state.scheduled_resets {
        validate_non_empty("预约任务 ID", &task.id)?;
        validate_non_empty("预约账号 ID", &task.account_id)?;
        validate_timestamp("预约时间", task.scheduled_at)?;
        validate_timestamp("创建时间", task.created_at)?;
        validate_optional_timestamp("开始时间", task.started_at)?;
        validate_optional_timestamp("结束时间", task.finished_at)?;
    }
    for log in &state.logs {
        validate_non_empty("日志 ID", &log.id)?;
        validate_non_empty("日志账号 ID", &log.account_id)?;
        validate_timestamp("日志时间", log.occurred_at)?;
    }
    Ok(())
}

fn validate_non_empty(label: &str, value: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        Err(format!("{}不能为空", label))
    } else {
        Ok(())
    }
}

fn validate_timestamp(label: &str, value: i64) -> Result<(), String> {
    if value < 0 {
        Err(format!("{}不能为负数", label))
    } else if value > MAX_JAVASCRIPT_TIMESTAMP_MS {
        Err(format!("{}超出支持范围", label))
    } else {
        Ok(())
    }
}

fn validate_optional_timestamp(label: &str, value: Option<i64>) -> Result<(), String> {
    value.map_or(Ok(()), |value| validate_timestamp(label, value))
}

fn lock_reset_state() -> Result<MutexGuard<'static, ()>, String> {
    RESET_STATE_LOCK
        .lock()
        .map_err(|_| "重置状态锁已损坏".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Barrier};
    use std::thread;

    fn scheduled_task(id: &str, scheduled_at: i64) -> ScheduledReset {
        ScheduledReset {
            id: id.to_string(),
            account_id: "account-1".to_string(),
            account_label: "demo@example.com".to_string(),
            reset_credit_id: None,
            scheduled_at,
            status: ResetStatus::Scheduled,
            created_at: 500,
            started_at: None,
            finished_at: None,
            error: None,
        }
    }

    fn test_log(index: i64) -> ResetLog {
        ResetLog {
            id: format!("log-{index}"),
            account_id: "account-1".to_string(),
            account_label: "demo@example.com".to_string(),
            reset_type: ResetLogType::Immediate,
            reset_credit_id: None,
            occurred_at: index,
            result: ResetLogResult::Success,
            error: None,
        }
    }

    #[test]
    fn startup_marks_expired_scheduled_reset_as_missed() {
        let state = ResetState {
            scheduled_resets: vec![scheduled_task("schedule-1", 1_000)],
            logs: Vec::new(),
        };
        let normalized = normalize_on_startup(state, 2_000);
        assert!(normalized.scheduled_resets.is_empty());
        assert_eq!(normalized.logs[0].result, ResetLogResult::Missed);
    }

    #[test]
    fn saving_state_discards_terminal_schedules() {
        let dir = tempfile::tempdir().unwrap();
        let store = ResetStateStore::new(dir.path().join("reset-state.json"));
        let mut completed = scheduled_task("completed", 1_000);
        completed.status = ResetStatus::Completed;
        completed.finished_at = Some(2_000);

        let saved = store
            .save(&ResetState {
                scheduled_resets: vec![scheduled_task("active", 3_000), completed],
                logs: Vec::new(),
            })
            .unwrap();

        assert_eq!(saved.scheduled_resets.len(), 1);
        assert_eq!(saved.scheduled_resets[0].id, "active");
    }

    #[test]
    fn claiming_due_schedule_is_atomic() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("reset-state.json");
        let store = ResetStateStore::new(path.clone());
        store
            .save(&ResetState {
                scheduled_resets: vec![scheduled_task("schedule-1", 1_000)],
                logs: Vec::new(),
            })
            .unwrap();
        let barrier = Arc::new(Barrier::new(3));

        let first_path = path.clone();
        let first_barrier = Arc::clone(&barrier);
        let first = thread::spawn(move || {
            first_barrier.wait();
            ResetStateStore::new(first_path)
                .claim_scheduled_reset("schedule-1", 2_000)
                .unwrap()
        });
        let second_path = path.clone();
        let second_barrier = Arc::clone(&barrier);
        let second = thread::spawn(move || {
            second_barrier.wait();
            ResetStateStore::new(second_path)
                .claim_scheduled_reset("schedule-1", 2_000)
                .unwrap()
        });

        barrier.wait();
        let claims = [first.join().unwrap(), second.join().unwrap()];

        assert_eq!(
            claims.iter().filter(|claim| claim.task.is_some()).count(),
            1
        );
        let saved = ResetStateStore::new(path).load().unwrap();
        assert_eq!(saved.scheduled_resets[0].status, ResetStatus::Running);
    }

    #[test]
    fn creating_second_active_schedule_for_account_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let store = ResetStateStore::new(dir.path().join("reset-state.json"));

        store
            .create_scheduled_reset(scheduled_task("schedule-1", 1_000), 0)
            .unwrap();
        let error = store
            .create_scheduled_reset(scheduled_task("schedule-2", 2_000), 0)
            .unwrap_err();

        assert!(error.contains("已有预约"));
        let saved = store.load().unwrap();
        assert_eq!(saved.scheduled_resets.len(), 1);
        assert_eq!(saved.scheduled_resets[0].id, "schedule-1");
    }

    #[test]
    fn creating_expired_schedule_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let store = ResetStateStore::new(dir.path().join("reset-state.json"));

        let error = store
            .create_scheduled_reset(scheduled_task("schedule-1", 1_000), 2_000)
            .unwrap_err();

        assert!(error.contains("晚于当前时间"));
        assert_eq!(store.load().unwrap(), ResetState::default());
    }

    #[test]
    fn updating_future_schedule_changes_only_its_time() {
        let dir = tempfile::tempdir().unwrap();
        let store = ResetStateStore::new(dir.path().join("reset-state.json"));
        store
            .save(&ResetState {
                scheduled_resets: vec![scheduled_task("schedule-1", 3_000)],
                logs: vec![test_log(10)],
            })
            .unwrap();

        let updated = store
            .update_scheduled_reset("schedule-1", 5_000, 2_000)
            .unwrap();

        assert_eq!(updated.scheduled_resets[0].id, "schedule-1");
        assert_eq!(updated.scheduled_resets[0].created_at, 500);
        assert_eq!(updated.scheduled_resets[0].scheduled_at, 5_000);
        assert_eq!(updated.logs, vec![test_log(10)]);
    }

    #[test]
    fn updating_due_or_running_schedule_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let store = ResetStateStore::new(dir.path().join("reset-state.json"));
        store
            .save(&ResetState {
                scheduled_resets: vec![scheduled_task("due", 1_000)],
                logs: Vec::new(),
            })
            .unwrap();
        assert!(store
            .update_scheduled_reset("due", 4_000, 2_000)
            .unwrap_err()
            .contains("无法修改"));

        let mut running = scheduled_task("running", 4_000);
        running.status = ResetStatus::Running;
        running.started_at = Some(2_000);
        store
            .save(&ResetState {
                scheduled_resets: vec![running],
                logs: Vec::new(),
            })
            .unwrap();
        assert!(store
            .update_scheduled_reset("running", 5_000, 3_000)
            .unwrap_err()
            .contains("无法修改"));
    }

    #[test]
    fn updating_schedule_to_past_time_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let store = ResetStateStore::new(dir.path().join("reset-state.json"));
        store
            .save(&ResetState {
                scheduled_resets: vec![scheduled_task("schedule-1", 4_000)],
                logs: Vec::new(),
            })
            .unwrap();

        let error = store
            .update_scheduled_reset("schedule-1", 2_000, 3_000)
            .unwrap_err();

        assert!(error.contains("晚于当前时间"));
        assert_eq!(
            store.load().unwrap().scheduled_resets[0].scheduled_at,
            4_000
        );
    }

    #[test]
    fn startup_migration_is_separate_from_regular_loads() {
        let dir = tempfile::tempdir().unwrap();
        let store = ResetStateStore::new(dir.path().join("reset-state.json"));
        store
            .save(&ResetState {
                scheduled_resets: vec![scheduled_task("schedule-1", 1_000)],
                logs: Vec::new(),
            })
            .unwrap();

        let loaded = store.load().unwrap();
        assert_eq!(loaded.scheduled_resets.len(), 1);

        let initialized = store.initialize(2_000).unwrap();
        assert!(initialized.scheduled_resets.is_empty());
        assert_eq!(initialized.logs.len(), 1);

        let loaded_again = store.load().unwrap();
        assert!(loaded_again.scheduled_resets.is_empty());
        assert_eq!(loaded_again.logs.len(), 1);
    }

    #[test]
    fn startup_migration_trims_legacy_logs_even_without_expired_tasks() {
        let dir = tempfile::tempdir().unwrap();
        let store = ResetStateStore::new(dir.path().join("reset-state.json"));
        let legacy_state = ResetState {
            scheduled_resets: vec![scheduled_task("future", 3_000)],
            logs: (0..205).map(test_log).collect(),
        };
        std::fs::write(
            store.path(),
            serde_json::to_vec_pretty(&legacy_state).unwrap(),
        )
        .unwrap();

        let initialized = store.initialize(2_000).unwrap();

        assert_eq!(initialized.logs.len(), 200);
        assert_eq!(initialized.logs[0].id, "log-5");
        assert_eq!(initialized.scheduled_resets[0].id, "future");
    }

    #[test]
    fn finishing_claimed_schedule_removes_task_and_appends_log() {
        let dir = tempfile::tempdir().unwrap();
        let store = ResetStateStore::new(dir.path().join("reset-state.json"));
        store
            .save(&ResetState {
                scheduled_resets: vec![scheduled_task("schedule-1", 1_000)],
                logs: Vec::new(),
            })
            .unwrap();
        store.claim_scheduled_reset("schedule-1", 2_000).unwrap();

        let saved = store
            .finish_scheduled_reset(
                "schedule-1",
                3_000,
                ResetLogResult::Success,
                None,
                "finish-log".to_string(),
            )
            .unwrap();

        assert!(saved.scheduled_resets.is_empty());
        assert_eq!(saved.logs.len(), 1);
        assert_eq!(saved.logs[0].id, "finish-log");
        assert_eq!(saved.logs[0].result, ResetLogResult::Success);
    }

    #[test]
    fn concurrent_cancel_and_log_append_preserve_both_results() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("reset-state.json");
        ResetStateStore::new(path.clone())
            .save(&ResetState {
                scheduled_resets: vec![scheduled_task("schedule-1", 3_000)],
                logs: Vec::new(),
            })
            .unwrap();
        let barrier = Arc::new(Barrier::new(3));

        let cancel_path = path.clone();
        let cancel_barrier = Arc::clone(&barrier);
        let cancel = thread::spawn(move || {
            cancel_barrier.wait();
            ResetStateStore::new(cancel_path)
                .cancel_scheduled_reset("schedule-1", 2_000, "cancel-log".to_string())
                .unwrap();
        });

        let append_path = path.clone();
        let append_barrier = Arc::clone(&barrier);
        let append = thread::spawn(move || {
            append_barrier.wait();
            ResetStateStore::new(append_path)
                .append_log(test_log(1))
                .unwrap();
        });

        barrier.wait();
        cancel.join().unwrap();
        append.join().unwrap();

        let saved = ResetStateStore::new(path).load().unwrap();
        assert!(saved.scheduled_resets.is_empty());
        assert_eq!(saved.logs.len(), 2);
        assert!(saved.logs.iter().any(|log| log.id == "cancel-log"));
        assert!(saved.logs.iter().any(|log| log.id == "log-1"));
    }

    #[test]
    fn deleting_one_log_preserves_schedules_and_other_logs() {
        let dir = tempfile::tempdir().unwrap();
        let store = ResetStateStore::new(dir.path().join("reset-state.json"));
        store
            .save(&ResetState {
                scheduled_resets: vec![scheduled_task("schedule-1", 3_000)],
                logs: vec![test_log(1), test_log(2)],
            })
            .unwrap();

        let updated = store.delete_log("log-1").unwrap();

        assert_eq!(updated.scheduled_resets.len(), 1);
        assert_eq!(updated.logs, vec![test_log(2)]);
        assert!(store.delete_log("missing").unwrap_err().contains("不存在"));
    }

    #[test]
    fn clearing_logs_preserves_active_schedules() {
        let dir = tempfile::tempdir().unwrap();
        let store = ResetStateStore::new(dir.path().join("reset-state.json"));
        store
            .save(&ResetState {
                scheduled_resets: vec![scheduled_task("schedule-1", 3_000)],
                logs: vec![test_log(1), test_log(2)],
            })
            .unwrap();

        let updated = store.clear_logs().unwrap();

        assert!(updated.logs.is_empty());
        assert_eq!(updated.scheduled_resets[0].id, "schedule-1");
    }

    #[test]
    fn saving_state_keeps_only_latest_200_logs() {
        let dir = tempfile::tempdir().unwrap();
        let store = ResetStateStore::new(dir.path().join("reset-state.json"));
        let logs = (0..205).map(test_log).collect();
        let saved = store
            .save(&ResetState {
                scheduled_resets: Vec::new(),
                logs,
            })
            .unwrap();
        assert_eq!(saved.logs.len(), 200);
        assert_eq!(saved.logs[0].id, "log-5");
        assert!(store.path().exists());
    }

    #[test]
    fn saving_state_rejects_blank_account_id() {
        let dir = tempfile::tempdir().unwrap();
        let store = ResetStateStore::new(dir.path().join("reset-state.json"));
        let state = ResetState {
            scheduled_resets: vec![ScheduledReset {
                id: "schedule-1".into(),
                account_id: " ".into(),
                account_label: "demo@example.com".into(),
                reset_credit_id: None,
                scheduled_at: 1_000,
                status: ResetStatus::Scheduled,
                created_at: 500,
                started_at: None,
                finished_at: None,
                error: None,
            }],
            logs: Vec::new(),
        };
        assert!(store.save(&state).unwrap_err().contains("账号 ID"));
    }

    #[test]
    fn saving_state_rejects_timestamp_outside_javascript_date_range() {
        let dir = tempfile::tempdir().unwrap();
        let store = ResetStateStore::new(dir.path().join("reset-state.json"));
        let state = ResetState {
            scheduled_resets: vec![scheduled_task("schedule-1", 9_000_000_000_000_000)],
            logs: Vec::new(),
        };

        let error = store.save(&state).unwrap_err();

        assert!(error.contains("预约时间"));
        assert!(error.contains("支持范围"));
    }

    #[test]
    fn loading_state_rejects_timestamp_outside_javascript_date_range() {
        let dir = tempfile::tempdir().unwrap();
        let store = ResetStateStore::new(dir.path().join("reset-state.json"));
        let state = ResetState {
            scheduled_resets: vec![scheduled_task("schedule-1", 9_000_000_000_000_000)],
            logs: Vec::new(),
        };
        std::fs::write(store.path(), serde_json::to_vec_pretty(&state).unwrap()).unwrap();

        let error = store.load().unwrap_err();

        assert!(error.contains("预约时间"));
        assert!(error.contains("支持范围"));
    }

    #[test]
    fn missing_state_file_returns_default_state() {
        let dir = tempfile::tempdir().unwrap();
        let store = ResetStateStore::new(dir.path().join("reset-state.json"));
        assert_eq!(store.load().unwrap(), ResetState::default());
    }
}
