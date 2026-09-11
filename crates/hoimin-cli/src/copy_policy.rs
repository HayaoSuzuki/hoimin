//! Built-in exclusions shared by target discovery and worker copying.

use std::ffi::OsStr;
use std::path::Path;

use ignore::DirEntry;

pub(crate) fn default_excluded(entry: &DirEntry) -> bool {
    entry.depth() != 0 && excluded_name(entry.file_name())
}

pub(crate) fn excluded_path(relative: &Path) -> bool {
    relative
        .components()
        .any(|component| excluded_name(component.as_os_str()))
}

fn excluded_name(name: &OsStr) -> bool {
    let Some(name) = name.to_str() else {
        return false;
    };
    [
        ".git",
        ".venv",
        "venv",
        "env",
        "__pycache__",
        ".pytest_cache",
        ".mypy_cache",
        ".ruff_cache",
        ".pyre",
        ".pytype",
        ".tox",
        ".nox",
    ]
    .iter()
    .any(|excluded| {
        if cfg!(windows) {
            name.eq_ignore_ascii_case(excluded)
        } else {
            name == *excluded
        }
    })
}
