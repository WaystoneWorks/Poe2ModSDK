//! The engine of the `poe-mod` command: project templates, manifest check, reproducible
//! packing and package verification. The manifest rules and the package check are the shared
//! ones of `poe-hook-mods` (the same code the launcher and the server run).

mod pack;
mod template;

use std::path::Path;

use poe_hook_mods::{Limits, MANIFEST_FILE, parse_manifest, verify_package};

pub use pack::{PackOptions, pack};
pub use template::{Kind, NewOptions, SdkSource, new_project, render};

/// `poe-mod verify`: the package check with a human-readable report.
pub fn verify_zip(zip: &Path) -> Result<String, Vec<String>> {
    let r = verify_package(zip, &Limits::default())?;
    Ok(report_text(&r))
}

pub(crate) fn report_text(r: &poe_hook_mods::PackageReport) -> String {
    use std::fmt::Write;
    let m = &r.manifest;
    let mut s = String::new();
    let _ = writeln!(s, "{} {} (side: {})", m.id, m.version, m.side.as_str());
    let _ = writeln!(s, "sha256 {}", r.sha256);
    let _ = writeln!(s, "{} bytes, {} files", r.size, r.files.len());
    for f in &r.files {
        let _ = writeln!(s, "  {:>10}  {}", f.size, f.path);
    }
    for w in &r.warnings {
        let _ = writeln!(s, "warning: {w}");
    }
    let _ = write!(s, "ok");
    s
}

/// `poe-mod check`: validates `<dir>/mod.toml` and says which files a package will need.
pub fn check_dir(dir: &Path) -> Result<String, Vec<String>> {
    use std::fmt::Write;
    let path = dir.join(MANIFEST_FILE);
    let text = std::fs::read_to_string(&path)
        .map_err(|e| vec![format!("cannot read {}: {e}", path.display())])?;
    let parsed = parse_manifest(&text).map_err(|e| {
        e.into_iter()
            .map(|m| format!("{MANIFEST_FILE}: {m}"))
            .collect::<Vec<_>>()
    })?;
    let m = &parsed.manifest;
    let mut s = String::new();
    let _ = writeln!(
        s,
        "{} {} (side: {}, manifest v{})",
        m.id,
        m.version,
        m.side.as_str(),
        m.manifest_version
    );
    let _ = writeln!(
        s,
        "package: {}",
        poe_hook_mods::package_file_name(&m.id, &m.version)
    );
    for rel in m.referenced_paths() {
        let state = if dir.join(rel).exists() {
            "found"
        } else {
            "not in the folder (take it from a build with --artifacts or --add)"
        };
        let _ = writeln!(s, "  {rel}: {state}");
    }
    for k in &parsed.unknown_keys {
        let _ = writeln!(s, "warning: unknown key {k}");
    }
    let _ = write!(s, "ok");
    Ok(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_reports_files_and_errors() {
        let t = tempfile::tempdir().unwrap();
        std::fs::write(
            t.path().join("mod.toml"),
            "manifest_version = 2\nid = \"ab\"\nname = \"x\"\nversion = \"1.0.0\"\nside = \"server\"\n[[commands]]\nname = \"hi\"\nreply = \"x\"\n",
        )
        .unwrap();
        let out = check_dir(t.path()).unwrap();
        assert!(
            out.contains("ab 1.0.0 (side: server, manifest v2)"),
            "{out}"
        );
        assert!(out.contains("ab-1.0.0.zip"));
        std::fs::write(t.path().join("mod.toml"), "id = [").unwrap();
        assert!(check_dir(t.path()).unwrap_err()[0].contains("not valid TOML"));
        assert!(check_dir(&t.path().join("nowhere")).is_err());
    }
}
