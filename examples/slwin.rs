use std::{
    cell::RefCell,
    ffi::c_void,
    future::pending,
    ptr::null_mut,
    rc::{Rc, Weak},
};

use core_foundation::runloop::kCFRunLoopAllActivities;
use core_graphics::base::CGError;
use glide_wm::sys::{
    app::ProcessInfo,
    executor::Executor,
    window_server::{self, SkylightNotifier, WindowServerId, kCGSWindowIsTerminated},
};
use objc2::MainThreadMarker;
use objc2_core_foundation::{
    CFRunLoop, CFRunLoopActivity, CFRunLoopObserver, kCFRunLoopCommonModes,
};
use objc2_core_graphics::CGSetLocalEventsSuppressionInterval;
use tracing::{debug, info, trace, warn};

struct Watcher {
    #[expect(dead_code)]
    create_notifier: SkylightNotifier,
    destroy_notifier: SkylightNotifier,
}

#[expect(non_upper_case_globals)]
const kCGSWindowDidCreate: u32 = 811;

impl Watcher {
    fn new() -> Rc<RefCell<Self>> {
        Rc::new_cyclic(|state: &Weak<RefCell<Self>>| {
            let create_notifier =
                SkylightNotifier::new_for_event(kCGSWindowDidCreate, Self::make_handler(state))
                    .expect("Initializing SkylightNotifier 1");
            let mut destroy_notifier =
                SkylightNotifier::new_for_event(kCGSWindowIsTerminated, Self::make_handler(state))
                    .expect("Initializing SkylightNotifier 2");
            for win in window_server::get_visible_windows_with_layer(Some(0)) {
                if let Err(e) = destroy_notifier.add_window(win.id) {
                    warn!("failed to add window: {e:?}");
                }
            }
            RefCell::new(Watcher {
                create_notifier,
                destroy_notifier,
            })
        })
    }

    fn make_handler(state: &Weak<RefCell<Self>>) -> impl Fn(u32, &[u8]) + 'static {
        let state = state.clone();
        move |event, data| {
            #[expect(non_upper_case_globals)]
            match event {
                kCGSWindowIsTerminated | kCGSWindowDidCreate => (),
                _ => {
                    println!("Got unexpected event {event}");
                    return;
                }
            }
            assert_eq!(data.len(), size_of::<WindowServerId>());
            // SAFETY: We just asserted the correct size.
            let wsid: WindowServerId = unsafe { *data.as_ptr().cast() };
            let Some(state) = state.upgrade() else {
                println!("could not upgrade state in callback");
                return;
            };
            state.borrow_mut().on_event(event, wsid);
        }
    }

    fn on_event(&mut self, event: u32, wsid: WindowServerId) {
        #[expect(non_upper_case_globals)]
        match event {
            kCGSWindowDidCreate => {
                let Some(info) = window_server::get_window(wsid) else {
                    warn!("saw new window {wsid:?} but couldn't get info for it");
                    return;
                };
                if let Ok(proc_info) = ProcessInfo::for_pid(info.pid)
                    && proc_info.is_xpc
                {
                    trace!("filtering out window {wsid:?} for xpc service {}", info.pid);
                    return;
                }
                println!("window created: {wsid:?}");
                println!("=> {info:?}");
                if let Err(e) = self.destroy_notifier.add_window(wsid) {
                    warn!("failed to add window: {e:?}");
                }
            }
            kCGSWindowIsTerminated => {
                info!("window destroyed: {wsid:?}");
                self.destroy_notifier.on_window_destroyed(wsid);
            }
            _ => unreachable!(),
        }
    }
}

async fn timer_task() {
    let mut timer = glide_wm::sys::timer::Timer::new(0.0, 4.0);
    while let Some(()) = timer.next().await {
        tracing::info!("timer fired");
    }
}

unsafe extern "C" {
    safe fn CGEnableEventStateCombining(combineState: bool) -> CGError;
}

fn main() {
    glide_wm::log::init_logging();
    dbg!(CGSetLocalEventsSuppressionInterval(0.0));
    dbg!(CGEnableEventStateCombining(false));
    let _watcher = Watcher::new();
    let observer = unsafe {
        CFRunLoopObserver::new(None, kCFRunLoopAllActivities, true, 0, Some(obs), null_mut())
    };
    CFRunLoop::current()
        .unwrap()
        .add_observer(observer.as_deref(), unsafe { kCFRunLoopCommonModes });
    Executor::run_main(MainThreadMarker::new().unwrap(), pending());
}

unsafe extern "C-unwind" fn obs(_: *mut CFRunLoopObserver, act: CFRunLoopActivity, _: *mut c_void) {
    debug!("runloop: {act:?}");
}
