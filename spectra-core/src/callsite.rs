//! The reserved `_callsite` value: which source line emitted a row or metric point.
//!
//! Generated `*Logger` and `*Recorder` helpers and the direct `try_log_event*` functions are
//! `#[track_caller]`, so [`Location::caller`] names the application line that called them.
//! [`callsite_value`] turns that location into the `{file, line, module}` object stored under
//! [`crate::CALLSITE_FIELD`]: as an event field, or as a metric label holding the object's
//! JSON text.

use std::panic::Location;

use serde_json::{Map, Value};

/// Builds the `_callsite` object for `location`: `file`, `line` and, when the file path
/// allows it, `module`.
///
/// `#[track_caller]` records no module path, so `module` is derived from the file path
/// (see [`module_path_from_file`]). A module declared inline (`mod foo { … }`) reports the
/// module of the file that holds it.
///
/// # Examples
///
/// ```
/// let value = spectra_core::callsite_value(std::panic::Location::caller());
/// assert!(value["file"].as_str().is_some());
/// assert!(value["line"].as_u64().is_some());
/// ```
#[must_use]
pub fn callsite_value(location: &Location<'_>) -> Value {
    let mut map = Map::new();
    map.insert("file".to_owned(), Value::from(location.file()));
    map.insert("line".to_owned(), Value::from(location.line()));
    if let Some(module) = module_path_from_file(location.file()) {
        map.insert("module".to_owned(), Value::from(module));
    }
    Value::Object(map)
}

/// Derives a Rust module path from a source file path, such as
/// `fernwood-store/src/checkout/abandon.rs` → `fernwood_store::checkout::abandon`.
///
/// The crate is the directory above `src/` (a registry `-x.y.z` version suffix dropped), the
/// binary name under `src/bin/`, or the target name under `tests/`, `examples/` and `benches/`.
/// `lib.rs`, `main.rs` and `mod.rs` name their parent module. Returns `None` when the path
/// has no such root or is not a `.rs` file.
///
/// # Examples
///
/// ```
/// use spectra_core::module_path_from_file;
///
/// assert_eq!(
///     module_path_from_file("fernwood-store/src/checkout/abandon.rs").as_deref(),
///     Some("fernwood_store::checkout::abandon"),
/// );
/// assert_eq!(module_path_from_file("build.rs"), None);
/// ```
#[must_use]
pub fn module_path_from_file(file: &str) -> Option<String> {
    let normalized = file.replace('\\', "/");
    let parts: Vec<&str> = normalized
        .split('/')
        .filter(|p| !p.is_empty() && *p != ".")
        .collect();
    let root = parts
        .iter()
        .rposition(|p| matches!(*p, "src" | "tests" | "examples" | "benches"))?;
    let rest = &parts[root + 1..];
    if std::path::Path::new(rest.last()?)
        .extension()
        .is_none_or(|ext| ext != "rs")
    {
        return None;
    }
    let (crate_name, modules) = match (parts[root], rest) {
        ("src", ["bin", name, tail @ ..]) => (target_name(name), tail),
        ("src", _) => (strip_version(parts.get(root.checked_sub(1)?)?), rest),
        (_, [name, tail @ ..]) => (target_name(name), tail),
        _ => return None,
    };
    let mut path = vec![crate_name.replace('-', "_")];
    for (i, segment) in modules.iter().enumerate() {
        let segment = if i + 1 == modules.len() {
            let stem = segment.strip_suffix(".rs")?;
            if matches!(stem, "lib" | "main" | "mod") {
                continue;
            }
            stem
        } else {
            segment
        };
        path.push(segment.replace('-', "_"));
    }
    if path.iter().any(String::is_empty) {
        return None;
    }
    Some(path.join("::"))
}

/// A `src/bin`, `tests`, `examples` or `benches` entry: `name.rs` or a `name/` directory.
fn target_name(entry: &str) -> &str {
    entry.strip_suffix(".rs").unwrap_or(entry)
}

/// Drops a registry `-major.minor.patch…` suffix: `serde-json-1.0.2` → `serde-json`.
fn strip_version(dir: &str) -> &str {
    for (i, _) in dir.match_indices('-') {
        let mut fields = dir[i + 1..].splitn(3, '.');
        let numeric = |s: Option<&str>| {
            s.is_some_and(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
        };
        let (major, minor, patch) = (fields.next(), fields.next(), fields.next());
        if numeric(major)
            && numeric(minor)
            && patch.is_some_and(|p| p.as_bytes().first().is_some_and(u8::is_ascii_digit))
        {
            return &dir[..i];
        }
    }
    dir
}

#[cfg(test)]
mod tests {
    use super::*;

    fn module(file: &str) -> Option<String> {
        module_path_from_file(file)
    }

    #[test]
    fn workspace_lib_paths() {
        assert_eq!(
            module("fernwood-store/src/checkout/abandon.rs").as_deref(),
            Some("fernwood_store::checkout::abandon")
        );
        assert_eq!(
            module("spectra-core/src/lib.rs").as_deref(),
            Some("spectra_core")
        );
        assert_eq!(
            module("spectra-core/src/sinks/mod.rs").as_deref(),
            Some("spectra_core::sinks")
        );
        assert_eq!(module("app/src/main.rs").as_deref(), Some("app"));
    }

    #[test]
    fn registry_and_git_paths() {
        assert_eq!(
            module("/home/u/.cargo/registry/src/index.crates.io-6f17/photon-core-1.2.3/src/log.rs")
                .as_deref(),
            Some("photon_core::log")
        );
        assert_eq!(
            module("/r/x25519-dalek-2.0.0-rc.3/src/lib.rs").as_deref(),
            Some("x25519_dalek")
        );
        assert_eq!(
            module("/home/u/.cargo/git/checkouts/ledger-ab12/cd34/ledger/src/db/read.rs")
                .as_deref(),
            Some("ledger::db::read")
        );
    }

    #[test]
    fn bin_test_and_example_targets() {
        assert_eq!(module("app/src/bin/seed.rs").as_deref(), Some("seed"));
        assert_eq!(
            module("app/src/bin/seed/rows.rs").as_deref(),
            Some("seed::rows")
        );
        assert_eq!(
            module("spectra/tests/callsite.rs").as_deref(),
            Some("callsite")
        );
        assert_eq!(
            module("spectra/tests/suite/main.rs").as_deref(),
            Some("suite")
        );
        assert_eq!(
            module("spectra/examples/demo/util.rs").as_deref(),
            Some("demo::util")
        );
        assert_eq!(
            module(r"C:\work\shop\src\cart.rs").as_deref(),
            Some("shop::cart")
        );
    }

    #[test]
    fn unusable_paths() {
        assert_eq!(module("build.rs"), None);
        assert_eq!(module("src/lib.rs"), None);
        assert_eq!(module("crate/src/templates/page.html"), None);
        assert_eq!(module("crate/tests"), None);
    }

    #[test]
    fn value_has_file_line_and_module() {
        let location = Location::caller();
        let value = callsite_value(location);
        assert_eq!(value["file"], location.file());
        assert_eq!(value["line"], location.line());
        assert_eq!(value["module"], "spectra_core::callsite");
    }
}
