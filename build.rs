// SPDX-License-Identifier: GPL-3.0-or-later
//! Compiles the Blueprint files to GtkBuilder XML and bundles them, together
//! with the icon and the stylesheet, into a GResource embedded in the binary.

use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

/// Blueprint sources and the path of the generated `.ui` inside the resource.
const BLUEPRINTS: &[(&str, &str)] = &[
    ("data/ui/window.blp", "ui/window.ui"),
    ("data/ui/preferences-dialog.blp", "ui/preferences-dialog.ui"),
    ("data/ui/help-overlay.blp", "gtk/help-overlay.ui"),
];

fn main() {
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("cargo sets OUT_DIR"));
    // Allows pointing at a wrapper when the system blueprint-compiler is broken.
    let compiler = env::var("BLUEPRINT_COMPILER").unwrap_or_else(|_| "blueprint-compiler".into());
    println!("cargo::rerun-if-env-changed=BLUEPRINT_COMPILER");

    for (source, target) in BLUEPRINTS {
        println!("cargo::rerun-if-changed={source}");
        let output = out_dir.join(target);
        if let Some(parent) = output.parent() {
            fs::create_dir_all(parent).expect("create output directory");
        }
        let status = Command::new(&compiler)
            .arg("compile")
            .arg("--output")
            .arg(&output)
            .arg(source)
            .status()
            .unwrap_or_else(|err| {
                panic!("could not run `{compiler}` (install blueprint-compiler): {err}")
            });
        assert!(status.success(), "blueprint-compiler failed on {source}");
    }

    let out = out_dir.to_str().expect("OUT_DIR is valid UTF-8");
    glib_build_tools::compile_resources(
        &[out, "data"],
        "data/myne.gresource.xml",
        "myne.gresource",
    );
}
