// Copyright The Glide Authors
// SPDX-License-Identifier: MIT OR Apache-2.0

fn main() {
    println!(
        "cargo:rustc-link-search=framework={}",
        "/System/Library/PrivateFrameworks"
    );
}
