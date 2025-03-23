use std::{collections::BTreeMap, io::Write, sync::Arc};

use accessibility_sys::pid_t;
use icrate::Foundation::{CGPoint, CGRect, CGSize};
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use tracing::{debug, Span};

use super::{Event, Reactor, Record, Requested, TransactionId};
use crate::{
    actor::{
        app::{AppThreadHandle, Request, WindowId},
        layout::LayoutManager,
    },
    config::Config,
    sys::{
        app::{AppInfo, WindowInfo},
        geometry::SameAs,
        window_server::{WindowServerId, WindowServerInfo},
    },
};

impl Reactor {
    pub fn new_for_test(layout: LayoutManager) -> Reactor {
        let mut config = Config::default();
        config.settings.default_disable = false;
        config.settings.animate = false;
        let record = Record::new_for_test(tempfile::NamedTempFile::new().unwrap());
        Reactor::new(Arc::new(config), layout, record)
    }

    pub fn handle_events(&mut self, events: Vec<Event>) {
        for event in events {
            self.handle_event(event);
        }
    }
}

impl Drop for Reactor {
    fn drop(&mut self) {
        if std::thread::panicking() {
            return;
        }
        // Replay the recorded data to make sure we can do so without crashing.
        if let Some(temp) = self.record.temp() {
            temp.as_file().flush().unwrap();
            let mut cmd = test_bin::get_test_bin("examples/devtool");
            cmd.arg("replay").arg(temp.path());
            println!("Replaying recorded data:\n{cmd:?}");
            assert!(cmd.spawn().unwrap().wait().unwrap().success(), "replay failed");
        }
    }
}

pub fn make_window(idx: usize) -> WindowInfo {
    WindowInfo {
        is_standard: true,
        title: format!("Window{idx}"),
        frame: CGRect::new(
            CGPoint::new(100.0 * f64::from(idx as u32), 100.0),
            CGSize::new(50.0, 50.0),
        ),
        // TODO: This is wrong and conflicts with windows from other apps.
        sys_id: Some(WindowServerId::new(idx as u32)),
    }
}

pub fn make_windows(count: usize) -> Vec<WindowInfo> {
    (1..=count).map(make_window).collect()
}

pub struct Apps {
    tx: UnboundedSender<(Span, Request)>,
    rx: UnboundedReceiver<(Span, Request)>,
    pub windows: BTreeMap<WindowId, WindowState>,
}

#[derive(Default, PartialEq, Debug, Clone)]
pub struct WindowState {
    pub last_seen_txid: TransactionId,
    pub animating: bool,
    pub frame: CGRect,
}

impl Apps {
    pub fn new() -> Apps {
        let (tx, rx) = unbounded_channel();
        Apps {
            tx,
            rx,
            windows: BTreeMap::new(),
        }
    }

    pub fn make_app(&mut self, pid: pid_t, windows: Vec<WindowInfo>) -> Vec<Event> {
        let frontmost = windows.first().map(|_| WindowId::new(pid, 1));
        self.make_app_with_opts(pid, windows, frontmost, false, true)
    }

    pub fn make_app_with_opts(
        &mut self,
        pid: pid_t,
        windows: Vec<WindowInfo>,
        main_window: Option<WindowId>,
        is_frontmost: bool,
        with_ws_info: bool,
    ) -> Vec<Event> {
        for (id, info) in (1..).map(|idx| WindowId::new(pid, idx)).zip(&windows) {
            self.windows.insert(
                id,
                WindowState {
                    frame: info.frame,
                    ..Default::default()
                },
            );
        }
        let handle = AppThreadHandle::new_for_test(self.tx.clone());
        vec![Event::ApplicationLaunched {
            pid,
            info: AppInfo {
                bundle_id: Some(format!("com.testapp{pid}")),
                localized_name: Some(format!("TestApp{pid}")),
            },
            handle,
            is_frontmost,
            main_window,
            window_server_info: if with_ws_info {
                windows
                    .iter()
                    .map(|info| WindowServerInfo {
                        pid,
                        id: info.sys_id.unwrap(),
                        layer: 0,
                        frame: info.frame,
                    })
                    .collect()
            } else {
                Default::default()
            },
            visible_windows: (1..).map(|idx| WindowId::new(pid, idx)).zip(windows).collect(),
        }]
    }

    pub fn requests(&mut self) -> Vec<Request> {
        let mut requests = Vec::new();
        while let Ok((_, req)) = self.rx.try_recv() {
            requests.push(req);
        }
        requests
    }

    pub fn simulate_until_quiet(&mut self, reactor: &mut Reactor) {
        let mut requests = self.requests();
        while !requests.is_empty() {
            for event in self.simulate_events_for_requests(requests) {
                reactor.handle_event(event);
            }
            requests = self.requests();
        }
    }

    pub fn simulate_events(&mut self) -> Vec<Event> {
        let requests = self.requests();
        self.simulate_events_for_requests(requests)
    }

    pub fn simulate_events_for_requests(&mut self, requests: Vec<Request>) -> Vec<Event> {
        let mut events = vec![];
        let mut got_visible_windows = false;
        for request in requests {
            debug!(?request);
            match request {
                Request::Terminate => break,
                Request::GetVisibleWindows => {
                    // Only do this once per cycle, since we simulate responding
                    // from all apps.
                    if got_visible_windows {
                        continue;
                    }
                    got_visible_windows = true;
                    let mut app_windows = BTreeMap::<pid_t, Vec<WindowId>>::new();
                    for &wid in self.windows.keys() {
                        app_windows.entry(wid.pid).or_default().push(wid);
                    }
                    for (pid, windows) in app_windows {
                        events.push(Event::WindowsDiscovered {
                            pid,
                            new: vec![],
                            known_visible: windows,
                        });
                    }
                }
                Request::SetWindowFrame(wid, frame, txid) => {
                    let window = self.windows.entry(wid).or_default();
                    window.last_seen_txid = txid;
                    let old_frame = window.frame;
                    window.frame = frame;
                    if !window.animating && !old_frame.same_as(frame) {
                        events.push(Event::WindowFrameChanged(
                            wid,
                            frame,
                            txid,
                            Requested(true),
                            None,
                        ));
                    }
                }
                Request::SetWindowPos(wid, pos, txid) => {
                    let window = self.windows.entry(wid).or_default();
                    window.last_seen_txid = txid;
                    let old_frame = window.frame;
                    window.frame.origin = pos;
                    if !window.animating && !old_frame.same_as(window.frame) {
                        events.push(Event::WindowFrameChanged(
                            wid,
                            window.frame,
                            txid,
                            Requested(true),
                            None,
                        ));
                    }
                }
                Request::BeginWindowAnimation(wid) => {
                    self.windows.entry(wid).or_default().animating = true;
                }
                Request::EndWindowAnimation(wid) => {
                    let window = self.windows.entry(wid).or_default();
                    window.animating = false;
                    events.push(Event::WindowFrameChanged(
                        wid,
                        window.frame,
                        window.last_seen_txid,
                        Requested(true),
                        None,
                    ));
                }
                Request::Raise(..) => todo!(),
            }
        }
        debug!(?events);
        events
    }
}
