//! Task queue state machine — scheduling, lane management, and ready-task selection.

use std::collections::{HashMap, HashSet, VecDeque};

use super::now_millis;
use super::types::*;
mod classification;
pub(crate) mod selection;

pub(in crate::agent) use classification::{classify_task, project_task_runs_with_runtime};
pub(in crate::agent) use selection::{
    compute_task_backoff_ms, describe_scheduled_time, is_task_terminal_status, make_task_log_entry,
    refresh_task_queue_state, select_ready_task_indices, status_message,
};

#[cfg(test)]
#[path = "task_scheduler/tests.rs"]
mod tests;
