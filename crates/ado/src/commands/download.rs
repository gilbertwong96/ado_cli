//! The streamed file write the two download commands share (`D28`): the body goes
//! straight to a sibling temp file in 64 KiB chunks — no size cap — and only a
//! fully received body is renamed onto the target. That keeps both invariants: no
//! partial file ever appears at the target, and a pre-existing target survives a
//! failed download byte for byte. The temp name is unique per process, so a
//! pre-existing `{target}.tmp` is never opened — a failed download cannot truncate
//! or delete it — and a failure while removing the temp never masks the original
//! error.
//!
//! `pipelines-artifacts download` and `workitems attachments download` both go
//! through this path; the noun only selects the error wording ("artifact",
//! "attachment"), which is §8 surface.

use std::fs;
use std::io::{self, Write};
use std::sync::atomic::{AtomicUsize, Ordering};

use ado_core::client::RawBody;
use ado_core::error::AdoError;

/// Stream `body` into `target` and report how many bytes were written. `what`
/// names the thing being written, for the module's `File.write!/2` failure
/// presentation (D4).
pub fn write_streamed(target: &str, what: &str, mut body: RawBody) -> Result<u64, AdoError> {
    let temp = temp_path(target);
    let mut file = fs::File::create(&temp).map_err(|error| write_error(target, what, &error))?;

    let written = match copy_body(&mut body, &mut file, target, what) {
        Ok(written) => written,
        Err(error) => {
            drop(file);
            let _ = fs::remove_file(&temp);

            return Err(error);
        }
    };

    drop(file);
    fs::rename(&temp, target).map_err(|error| {
        let _ = fs::remove_file(&temp);
        write_error(target, what, &error)
    })?;

    Ok(written)
}

/// The module's `File.write!/2` raises; Rust answers with the usual command-level
/// error presentation, the way `completion --write-to-file` does (D4).
fn write_error(path: &str, what: &str, error: &io::Error) -> AdoError {
    AdoError::validation(format!("Could not write the {what} to {path}: {error}"))
}

/// The temp name sits beside the target, so the successful rename is a
/// same-directory replace of the target's bytes. The pid and per-process counter
/// infix keeps it unique, so a pre-existing `{target}.tmp` is never opened — a
/// failed download cannot truncate or delete it. The `.tmp` suffix stays, so the
/// temp still reads as a temporary download in the target's directory.
fn temp_path(target: &str) -> String {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);

    let unique = COUNTER.fetch_add(1, Ordering::Relaxed);

    format!("{target}.{}.{unique}.tmp", std::process::id())
}

/// The copy loop, so a reader failure and a writer failure classify differently:
/// a dropped connection is the §6.2 transport class, while an unwritable target is
/// command-level input. A fixed 64 KiB buffer keeps the copy at one allocation and
/// the body itself has no size cap.
fn copy_body(
    body: &mut RawBody,
    file: &mut fs::File,
    target: &str,
    what: &str,
) -> Result<u64, AdoError> {
    let mut buffer = vec![0u8; 64 * 1024];
    let mut written = 0u64;

    loop {
        let read = body.read_chunk(&mut buffer)?;

        if read == 0 {
            return Ok(written);
        }

        file.write_all(&buffer[..read])
            .map_err(|error| write_error(target, what, &error))?;
        written += read as u64;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn temp_path_is_a_unique_tmp_sibling() {
        use std::path::Path;

        let first = temp_path("out.zip");
        let second = temp_path("out.zip");

        assert!(
            first.ends_with(".tmp"),
            "the temp reads as a temporary download: {first}"
        );
        assert!(
            first.starts_with("out.zip."),
            "the temp is derived from the target: {first}"
        );
        assert_ne!(
            first, "out.zip.tmp",
            "a pre-existing `{{target}}.tmp` is never reused: {first}"
        );
        assert_ne!(first, second, "each download gets its own temp name");
        assert_eq!(
            Path::new(&temp_path("/tmp/dir/out.zip")).parent(),
            Some(Path::new("/tmp/dir")),
            "the temp stays in the target's directory, so the rename is same-filesystem"
        );
    }
}
