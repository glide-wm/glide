// Copyright The Glide Authors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Post-install/update flow for obtaining accessibility permissions.

use std::thread::sleep;
use std::time::Duration;

use accessibility_sys::{AXIsProcessTrustedWithOptions, kAXTrustedCheckOptionPrompt};
use core_foundation::base::TCFType;
use core_foundation::boolean::CFBoolean;
use core_foundation::string::CFString;
use core_graphics::display::CFDictionary;
use objc2::rc::Retained;
use objc2::{MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{
    NSAlert, NSAlertFirstButtonReturn, NSAlertSecondButtonReturn, NSApplicationActivationOptions,
    NSButton, NSRunningApplication,
};
use objc2_foundation::{NSObject, NSString, ns_string};
use tracing::{error, info, warn};

use crate::sys::bundle::{MustExit, relaunch_current_bundle};
use crate::sys::event::HotkeyManager;

pub struct PermissionNotGranted;

pub fn obtain_permissions(mtm: MainThreadMarker) -> Result<(), PermissionNotGranted> {
    obtain_ax_permissions(mtm)?;
    check_input_permissions(mtm)
}

fn obtain_ax_permissions(mtm: MainThreadMarker) -> Result<(), PermissionNotGranted> {
    if check_ax(false) {
        return Ok(());
    }

    let alert = NSAlert::new(mtm);
    alert.setMessageText(ns_string!("Grant accessibility permissions"));
    alert.setInformativeText(&NSString::from_str(&format!(
        "\
        Glide needs permission to access accessibility APIs in order to \
        function.

        1.  Hit the button below to request permissions.
        2. Choose \"𝗢𝗽𝗲𝗻 𝗦𝘆𝘀𝘁𝗲𝗺 𝗦𝗲𝘁𝘁𝗶𝗻𝗴𝘀\".
        3. Click the 𝗿𝗮𝗱𝗶𝗼 𝗯𝘂𝘁𝘁𝗼𝗻 𝗻𝗲𝘅𝘁 𝘁𝗼 𝗚𝗹𝗶𝗱𝗲 to enable it.

        𝗜𝗳 𝗚𝗹𝗶𝗱𝗲 𝗶𝘀 𝗮𝗹𝗿𝗲𝗮𝗱𝘆 𝗲𝗻𝗮𝗯𝗹𝗲𝗱, 𝘀𝗲𝗹𝗲𝗰𝘁 𝗶𝘁 𝗮𝗻𝗱 𝗵𝗶𝘁 𝘁𝗵𝗲 𝗺𝗶𝗻𝘂𝘀 𝘀𝗶𝗴𝗻 (-) 𝗯𝗲𝗹𝗼𝘄 \
        𝘁𝗵𝗲 𝗹𝗶𝘀𝘁 𝘁𝗼 𝗿𝗲𝗺𝗼𝘃𝗲 𝘁𝗵𝗲 𝗼𝗹𝗱 𝘃𝗲𝗿𝘀𝗶𝗼𝗻. Then, click \"Request \
        Permissions\" again and follow the steps above.

        Once permissions are granted, select \"I approved\" below to continue.
        "
    )));

    let request_action = RequestAXPermissionsAction::new(mtm);
    // SAFETY: action outlives button and the selector is valid.
    let request_button = unsafe {
        NSButton::buttonWithTitle_target_action(
            ns_string!("Request Permissions"),
            Some(&request_action),
            Some(sel!(requestPermissions:)),
            mtm,
        )
    };
    alert.setAccessoryView(Some(&request_button));

    let first_button = alert.addButtonWithTitle(ns_string!("I approved"));
    alert.addButtonWithTitle(ns_string!("Quit"));

    // Highlight the Request Permissions button as the default.
    request_button.setKeyEquivalent(ns_string!("\r"));
    first_button.setKeyEquivalent(ns_string!(""));

    match alert.runModal() {
        r if r == NSAlertFirstButtonReturn => (),
        r if r == NSAlertSecondButtonReturn => return Err(PermissionNotGranted),
        _ => error!("Unexpected button press"),
    }

    if check_ax(false) {
        // Permissions all work, but for some reason after showing the NSAlert,
        // our app's windows (the group indicators) don't show up until the user
        // manually activates the app. This is impossible since the app is an
        // accessory, but even if not it's a bad experience, so we attempt to
        // relaunch instead.
        match relaunch_current_bundle() {
            Ok(MustExit) => {
                info!("Relaunch succeeded; exiting");
                std::process::exit(0);
            }
            Err(e) => warn!("{e}"),
        }
        Ok(())
    } else {
        error!("Not trusted; trying again");
        obtain_ax_permissions(mtm)
    }
}

define_class!(
    // SAFETY:
    // - The superclass NSObject does not have any subclassing requirements.
    // - `RequestPermissionsAction` does not implement `Drop`.
    #[unsafe(super = NSObject)]
    #[thread_kind = MainThreadOnly]
    struct RequestAXPermissionsAction;

    impl RequestAXPermissionsAction {
        #[unsafe(method(requestPermissions:))]
        fn request_permissions(&self, _sender: &NSButton) {
            check_ax(true);
            raise_dialog();
        }
    }
);

impl RequestAXPermissionsAction {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(());
        // SAFETY: The signature of `NSObject`'s `init` method is correct.
        unsafe { msg_send![super(this), init] }
    }
}

fn check_ax(prompt: bool) -> bool {
    let options = CFDictionary::from_CFType_pairs(&[(
        unsafe { CFString::wrap_under_create_rule(kAXTrustedCheckOptionPrompt) },
        CFBoolean::from(prompt),
    )]);
    unsafe { AXIsProcessTrustedWithOptions(options.as_concrete_TypeRef()) }
}

fn raise_dialog() {
    // The permissions dialog can pop up behind our alert dialog, so
    // try to raise it above in case it does.
    for _ in 0..20 {
        sleep(Duration::from_millis(50));
        let mut app_found = false;
        for app in NSRunningApplication::runningApplicationsWithBundleIdentifier(ns_string!(
            "com.apple.accessibility.universalAccessAuthWarn"
        )) {
            app_found = true;
            app.activateFromApplication_options(
                &NSRunningApplication::currentApplication(),
                NSApplicationActivationOptions::empty(),
            );
        }
        if app_found {
            break;
        }
    }
    warn!("Couldn't find access request app to raise after 1s");
}

fn check_input_permissions(mtm: MainThreadMarker) -> Result<(), PermissionNotGranted> {
    // NOTE(tmandry): IOHIDCheckAccess is useless for checking after
    // accessibility is granted. If the app is restarted it works correctly,
    // otherwise it reports no access, even though starting the event tap works.
    // So we just check by starting the event tap. Note that the
    // IOHIDRequestAccess API also exists, but never produced a prompt in my
    // testing on macOS 26. It's also unnecessary once we have accessibility
    // permissions.
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    let Err(err) = HotkeyManager::new(tx) else {
        return Ok(());
    };
    error!("Not trusted for input access; err={err:?}");
    let alert = NSAlert::new(mtm);
    alert.setMessageText(ns_string!("Input monitoring permissions not granted"));
    alert.setInformativeText(ns_string!(
        "Key bindings will not work.

        Input monitoring should be included as part of accessibility \
        permissions, but Glide was not granted permission for some reason.

        Try going to System Settings > Privacy & Security > Input Monitoring \
        to see if Glide is listed, and grant it permissions.

        Please file a bug so we can investigate further: \
        https://github.com/glide-wm/glide/issues
        "
    ));
    alert.addButtonWithTitle(ns_string!("Quit"));
    alert.addButtonWithTitle(ns_string!("Ignore"));
    match alert.runModal() {
        r if r == NSAlertFirstButtonReturn => (),
        r if r == NSAlertSecondButtonReturn => {
            warn!("User chose to ignore missing input monitoring permission");
            return Ok(());
        }
        _ => error!("Unexpected button press"),
    }
    Err(PermissionNotGranted)
}
