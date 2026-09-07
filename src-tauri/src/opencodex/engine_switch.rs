#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SwitchStep {
    StopOld,
    ActivateNew,
    StartNew,
    StopNew,
    ActivateOld,
    RecoverOld,
}

// Keep activation and runtime recovery in one transaction; never repoint the
// marker while a failed replacement may still be serving requests.
pub fn switch_engine(
    needs_stop: bool,
    mut run: impl FnMut(SwitchStep) -> Result<(), String>,
) -> Result<(), String> {
    if needs_stop {
        if let Err(error) = run(SwitchStep::StopOld) {
            return recovered_error(error, run(SwitchStep::RecoverOld));
        }
    }
    if let Err(error) = run(SwitchStep::ActivateNew) {
        let recovery = run(SwitchStep::ActivateOld).and_then(|_| run(SwitchStep::RecoverOld));
        return recovered_error(error, recovery);
    }
    if let Err(error) = run(SwitchStep::StartNew) {
        let recovery = run(SwitchStep::StopNew)
            .and_then(|_| run(SwitchStep::ActivateOld))
            .and_then(|_| run(SwitchStep::RecoverOld));
        return recovered_error(error, recovery);
    }
    Ok(())
}

fn recovered_error(error: String, recovery: Result<(), String>) -> Result<(), String> {
    Err(match recovery {
        Ok(()) => format!("{error}；已恢复原 Engine 和运行状态"),
        Err(recovery) => {
            format!("{error}；恢复原状态失败：{recovery}。请检查运行日志后手动恢复服务")
        }
    })
}

#[cfg(test)]
mod tests {
    use super::{switch_engine, SwitchStep::*};

    #[test]
    fn running_switch_stops_before_activation_and_restarts() {
        let mut steps = vec![];
        assert!(switch_engine(true, |step| {
            steps.push(step);
            Ok(())
        })
        .is_ok());
        assert_eq!(steps, [StopOld, ActivateNew, StartNew]);
    }

    #[test]
    fn stopped_switch_does_not_stop_a_service() {
        let mut steps = vec![];
        assert!(switch_engine(false, |step| {
            steps.push(step);
            Ok(())
        })
        .is_ok());
        assert_eq!(steps, [ActivateNew, StartNew]);
    }

    #[test]
    fn stop_failure_never_activates_new_engine() {
        let mut steps = vec![];
        let result = switch_engine(true, |step| {
            steps.push(step);
            if step == StopOld {
                Err("stop failed".into())
            } else {
                Ok(())
            }
        });
        assert!(result.unwrap_err().contains("已恢复原 Engine"));
        assert_eq!(steps, [StopOld, RecoverOld]);
    }

    #[test]
    fn failed_health_check_rolls_back_before_recovery() {
        let mut steps = vec![];
        assert!(switch_engine(true, |step| {
            steps.push(step);
            if step == StartNew {
                Err("health failed".into())
            } else {
                Ok(())
            }
        })
        .is_err());
        assert_eq!(
            steps,
            [
                StopOld,
                ActivateNew,
                StartNew,
                StopNew,
                ActivateOld,
                RecoverOld
            ]
        );
    }

    #[test]
    fn failed_activation_restores_marker_and_runtime() {
        let mut steps = vec![];
        assert!(switch_engine(true, |step| {
            steps.push(step);
            if step == ActivateNew {
                Err("write failed".into())
            } else {
                Ok(())
            }
        })
        .is_err());
        assert_eq!(steps, [StopOld, ActivateNew, ActivateOld, RecoverOld]);
    }

    #[test]
    fn failed_replacement_stop_does_not_repoint_marker() {
        let mut steps = vec![];
        let result = switch_engine(true, |step| {
            steps.push(step);
            if step == StartNew || step == StopNew {
                Err("failed".into())
            } else {
                Ok(())
            }
        });
        assert!(result.unwrap_err().contains("恢复原状态失败"));
        assert_eq!(steps, [StopOld, ActivateNew, StartNew, StopNew]);
    }
}
