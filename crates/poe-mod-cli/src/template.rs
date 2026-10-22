//! `poe-mod new`: project templates (`templates/<kind>/…`, `{{placeholder}}` text substitution).

use std::{
    fmt,
    path::{Path, PathBuf},
    str::FromStr,
};

use clap::ValueEnum;
use poe_hook_mods::valid_id;

/// The game version new mods are made for (the server and client this SDK release targets).
pub const GAME_VERSION: &str = "0.5.5e";
const SDK_GIT: &str = "https://github.com/WaystoneWorks/Poe2ModSDK";

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Kind {
    Client,
    Server,
    Both,
}

impl FromStr for Kind {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, String> {
        match s {
            "client" => Ok(Self::Client),
            "server" => Ok(Self::Server),
            "both" => Ok(Self::Both),
            _ => Err(format!("{s:?} is not client, server or both")),
        }
    }
}

impl fmt::Display for Kind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Client => "client",
            Self::Server => "server",
            Self::Both => "both",
        })
    }
}

/// Where a new project gets `poe-mod-sdk` from.
#[derive(Debug, Clone)]
pub enum SdkSource {
    /// The SDK's git tag for this tool's version (`v<version>`).
    Git { tag: String },
    /// A checkout of the SDK repository (a path to its root; relative paths are written as given).
    Path(PathBuf),
}

impl SdkSource {
    /// The tag that matches this build of the tool.
    pub fn release() -> Self {
        Self::Git {
            tag: format!("v{}", env!("CARGO_PKG_VERSION")),
        }
    }

    fn dependency(&self, features: &str) -> String {
        match self {
            Self::Git { tag } => format!("{{ git = \"{SDK_GIT}\", tag = \"{tag}\"{features} }}"),
            Self::Path(p) => {
                let root = p.to_string_lossy().replace('\\', "/");
                let root = root.trim_end_matches('/');
                format!("{{ path = \"{root}/crates/poe-mod-sdk\"{features} }}")
            }
        }
    }
}

pub struct NewOptions {
    pub kind: Kind,
    pub id: String,
    pub dir: PathBuf,
    pub author: String,
    pub sdk: SdkSource,
    /// Write into a directory that already has files.
    pub force: bool,
}

/// (path in the new project, template text). `gitignore` is written as `.gitignore`.
fn files(kind: Kind) -> &'static [(&'static str, &'static str)] {
    match kind {
        Kind::Client => &[
            (
                "Cargo.toml",
                include_str!("../templates/client/Cargo.toml.tmpl"),
            ),
            (
                "mod.toml",
                include_str!("../templates/client/mod.toml.tmpl"),
            ),
            (
                "README.md",
                include_str!("../templates/client/README.md.tmpl"),
            ),
            (
                ".gitignore",
                include_str!("../templates/client/gitignore.tmpl"),
            ),
            (
                "src/lib.rs",
                include_str!("../templates/client/src/lib.rs.tmpl"),
            ),
        ],
        Kind::Server => &[
            (
                "Cargo.toml",
                include_str!("../templates/server/Cargo.toml.tmpl"),
            ),
            (
                "mod.toml",
                include_str!("../templates/server/mod.toml.tmpl"),
            ),
            (
                "README.md",
                include_str!("../templates/server/README.md.tmpl"),
            ),
            (
                ".gitignore",
                include_str!("../templates/server/gitignore.tmpl"),
            ),
            (
                "src/lib.rs",
                include_str!("../templates/server/src/lib.rs.tmpl"),
            ),
        ],
        Kind::Both => &[
            ("mod.toml", include_str!("../templates/both/mod.toml.tmpl")),
            (
                "README.md",
                include_str!("../templates/both/README.md.tmpl"),
            ),
            (
                ".gitignore",
                include_str!("../templates/both/gitignore.tmpl"),
            ),
            (
                "client/Cargo.toml",
                include_str!("../templates/both/client/Cargo.toml.tmpl"),
            ),
            (
                "client/src/lib.rs",
                include_str!("../templates/both/client/src/lib.rs.tmpl"),
            ),
            (
                "server/Cargo.toml",
                include_str!("../templates/both/server/Cargo.toml.tmpl"),
            ),
            (
                "server/src/lib.rs",
                include_str!("../templates/both/server/src/lib.rs.tmpl"),
            ),
        ],
    }
}

fn title(id: &str) -> String {
    let spaced = id.replace(['-', '_'], " ");
    let mut c = spaced.chars();
    c.next()
        .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
        .unwrap_or_default()
}

/// Renders every file of the template: `(path, text)`.
pub fn render(kind: Kind, id: &str, author: &str, sdk: &SdkSource) -> Vec<(String, String)> {
    let client = sdk.dependency(", default-features = false, features = [\"client\"]");
    let server = sdk.dependency("");
    let vars: [(&str, String); 10] = [
        ("{{id}}", id.to_owned()),
        ("{{lib}}", id.replace('-', "_")),
        ("{{name}}", title(id)),
        ("{{author}}", author.to_owned()),
        ("{{game_version}}", GAME_VERSION.to_owned()),
        ("{{hook_api}}", poe_hook_mods::HOOK_API.to_string()),
        (
            "{{server_api}}",
            poe_mod_api::SERVER_API_VERSION.to_string(),
        ),
        ("{{sdk_client}}", client),
        ("{{sdk_server}}", server),
        ("{{kind}}", kind.to_string()),
    ];
    files(kind)
        .iter()
        .map(|(path, text)| {
            let mut out = (*text).to_owned();
            for (k, v) in &vars {
                out = out.replace(k, v);
            }
            ((*path).to_owned(), out)
        })
        .collect()
}

/// Creates the project. Returns a message with the next steps.
pub fn new_project(o: &NewOptions) -> Result<String, Vec<String>> {
    if !valid_id(&o.id) {
        return Err(vec![format!(
            "id {:?} must match [a-z0-9][a-z0-9_-]{{1,63}}",
            o.id
        )]);
    }
    if !o.id.starts_with(|c: char| c.is_ascii_lowercase()) {
        return Err(vec![format!(
            "id {:?} must start with a letter (it is also the Cargo package name)",
            o.id
        )]);
    }
    if o.author.contains(['"', '\\', '\n']) {
        return Err(vec![
            "author may not contain quotes, backslashes or newlines".into(),
        ]);
    }
    if o.dir.exists() && !o.force && std::fs::read_dir(&o.dir).is_ok_and(|mut d| d.next().is_some())
    {
        return Err(vec![format!(
            "{} is not empty (use --force to write into it)",
            o.dir.display()
        )]);
    }
    let rendered = render(o.kind, &o.id, &o.author, &o.sdk);
    for (path, text) in &rendered {
        let to = o.dir.join(path);
        write_file(&to, text).map_err(|e| vec![format!("{}: {e}", to.display())])?;
    }
    Ok(format!(
        "created {} project {} in {}\nnext: cd {} && see README.md (build, then `poe-mod pack`)",
        o.kind,
        o.id,
        o.dir.display(),
        o.dir.display()
    ))
}

fn write_file(to: &Path, text: &str) -> std::io::Result<()> {
    if let Some(p) = to.parent() {
        std::fs::create_dir_all(p)?;
    }
    std::fs::write(to, text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use poe_hook_mods::{Side, parse_manifest};

    fn manifest_of(kind: Kind) -> poe_hook_mods::Manifest {
        let files = render(kind, "my-mod", "Me", &SdkSource::release());
        let (_, text) = files.iter().find(|(p, _)| p == "mod.toml").unwrap();
        let parsed = parse_manifest(text).unwrap_or_else(|e| panic!("{kind}: {e:?}"));
        assert!(parsed.unknown_keys.is_empty(), "{:?}", parsed.unknown_keys);
        parsed.manifest
    }

    #[test]
    fn every_template_manifest_is_valid_for_the_shared_parser() {
        let c = manifest_of(Kind::Client);
        assert_eq!((c.side, c.manifest_version), (Side::Client, 1));
        assert_eq!(c.hook_api, Some(poe_hook_mods::HOOK_API));
        assert_eq!(c.entry_dll.as_deref(), Some("client/my_mod.dll"));
        let s = manifest_of(Kind::Server);
        assert_eq!((s.side, s.manifest_version), (Side::Server, 2));
        assert_eq!(s.server_api, Some(poe_mod_api::SERVER_API_VERSION));
        assert_eq!(s.server_linux.as_deref(), Some("server/libmy_mod.so"));
        let b = manifest_of(Kind::Both);
        assert_eq!(b.side, Side::Both);
        assert_eq!(b.entry_dll.as_deref(), Some("client/my_mod_client.dll"));
        assert_eq!(
            b.server_windows.as_deref(),
            Some("server/my_mod_server.dll")
        );
        assert_eq!(b.game_versions, [GAME_VERSION]);
    }

    #[test]
    fn no_placeholder_is_left_behind() {
        for kind in [Kind::Client, Kind::Server, Kind::Both] {
            for sdk in [SdkSource::release(), SdkSource::Path("../..".into())] {
                for (path, text) in render(kind, "my-mod", "Me", &sdk) {
                    assert!(!text.contains("{{"), "{kind} {path}: {text}");
                }
            }
        }
    }

    #[test]
    fn dependencies_render_for_git_and_path() {
        let g = SdkSource::Git {
            tag: "v1.2.3".into(),
        };
        assert_eq!(
            g.dependency(""),
            "{ git = \"https://github.com/WaystoneWorks/Poe2ModSDK\", tag = \"v1.2.3\" }"
        );
        let p = SdkSource::Path("..\\sdk\\".into());
        assert_eq!(
            p.dependency(", default-features = false"),
            "{ path = \"../sdk/crates/poe-mod-sdk\", default-features = false }"
        );
    }

    #[test]
    fn new_project_writes_files_and_refuses_bad_ids_and_busy_dirs() {
        let t = tempfile::tempdir().unwrap();
        let mut o = NewOptions {
            kind: Kind::Both,
            id: "my-mod".into(),
            dir: t.path().join("my-mod"),
            author: "Me".into(),
            sdk: SdkSource::release(),
            force: false,
        };
        new_project(&o).unwrap();
        assert!(o.dir.join("mod.toml").is_file());
        assert!(o.dir.join("client/src/lib.rs").is_file());
        assert!(o.dir.join(".gitignore").is_file());
        // Not empty any more.
        assert!(new_project(&o).unwrap_err()[0].contains("not empty"));
        o.force = true;
        assert!(new_project(&o).is_ok());
        for bad in ["A", "x", "9lives", "a b", "a/b"] {
            o.id = bad.into();
            assert!(new_project(&o).is_err(), "{bad}");
        }
        o.id = "ok-id".into();
        o.author = "Evil \" quote".into();
        assert!(new_project(&o).is_err());
    }

    #[test]
    fn titles_are_readable() {
        assert_eq!(title("hello-server"), "Hello server");
        assert_eq!(title("a_b"), "A b");
    }
}
