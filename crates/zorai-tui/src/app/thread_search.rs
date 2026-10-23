use super::*;
use crate::client::ThreadSearchResultsVm;

pub(crate) const THREAD_SEARCH_DEBOUNCE: Duration = Duration::from_millis(300);

#[derive(Debug, Clone)]
pub(super) struct PendingThreadSearch {
    query: String,
    ready_at: Instant,
}

impl TuiModel {
    pub(crate) fn queue_thread_search(&mut self) {
        self.queue_thread_search_at(Instant::now());
    }

    pub(super) fn queue_thread_search_at(&mut self, typed_at: Instant) {
        self.pending_thread_search =
            self.modal
                .thread_search_query()
                .map(|query| PendingThreadSearch {
                    query: query.to_string(),
                    ready_at: typed_at + THREAD_SEARCH_DEBOUNCE,
                });
    }

    pub(crate) fn pending_thread_search_deadline(&self) -> Option<Instant> {
        self.pending_thread_search
            .as_ref()
            .map(|pending| pending.ready_at)
    }

    pub(crate) fn dispatch_due_thread_search(&mut self, now: Instant) -> bool {
        let Some(pending) = self.pending_thread_search.clone() else {
            return false;
        };
        if now < pending.ready_at {
            return false;
        }
        self.pending_thread_search = None;
        if self.modal.thread_search_query() != Some(pending.query.as_str()) {
            return false;
        }
        self.send_daemon_command(DaemonCommand::SearchThreads {
            query: pending.query,
        });
        true
    }

    pub(crate) fn handle_thread_search_results(&mut self, results: ThreadSearchResultsVm) {
        if self.modal.thread_search_query() != Some(results.query.as_str()) {
            return;
        }
        let hits = results
            .results
            .into_iter()
            .map(|result| {
                let updated_at = u64::try_from(result.updated_at).unwrap_or(0);
                modal::ThreadSearchHit {
                    thread: chat::AgentThread {
                        title: if result.title.trim().is_empty() {
                            result.thread_id.clone()
                        } else {
                            result.title
                        },
                        id: result.thread_id,
                        created_at: updated_at,
                        updated_at,
                        ..Default::default()
                    },
                    snippet: result
                        .snippet
                        .map(|snippet| snippet.split_whitespace().collect::<Vec<_>>().join(" "))
                        .filter(|snippet| !snippet.is_empty()),
                }
            })
            .collect();
        self.modal.set_thread_search(modal::ThreadSearchState {
            query: results.query,
            semantic: results.semantic,
            semantic_error: results.semantic_error,
            hits,
        });
        self.sync_thread_picker_item_count();
    }
}

#[cfg(test)]
#[path = "tests/thread_search.rs"]
mod tests;
