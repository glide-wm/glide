//! Manages raise requests.
//!
//! This actor sits behind the Reactor and is responsible for managing raise requests.
//! It ensures that windows are raised in the correct order and handles timeouts.

use crate::actor::app::{AppThreadHandle, Quiet, Request, WindowId};
use crate::actor::reactor::Sender;
use crate::actor::{mouse, reactor};
use crate::sys::timer::Timer;
use objc2_core_foundation::CGPoint;
use rustc_hash::FxHashMap as HashMap;
use std::collections::{HashSet, VecDeque};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering::{Acquire, Release};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tracing::{Span, debug, info, trace, warn};

#[derive(Clone, Debug)]
pub struct RaiseToken {
    cancelled: Arc<AtomicBool>,
}

impl RaiseToken {
    pub fn new() -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Acquire)
    }

    fn cancel(&self) {
        self.cancelled.store(true, Release);
    }
}

/// Messages that can be sent to the raise manager
#[derive(Debug)]
pub enum Event {
    RaiseRequest(RaiseRequest),
    RaiseCompleted {
        window_id: WindowId,
        sequence_id: u64,
    },
    RaiseTimeout {
        sequence_id: u64,
    },
}

/// A queued layout response waiting to be processed.
#[derive(Debug)]
pub struct RaiseRequest {
    pub raise_windows: Vec<WindowId>,
    pub focus_window: Option<(WindowId, Option<CGPoint>)>,
    pub app_handles: HashMap<i32, AppThreadHandle>,
}

pub struct RaiseManager {
    /// The currently active sequence, if any
    active_sequence: Option<ActiveSequence>,
    /// Queued sequences waiting to be processed
    queued_sequences: VecDeque<RaiseRequest>,
    next_sequence_id: u64,
    mouse_tx: Option<mouse::Sender>,
}

/// Tracks an executing sequence of raises.
#[derive(Debug)]
pub struct ActiveSequence {
    sequence_id: u64,
    pending_raises: HashSet<WindowId>,
    focus_window: Option<(WindowId, Option<CGPoint>)>,
    app_handles: HashMap<i32, AppThreadHandle>,
    raise_token: RaiseToken,
    started_at: Instant,
    timed_out: bool,
}

const TIMEOUT_DURATION: Duration = Duration::from_millis(250);

impl RaiseManager {
    /// Run the raise manager task.
    pub async fn run(
        mut rx: mpsc::UnboundedReceiver<Event>,
        events_tx: Sender,
        mouse_tx: Option<mouse::Sender>,
    ) {
        let mut raise_manager = RaiseManager::new();
        raise_manager.mouse_tx = mouse_tx;
        let mut timeout_timer = Timer::manual();

        let sequence_timeout = |sequence: &ActiveSequence| {
            if !sequence.timed_out {
                let elapsed = sequence.started_at.elapsed();
                TIMEOUT_DURATION.saturating_sub(elapsed)
            } else {
                Duration::MAX
            }
        };

        loop {
            // Calculate next timeout timer if we have an active sequence.
            let timeout = if let Some(sequence) = &raise_manager.active_sequence {
                sequence_timeout(sequence)
            } else {
                Duration::MAX
            };
            timeout_timer.set_next_fire(timeout);

            tokio::select! {
                // Handle messages from reactor
                Some(msg) = rx.recv() => {
                    raise_manager.handle_message(msg);
                }

                // Handle timeout - send timeout event to reactor
                _ = timeout_timer.next() => {
                    // Send timeout event for the active sequence
                    if let Some(sequence) = &mut raise_manager.active_sequence {
                        if sequence_timeout(sequence) <= Duration::ZERO {
                            // Send timeout event to reactor; this will get
                            // relayed back to us. We send these events through
                            // the reactor so that we can record/replay them.
                            sequence.timed_out = true;
                            let _ = events_tx.send((
                                tracing::Span::current(),
                                reactor::Event::RaiseTimeout { sequence_id: sequence.sequence_id }
                            ));
                        }
                    }
                }
            }
        }
    }

    fn new() -> Self {
        Self {
            active_sequence: None,
            queued_sequences: VecDeque::new(),
            next_sequence_id: 1,
            mouse_tx: None,
        }
    }

    fn handle_message(&mut self, msg: Event) {
        match msg {
            Event::RaiseRequest(RaiseRequest {
                raise_windows,
                focus_window,
                app_handles,
            }) => {
                debug!(
                    "Processing layout response with {} raise_windows",
                    raise_windows.len()
                );

                // Always queue the sequence
                self.queued_sequences.push_back(RaiseRequest {
                    raise_windows,
                    focus_window,
                    app_handles,
                });
            }
            Event::RaiseCompleted { window_id, sequence_id } => {
                trace!("Raise completed for {:?} in sequence {}", window_id, sequence_id);

                // Remove this window from the active sequence's pending raises
                if let Some(sequence) = &mut self.active_sequence {
                    if sequence.sequence_id == sequence_id {
                        sequence.pending_raises.remove(&window_id);
                    }
                }
            }
            Event::RaiseTimeout { sequence_id } => {
                trace!("Raise sequence {} timed out", sequence_id);

                // Clear pending raises for the active sequence if it matches
                if let Some(sequence) = &mut self.active_sequence {
                    if sequence.sequence_id == sequence_id {
                        warn!(
                            "Sequence {} timed out, clearing pending raises: {:?}",
                            sequence_id, sequence.pending_raises
                        );
                        sequence.pending_raises.clear();
                        sequence.raise_token.cancel();
                    }
                }
            }
        }

        // Process sequences until no more progress can be made (fixed-point iteration)
        loop {
            let mut changed = false;
            changed |= self.process_active_sequence();
            changed |= self.process_queued_responses();
            if !changed {
                break;
            }
        }
    }

    fn process_queued_responses(&mut self) -> bool {
        if self.active_sequence.is_none() {
            if let Some(queued) = self.queued_sequences.pop_front() {
                self.start_new_sequence(queued);
                return true;
            }
        }
        false
    }

    fn start_new_sequence(
        &mut self,
        RaiseRequest {
            raise_windows,
            focus_window,
            app_handles,
        }: RaiseRequest,
    ) {
        let sequence_id = self.next_sequence_id;
        self.next_sequence_id += 1;

        // Send all raise requests with completion notification
        let mut pending_raises = HashSet::default();
        let raise_token = RaiseToken::new();

        for wid in raise_windows {
            if let Some(app_handle) = app_handles.get(&wid.pid) {
                if app_handle
                    .send(Request::Raise(wid, raise_token.clone(), sequence_id, Quiet::Yes))
                    .is_ok()
                {
                    pending_raises.insert(wid);
                }
            } else {
                // App not found
                warn!("App not found for window {:?}", wid);
            }
        }

        if !pending_raises.is_empty() || focus_window.is_some() {
            self.active_sequence = Some(ActiveSequence {
                sequence_id,
                pending_raises,
                focus_window,
                app_handles,
                raise_token,
                started_at: Instant::now(),
                timed_out: false,
            });
        }
    }

    /// Process the active sequence, handling completed raises and focus windows
    pub fn process_active_sequence(&mut self) -> bool {
        let Some(sequence) = &mut self.active_sequence else {
            return false;
        };
        let mut changed = false;

        // If all regular raises are complete but we have a focus window, send
        // the focus request.
        if sequence.pending_raises.is_empty() && sequence.focus_window.is_some() {
            changed = true;
            let (wid, warp) = sequence.focus_window.take().unwrap();
            info!(focus_window = ?wid);
            let app_handle = sequence.app_handles.get(&wid.pid);
            if let Some(handle) = app_handle {
                if handle
                    .send(Request::Raise(
                        wid,
                        sequence.raise_token.clone(),
                        sequence.sequence_id, // Use proper sequence ID for tracking
                        Quiet::No,
                    ))
                    .is_ok()
                {
                    // Add focus window to pending raises so we wait for completion
                    sequence.pending_raises.insert(wid);
                    trace!("Focus window request sent and added to pending raises");
                } else {
                    warn!("Failed to send focus window request");
                }

                if let Some(warp) = warp {
                    if let Some(mouse_tx) = &self.mouse_tx {
                        _ = mouse_tx.send((Span::current(), mouse::Request::Warp(warp)));
                    }
                }
            }
        }

        // If all raises (including focus) are complete, remove the active sequence
        if sequence.pending_raises.is_empty() && sequence.focus_window.is_none() {
            self.active_sequence = None;
            changed = true;
        }

        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actor::app::{AppThreadHandle, WindowId};
    use crate::sys::executor::Executor;
    use tokio::sync::mpsc;

    fn create_test_app_handles() -> (
        HashMap<i32, AppThreadHandle>,
        mpsc::UnboundedReceiver<(Span, Request)>,
    ) {
        let mut app_handles = HashMap::default();
        let (app_tx, app_rx) = mpsc::unbounded_channel();
        let app_handle = AppThreadHandle::new_for_test(app_tx);
        app_handles.insert(1, app_handle);
        (app_handles, app_rx)
    }

    fn create_layout_response(
        raise_windows: Vec<WindowId>,
        focus_window: Option<(WindowId, Option<CGPoint>)>,
        app_handles: HashMap<i32, AppThreadHandle>,
    ) -> Event {
        Event::RaiseRequest(RaiseRequest {
            raise_windows,
            focus_window,
            app_handles,
        })
    }

    fn collect_requests(app_rx: &mut mpsc::UnboundedReceiver<(Span, Request)>) -> Vec<Request> {
        let mut requests = Vec::new();
        while let Ok((_, request)) = app_rx.try_recv() {
            requests.push(request);
        }
        requests
    }

    fn assert_raise_request(
        request: &Request,
        expected_wid: WindowId,
        expected_seq_id: u64,
        expected_quiet: Quiet,
    ) {
        if let Request::Raise(wid, _, seq_id, quiet) = request {
            assert_eq!(*wid, expected_wid);
            assert_eq!(*seq_id, expected_seq_id);
            assert_eq!(*quiet, expected_quiet);
        } else {
            panic!("Expected raise request, got: {:?}", request);
        }
    }

    fn find_raise_request(requests: &[Request], expected_wid: WindowId) -> bool {
        requests.iter().any(|r| {
            if let Request::Raise(wid, _, _, quiet) = r {
                *wid == expected_wid && *quiet == Quiet::No
            } else {
                false
            }
        })
    }

    #[test]
    fn test_raise_manager_handles_layout_response() {
        Executor::run(async {
            let mut raise_manager = RaiseManager::new();
            let (app_handles, _app_rx) = create_test_app_handles();

            let msg = create_layout_response(
                vec![WindowId::new(1, 1), WindowId::new(1, 2)],
                Some((WindowId::new(1, 3), None)),
                app_handles,
            );

            // Handle the message synchronously
            raise_manager.handle_message(msg);
            // Verify that an active sequence was created
            assert!(raise_manager.active_sequence.is_some());
            let sequence = raise_manager.active_sequence.as_ref().unwrap();
            assert_eq!(sequence.sequence_id, 1);
            assert_eq!(sequence.pending_raises.len(), 2);
            assert!(sequence.focus_window.is_some());
        });
    }

    #[test]
    fn test_raise_completion_removes_pending_window() {
        Executor::run(async {
            let mut raise_manager = RaiseManager::new();
            let (app_handles, _app_rx) = create_test_app_handles();

            let layout_msg = create_layout_response(
                vec![WindowId::new(1, 1), WindowId::new(1, 2)],
                Some((WindowId::new(1, 3), None)),
                app_handles,
            );

            raise_manager.handle_message(layout_msg);

            // Verify initial state
            assert!(raise_manager.active_sequence.is_some());
            assert_eq!(
                raise_manager.active_sequence.as_ref().unwrap().pending_raises.len(),
                2
            );

            // Send completion for one window
            let completion_msg = Event::RaiseCompleted {
                window_id: WindowId::new(1, 1),
                sequence_id: 1,
            };

            raise_manager.handle_message(completion_msg);

            // Verify that one window was removed from pending
            assert!(raise_manager.active_sequence.is_some());
            let sequence = raise_manager.active_sequence.as_ref().unwrap();
            assert_eq!(sequence.pending_raises.len(), 1);
            assert!(!sequence.pending_raises.contains(&WindowId::new(1, 1)));
            assert!(sequence.pending_raises.contains(&WindowId::new(1, 2)));
        });
    }

    #[test]
    fn test_timeout_clears_pending_raises() {
        Executor::run(async {
            let mut raise_manager = RaiseManager::new();
            let (app_handles, _app_rx) = create_test_app_handles();

            let layout_msg = create_layout_response(vec![WindowId::new(1, 1)], None, app_handles);

            raise_manager.handle_message(layout_msg);

            // Verify initial state
            assert!(raise_manager.active_sequence.is_some());
            assert_eq!(
                raise_manager.active_sequence.as_ref().unwrap().pending_raises.len(),
                1
            );

            // Send timeout directly
            raise_manager.handle_message(Event::RaiseTimeout { sequence_id: 1 });

            // Verify that the sequence was completed
            assert!(raise_manager.active_sequence.is_none());
        });
    }

    #[test]
    fn test_all_raises_complete_triggers_focus() {
        Executor::run(async {
            let mut raise_manager = RaiseManager::new();
            let (mouse_tx, mut mouse_rx) = mpsc::unbounded_channel();
            raise_manager.mouse_tx = Some(mouse_tx);

            let (app_handles, mut app_rx) = create_test_app_handles();

            let layout_msg = create_layout_response(
                vec![WindowId::new(1, 1), WindowId::new(1, 2)],
                Some((WindowId::new(1, 3), Some(CGPoint::new(100.0, 200.0)))),
                app_handles,
            );

            raise_manager.handle_message(layout_msg);

            // Verify initial state
            assert!(raise_manager.active_sequence.is_some());
            assert_eq!(
                raise_manager.active_sequence.as_ref().unwrap().pending_raises.len(),
                2
            );

            // Send completions for both raise windows
            raise_manager.handle_message(Event::RaiseCompleted {
                window_id: WindowId::new(1, 1),
                sequence_id: 1,
            });

            raise_manager.handle_message(Event::RaiseCompleted {
                window_id: WindowId::new(1, 2),
                sequence_id: 1,
            });

            // Check that focus window request was sent
            let requests = collect_requests(&mut app_rx);
            assert!(
                find_raise_request(&requests, WindowId::new(1, 3)),
                "Focus window request should have been sent"
            );

            // Check that warp request was sent
            let warp_request = mouse_rx.try_recv().expect("Warp request should have been sent");
            let mouse::Request::Warp(warp) = warp_request.1 else {
                panic!("Unexpected mouse request sent: {:?}", warp_request.1)
            };
            assert_eq!(warp, CGPoint::new(100.0, 200.0));

            // Complete the focus window raise
            raise_manager.handle_message(Event::RaiseCompleted {
                window_id: WindowId::new(1, 3),
                sequence_id: 1,
            });

            // Verify that the sequence was completed and removed
            assert!(raise_manager.active_sequence.is_none());
        });
    }

    #[test]
    fn test_timeout_clears_pending_and_triggers_focus() {
        Executor::run(async {
            let mut raise_manager = RaiseManager::new();
            let (app_handles, mut app_rx) = create_test_app_handles();

            let layout_msg = create_layout_response(
                vec![WindowId::new(1, 1), WindowId::new(1, 2)],
                Some((WindowId::new(1, 3), None)),
                app_handles,
            );

            raise_manager.handle_message(layout_msg);

            // Verify initial state
            assert!(raise_manager.active_sequence.is_some());
            assert_eq!(
                raise_manager.active_sequence.as_ref().unwrap().pending_raises.len(),
                2
            );

            // Send timeout for the sequence
            raise_manager.handle_message(Event::RaiseTimeout { sequence_id: 1 });

            // Check that focus window request was sent after timeout
            let requests = collect_requests(&mut app_rx);
            assert!(
                find_raise_request(&requests, WindowId::new(1, 3)),
                "Focus window request should have been sent after timeout"
            );

            // Complete the focus window raise
            raise_manager.handle_message(Event::RaiseCompleted {
                window_id: WindowId::new(1, 3),
                sequence_id: 1,
            });

            // Verify that the sequence was completed and removed
            assert!(raise_manager.active_sequence.is_none());
        });
    }

    #[test]
    fn test_multiple_layout_responses_wait_for_focus_completion() {
        Executor::run(async {
            let mut raise_manager = RaiseManager::new();
            let (app_handles, mut app_rx) = create_test_app_handles();

            // Send two layout responses - second should be queued
            let msg1 = create_layout_response(
                vec![WindowId::new(1, 1)],
                Some((WindowId::new(1, 2), None)),
                app_handles.clone(),
            );
            let msg2 = create_layout_response(
                vec![WindowId::new(1, 3)],
                Some((WindowId::new(1, 4), None)),
                app_handles.clone(),
            );

            raise_manager.handle_message(msg1);
            raise_manager.handle_message(msg2);

            // Verify sequential processing: first active, second queued
            assert!(raise_manager.active_sequence.is_some());
            assert_eq!(raise_manager.active_sequence.as_ref().unwrap().sequence_id, 1);
            assert_eq!(raise_manager.queued_sequences.len(), 1);

            // Complete first sequence's regular raise
            raise_manager.handle_message(Event::RaiseCompleted {
                window_id: WindowId::new(1, 1),
                sequence_id: 1,
            });

            // Verify first sequence now has focus pending, second still queued
            let sequence = raise_manager.active_sequence.as_ref().unwrap();
            assert_eq!(sequence.pending_raises.len(), 1);
            assert!(sequence.pending_raises.contains(&WindowId::new(1, 2)));
            assert_eq!(raise_manager.queued_sequences.len(), 1);

            // Verify only first sequence requests sent (regular + focus)
            let requests = collect_requests(&mut app_rx);
            assert_eq!(requests.len(), 2);
            assert_raise_request(&requests[0], WindowId::new(1, 1), 1, Quiet::Yes);
            assert_raise_request(&requests[1], WindowId::new(1, 2), 1, Quiet::No);

            // Complete first sequence's focus window
            raise_manager.handle_message(Event::RaiseCompleted {
                window_id: WindowId::new(1, 2),
                sequence_id: 1,
            });

            // Verify second sequence now active
            let sequence = raise_manager.active_sequence.as_ref().unwrap();
            assert_eq!(sequence.sequence_id, 2);
            assert_eq!(sequence.pending_raises.len(), 1);
            assert!(sequence.pending_raises.contains(&WindowId::new(1, 3)));
            assert_eq!(raise_manager.queued_sequences.len(), 0);

            // Verify second sequence's regular raise sent
            let requests = collect_requests(&mut app_rx);
            assert_eq!(requests.len(), 1);
            assert_raise_request(&requests[0], WindowId::new(1, 3), 2, Quiet::Yes);

            // Complete second sequence's regular raise
            raise_manager.handle_message(Event::RaiseCompleted {
                window_id: WindowId::new(1, 3),
                sequence_id: 2,
            });

            // Verify second sequence's focus window sent
            let requests = collect_requests(&mut app_rx);
            assert_eq!(requests.len(), 1);
            assert_raise_request(&requests[0], WindowId::new(1, 4), 2, Quiet::No);

            // Complete second sequence's focus window
            raise_manager.handle_message(Event::RaiseCompleted {
                window_id: WindowId::new(1, 4),
                sequence_id: 2,
            });

            // Verify all sequences completed
            assert!(raise_manager.active_sequence.is_none());
            assert!(collect_requests(&mut app_rx).is_empty());
        });
    }

    #[test]
    fn test_multiple_iterations_required_for_chained_completions() {
        Executor::run(async {
            let mut raise_manager = RaiseManager::new();
            let (app_handles, mut app_rx) = create_test_app_handles();

            // Send three layout responses:
            // 1. First has one regular raise + focus
            // 2. Second has no regular raises, only focus (will start immediately when first completes)
            // 3. Third has one regular raise + focus
            let msg1 = create_layout_response(
                vec![WindowId::new(1, 1)],
                Some((WindowId::new(1, 2), None)),
                app_handles.clone(),
            );
            let msg2 = create_layout_response(
                vec![],
                Some((WindowId::new(1, 3), None)),
                app_handles.clone(),
            );
            let msg3 = create_layout_response(
                vec![WindowId::new(1, 4)],
                Some((WindowId::new(1, 5), None)),
                app_handles.clone(),
            );

            // Queue all three sequences
            raise_manager.handle_message(msg1);
            raise_manager.handle_message(msg2);
            raise_manager.handle_message(msg3);

            // Verify first sequence is active, others queued
            assert!(raise_manager.active_sequence.is_some());
            assert_eq!(raise_manager.active_sequence.as_ref().unwrap().sequence_id, 1);
            assert_eq!(raise_manager.queued_sequences.len(), 2);

            // Complete first sequence's regular raise
            raise_manager.handle_message(Event::RaiseCompleted {
                window_id: WindowId::new(1, 1),
                sequence_id: 1,
            });

            // First sequence should now have focus pending
            assert!(raise_manager.active_sequence.is_some());
            assert_eq!(raise_manager.active_sequence.as_ref().unwrap().sequence_id, 1);
            assert!(
                raise_manager
                    .active_sequence
                    .as_ref()
                    .unwrap()
                    .pending_raises
                    .contains(&WindowId::new(1, 2))
            );

            // Complete first sequence's focus window - this should trigger multiple iterations:
            // 1. First sequence completes
            // 2. Second sequence starts (no regular raises, immediately sends focus)
            // 3. Focus window is sent and tracked
            // Without multiple iterations, the second sequence's focus wouldn't be sent
            raise_manager.handle_message(Event::RaiseCompleted {
                window_id: WindowId::new(1, 2),
                sequence_id: 1,
            });

            // After the completion, we should have:
            // - Second sequence active with focus window pending
            // - Third sequence still queued
            // - Focus request for second sequence should have been sent
            assert!(raise_manager.active_sequence.is_some());
            assert_eq!(raise_manager.active_sequence.as_ref().unwrap().sequence_id, 2);
            assert!(
                raise_manager
                    .active_sequence
                    .as_ref()
                    .unwrap()
                    .pending_raises
                    .contains(&WindowId::new(1, 3))
            );
            assert_eq!(raise_manager.queued_sequences.len(), 1);

            // Verify the focus request for second sequence was sent
            // This would fail with single iteration because the second sequence
            // wouldn't have a chance to send its focus request
            let requests = collect_requests(&mut app_rx);
            let second_focus_sent = requests.iter().any(|r| {
                if let Request::Raise(wid, _, seq_id, quiet) = r {
                    *wid == WindowId::new(1, 3) && *seq_id == 2 && *quiet == Quiet::No
                } else {
                    false
                }
            });
            assert!(
                second_focus_sent,
                "Second sequence's focus request should have been sent immediately after first sequence completed"
            );
        });
    }
}
