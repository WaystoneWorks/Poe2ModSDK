//! The examples are what `poe-mod new --sdk-path` writes: if a template changes, the example
//! must change with it (regenerate with the commands in `examples/README.md`). This keeps every
//! template compiled and tested by the workspace build.

use std::path::Path;

use poe_mod_cli::{Kind, SdkSource, render};

const AUTHOR: &str = "WaystoneWorks and contributors";

fn check(kind: Kind, id: &str, sdk_path: &str) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples")
        .join(id);
    for (path, want) in render(kind, id, AUTHOR, &SdkSource::Path(sdk_path.into())) {
        let file = root.join(&path);
        let have =
            std::fs::read_to_string(&file).unwrap_or_else(|e| panic!("{}: {e}", file.display()));
        assert_eq!(
            have,
            want,
            "{} is out of date with its template: regenerate the example",
            file.display()
        );
    }
}

#[test]
fn examples_match_their_templates() {
    check(Kind::Client, "hello-client", "../..");
    check(Kind::Server, "hello-server", "../..");
    check(Kind::Both, "hello-both", "../../..");
}
