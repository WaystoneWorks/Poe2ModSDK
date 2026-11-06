//! Reproducible packing: `<id>-<version>.zip` + `.sha256` from a mod folder.
//!
//! What goes in: `mod.toml` (byte for byte), every file or folder it names (from the mod folder,
//! or from an `--add PATH=FILE` override, or found by file name in an `--artifacts` folder), and
//! README / LICENSE / NOTICE / CHANGELOG files at the root. Nothing else: build output and
//! sources next to the manifest never leak into a package.
//!
//! Reproducible: entries sorted by path, one fixed timestamp (1980-01-01, the zip epoch), fixed
//! permissions (0644), Deflate at a fixed level, no extra fields. The same inputs give the same
//! bytes (with the same tool and zip library version), so the SHA-256 is stable.

use std::{
    collections::BTreeMap,
    io::{Cursor, Write},
    path::{Path, PathBuf},
};

use poe_hook_mods::{
    Limits, MANIFEST_FILE, package_file_name, parse_manifest, sha256_file_text, sha256_hex,
    verify_package,
};
use zip::{CompressionMethod, DateTime, ZipWriter, write::SimpleFileOptions};

pub struct PackOptions {
    /// The mod folder (with `mod.toml`).
    pub mod_dir: PathBuf,
    /// Where the zip and the `.sha256` are written.
    pub out_dir: PathBuf,
    /// `PATH` inside the package, file on disk. Overrides the mod folder.
    pub add: Vec<(String, PathBuf)>,
    /// Folders searched by file name for manifest-named files the mod folder lacks.
    pub artifact_dirs: Vec<PathBuf>,
}

/// Root files that travel with the package when present.
const ROOT_DOCS: &[&str] = &["readme", "license", "notice", "changelog", "copying"];

enum Source {
    Bytes(Vec<u8>),
    File(PathBuf),
}

/// Packs and verifies. Returns the report text; on any error nothing is left behind.
pub fn pack(opts: &PackOptions) -> Result<String, Vec<String>> {
    let manifest_path = opts.mod_dir.join(MANIFEST_FILE);
    let text = std::fs::read(&manifest_path)
        .map_err(|e| vec![format!("cannot read {}: {e}", manifest_path.display())])?;
    let parsed = parse_manifest(&String::from_utf8_lossy(&text)).map_err(|e| {
        e.into_iter()
            .map(|m| format!("{MANIFEST_FILE}: {m}"))
            .collect::<Vec<_>>()
    })?;
    let m = &parsed.manifest;

    let mut errors: Vec<String> = Vec::new();
    let mut entries: BTreeMap<String, Source> = BTreeMap::new();
    entries.insert(MANIFEST_FILE.to_owned(), Source::Bytes(text));

    // Root documents.
    if let Ok(rd) = std::fs::read_dir(&opts.mod_dir) {
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            let lower = name.to_lowercase();
            let is_doc = ROOT_DOCS.iter().any(|d| lower.starts_with(d));
            if is_doc && e.path().is_file() && !e.path().is_symlink() {
                entries.insert(name, Source::File(e.path()));
            }
        }
    }

    // Explicit additions win over everything.
    let mut added: Vec<&str> = Vec::new();
    for (path, file) in &opts.add {
        if let Err(why) = clean_path(path) {
            errors.push(format!("--add {path}: {why}"));
            continue;
        }
        if !file.is_file() {
            errors.push(format!("--add {path}={}: not a file", file.display()));
            continue;
        }
        added.push(path);
        entries.insert(path.clone(), Source::File(file.clone()));
    }

    // Everything the manifest names.
    for rel in m.referenced_paths() {
        let satisfied_by_add = added
            .iter()
            .any(|a| *a == rel || a.starts_with(&format!("{rel}/")));
        if satisfied_by_add {
            continue;
        }
        let on_disk = opts.mod_dir.join(rel);
        if on_disk.exists() {
            if let Err(e) = collect(&on_disk, rel, &mut entries) {
                errors.push(e);
            }
            continue;
        }
        let file_name = Path::new(rel).file_name();
        let found = file_name.and_then(|n| {
            opts.artifact_dirs
                .iter()
                .map(|d| d.join(n))
                .find(|p| p.is_file())
        });
        match found {
            Some(p) => {
                entries.insert(rel.to_owned(), Source::File(p));
            }
            None => errors.push(format!(
                "{MANIFEST_FILE} names {rel:?}, which is not in the mod folder{}: build it and pass --artifacts <build folder>, or --add {rel}=<file>",
                if opts.artifact_dirs.is_empty() {
                    String::new()
                } else {
                    " or in the --artifacts folders".to_owned()
                }
            )),
        }
    }
    if !errors.is_empty() {
        return Err(errors);
    }

    let bytes = build_zip(&entries).map_err(|e| vec![e])?;
    let name = package_file_name(&m.id, &m.version);
    let zip_path = opts.out_dir.join(&name);
    let sidecar = opts.out_dir.join(format!("{name}.sha256"));
    std::fs::create_dir_all(&opts.out_dir)
        .map_err(|e| vec![format!("cannot create {}: {e}", opts.out_dir.display())])?;
    std::fs::write(&zip_path, &bytes)
        .map_err(|e| vec![format!("cannot write {}: {e}", zip_path.display())])?;
    let written = std::fs::write(&sidecar, sha256_file_text(&sha256_hex(&bytes), &name));
    let result = written
        .map_err(|e| vec![format!("cannot write {}: {e}", sidecar.display())])
        .and_then(|()| verify_package(&zip_path, &Limits::default()));
    match result {
        Ok(report) => Ok(format!(
            "wrote {}\nwrote {}\n{}",
            zip_path.display(),
            sidecar.display(),
            crate::report_text(&report)
        )),
        Err(e) => {
            let _ = std::fs::remove_file(&zip_path);
            let _ = std::fs::remove_file(&sidecar);
            Err(e)
        }
    }
}

/// A package path: relative, `/`-separated, nothing special.
fn clean_path(p: &str) -> Result<(), String> {
    let bad = p.is_empty()
        || p.starts_with('/')
        || p.contains(['\\', ':'])
        || p.split('/').any(|s| s.is_empty() || s == "." || s == "..");
    if bad {
        Err("must be a relative path with / separators, without .. or empty parts".into())
    } else {
        Ok(())
    }
}

/// Adds a file or a folder (recursively, sorted by the map) under the package path `rel`.
fn collect(
    on_disk: &Path,
    rel: &str,
    entries: &mut BTreeMap<String, Source>,
) -> Result<(), String> {
    let meta = std::fs::symlink_metadata(on_disk)
        .map_err(|e| format!("cannot read {}: {e}", on_disk.display()))?;
    if meta.file_type().is_symlink() {
        return Err(format!(
            "{} is a symbolic link; packages may not contain links",
            on_disk.display()
        ));
    }
    if meta.is_file() {
        entries.insert(rel.to_owned(), Source::File(on_disk.to_path_buf()));
        return Ok(());
    }
    let rd = std::fs::read_dir(on_disk)
        .map_err(|e| format!("cannot read {}: {e}", on_disk.display()))?;
    for e in rd {
        let e = e.map_err(|e| format!("cannot read {}: {e}", on_disk.display()))?;
        let name = e
            .file_name()
            .into_string()
            .map_err(|n| format!("{n:?} is not UTF-8"))?;
        collect(&e.path(), &format!("{rel}/{name}"), entries)?;
    }
    Ok(())
}

/// The zip bytes of `entries` (already sorted by path), reproducibly.
fn build_zip(entries: &BTreeMap<String, Source>) -> Result<Vec<u8>, String> {
    let opts = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .compression_level(Some(6))
        .last_modified_time(DateTime::default())
        .unix_permissions(0o644);
    let mut w = ZipWriter::new(Cursor::new(Vec::new()));
    for (path, source) in entries {
        let data = match source {
            Source::Bytes(b) => std::borrow::Cow::Borrowed(b.as_slice()),
            Source::File(p) => std::borrow::Cow::Owned(
                std::fs::read(p).map_err(|e| format!("cannot read {}: {e}", p.display()))?,
            ),
        };
        w.start_file(path.as_str(), opts)
            .map_err(|e| format!("{path}: {e}"))?;
        w.write_all(&data).map_err(|e| format!("{path}: {e}"))?;
    }
    Ok(w.finish().map_err(|e| e.to_string())?.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    const MANIFEST: &str = "manifest_version = 2\nid = \"hello\"\nname = \"Hello\"\nversion = \"1.0.0\"\nside = \"both\"\n[game]\nversions = [\"0.5.5f\"]\n[client]\ndll = \"client/hello.dll\"\nhook_api = 1\n[server]\nserver_api = 1\n[server.library]\nlinux = \"server/libhello.so\"\nwindows = \"server/hello.dll\"\n";

    struct Fixture {
        _t: tempfile::TempDir,
        dir: PathBuf,
        build: PathBuf,
        out: PathBuf,
    }

    fn fixture() -> Fixture {
        let t = tempfile::tempdir().unwrap();
        let dir = t.path().join("mod");
        let build = t.path().join("build");
        let out = t.path().join("out");
        fs::create_dir_all(dir.join("src")).unwrap();
        fs::create_dir_all(&build).unwrap();
        fs::write(dir.join("mod.toml"), MANIFEST).unwrap();
        fs::write(dir.join("src/lib.rs"), "// sources never travel").unwrap();
        fs::write(dir.join("README.md"), "docs").unwrap();
        fs::write(dir.join("LICENSE"), "MIT").unwrap();
        for f in ["hello.dll", "libhello.so"] {
            fs::write(build.join(f), format!("binary {f}")).unwrap();
        }
        Fixture {
            dir,
            build,
            out,
            _t: t,
        }
    }

    fn opts(f: &Fixture) -> PackOptions {
        PackOptions {
            mod_dir: f.dir.clone(),
            out_dir: f.out.clone(),
            add: vec![],
            artifact_dirs: vec![f.build.clone()],
        }
    }

    #[test]
    fn packs_exactly_the_named_files_and_root_docs_and_verifies() {
        let f = fixture();
        let report = pack(&opts(&f)).unwrap();
        assert!(report.contains("hello 1.0.0 (side: both)"), "{report}");
        assert!(f.out.join("hello-1.0.0.zip").is_file());
        assert!(f.out.join("hello-1.0.0.zip.sha256").is_file());
        let r = verify_package(&f.out.join("hello-1.0.0.zip"), &Limits::default()).unwrap();
        let paths: Vec<&str> = r.files.iter().map(|x| x.path.as_str()).collect();
        assert_eq!(
            paths,
            [
                "LICENSE",
                "README.md",
                "client/hello.dll",
                "mod.toml",
                "server/hello.dll",
                "server/libhello.so"
            ]
        );
    }

    #[test]
    fn packing_twice_gives_identical_bytes_even_after_touching_the_inputs() {
        let f = fixture();
        pack(&opts(&f)).unwrap();
        let first = fs::read(f.out.join("hello-1.0.0.zip")).unwrap();
        let first_hash = fs::read_to_string(f.out.join("hello-1.0.0.zip.sha256")).unwrap();
        // New file times and a different output folder change nothing.
        std::thread::sleep(std::time::Duration::from_millis(1100));
        for p in [f.build.join("hello.dll"), f.dir.join("README.md")] {
            let c = fs::read(&p).unwrap();
            fs::write(&p, c).unwrap();
        }
        let mut o = opts(&f);
        o.out_dir = f.out.join("again");
        pack(&o).unwrap();
        assert_eq!(
            first,
            fs::read(f.out.join("again/hello-1.0.0.zip")).unwrap()
        );
        assert_eq!(
            first_hash,
            fs::read_to_string(f.out.join("again/hello-1.0.0.zip.sha256")).unwrap()
        );
    }

    #[test]
    fn a_missing_named_file_is_an_error_that_says_how_to_fix_it() {
        let f = fixture();
        fs::remove_file(f.build.join("libhello.so")).unwrap();
        let mut o = opts(&f);
        let e = pack(&o).unwrap_err().join("\n");
        assert!(e.contains("\"server/libhello.so\""), "{e}");
        assert!(e.contains("--artifacts"), "{e}");
        assert!(
            !f.out.join("hello-1.0.0.zip").exists(),
            "nothing is written"
        );
        o.artifact_dirs.clear();
        assert!(
            pack(&o)
                .unwrap_err()
                .join("\n")
                .contains("--add server/libhello.so")
        );
    }

    #[test]
    fn add_overrides_and_satisfies_a_named_path() {
        let f = fixture();
        let custom = f.build.join("custom.bin");
        fs::write(&custom, "custom").unwrap();
        let mut o = opts(&f);
        o.add.push(("client/hello.dll".into(), custom));
        pack(&o).unwrap();
        let r = verify_package(&f.out.join("hello-1.0.0.zip"), &Limits::default()).unwrap();
        let dll = r
            .files
            .iter()
            .find(|x| x.path == "client/hello.dll")
            .unwrap();
        assert_eq!(dll.size, "custom".len() as u64);
        // Bad --add paths and files are refused.
        for bad in ["../x", "/x", "a//b", "a\\b", ""] {
            let mut o = opts(&f);
            o.add.push((bad.into(), f.build.join("hello.dll")));
            assert!(pack(&o).is_err(), "{bad:?}");
        }
        let mut o = opts(&f);
        o.add.push(("x".into(), f.build.join("nope")));
        assert!(pack(&o).unwrap_err()[0].contains("not a file"));
    }

    #[test]
    fn data_folders_are_packed_recursively_and_in_order() {
        let f = fixture();
        let m = "manifest_version = 2\nid = \"hello\"\nname = \"Hello\"\nversion = \"1.0.0\"\nside = \"server\"\n[server]\ntables = [\"data/tables\"]\n";
        fs::write(f.dir.join("mod.toml"), m).unwrap();
        fs::create_dir_all(f.dir.join("data/tables/sub")).unwrap();
        fs::write(f.dir.join("data/tables/b.txt"), "b").unwrap();
        fs::write(f.dir.join("data/tables/a.txt"), "a").unwrap();
        fs::write(f.dir.join("data/tables/sub/c.txt"), "c").unwrap();
        fs::write(f.dir.join("data/unlisted.txt"), "not named by the manifest").unwrap();
        pack(&opts(&f)).unwrap();
        let r = verify_package(&f.out.join("hello-1.0.0.zip"), &Limits::default()).unwrap();
        let paths: Vec<&str> = r.files.iter().map(|x| x.path.as_str()).collect();
        assert_eq!(
            paths,
            [
                "LICENSE",
                "README.md",
                "data/tables/a.txt",
                "data/tables/b.txt",
                "data/tables/sub/c.txt",
                "mod.toml"
            ]
        );
    }

    #[test]
    fn an_invalid_manifest_stops_before_anything_is_written() {
        let f = fixture();
        fs::write(
            f.dir.join("mod.toml"),
            "manifest_version = 2\nid = \"hello\"\n",
        )
        .unwrap();
        let e = pack(&opts(&f)).unwrap_err();
        assert!(e.iter().any(|m| m.contains("mod.toml: ")), "{e:?}");
        assert!(!f.out.exists());
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_in_the_mod_folder_are_refused() {
        let f = fixture();
        let m = "manifest_version = 2\nid = \"hello\"\nname = \"Hello\"\nversion = \"1.0.0\"\nside = \"server\"\n[server]\ntables = [\"data\"]\n";
        fs::write(f.dir.join("mod.toml"), m).unwrap();
        fs::create_dir_all(f.dir.join("data")).unwrap();
        std::os::unix::fs::symlink("/etc/passwd", f.dir.join("data/link")).unwrap();
        let e = pack(&opts(&f)).unwrap_err().join("\n");
        assert!(e.contains("symbolic link"), "{e}");
    }
}
