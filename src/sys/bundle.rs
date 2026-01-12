// Copyright The Glide Authors
// SPDX-License-Identifier: MIT OR Apache-2.0

use std::process::Command;

use anyhow::bail;
use objc2::rc::Retained;
use objc2_foundation::{NSBundle, NSString, ns_string};

pub enum BundleError {
    NotInBundle,
    BundleNotGlide { identifier: Retained<NSString> },
}

pub fn glide_bundle() -> Result<Retained<NSBundle>, BundleError> {
    let bundle = NSBundle::mainBundle();
    match bundle.bundleIdentifier() {
        None => Err(BundleError::NotInBundle),
        Some(identifier) if !identifier.containsString(ns_string!("glidewm")) => {
            Err(BundleError::BundleNotGlide { identifier })
        }
        Some(_) => Ok(bundle),
    }
}

pub fn launch(bundle: &NSBundle) -> anyhow::Result<()> {
    launch_inner(bundle, false)
}

pub fn relaunch_current_bundle() -> anyhow::Result<MustExit> {
    let Ok(bundle) = glide_bundle() else {
        bail!("Skipping relaunch because the current application is not Glide");
    };
    launch_inner(&bundle, true).map(|()| MustExit)
}

#[must_use = "Callers must immediately exit the process after reporting success"]
pub struct MustExit;
impl Drop for MustExit {
    fn drop(&mut self) {
        panic!("Must exit after relaunch");
    }
}

fn launch_inner(bundle: &NSBundle, relaunch: bool) -> anyhow::Result<()> {
    let path = bundle.bundlePath().to_string();
    let mut cmd = Command::new("/usr/bin/open");
    if relaunch {
        cmd.arg("-n");
    }
    match cmd.arg(path).output() {
        Ok(out) if out.status.success() => Ok(()),
        Ok(out) => bail!(
            "Launch failed with code {status}. stderr:\n{stderr}\n\nstdout:\n{stdout}",
            status = out.status,
            stderr = String::from_utf8_lossy(&out.stderr),
            stdout = String::from_utf8_lossy(&out.stdout)
        ),
        Err(err) => bail!("Relaunch failed with error: {err}"),
    }
}
