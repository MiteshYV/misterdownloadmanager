//! ClamAV (https://www.clamav.net, GPL), run as its own program when it is installed:
//! `clamscan` with its own virus definitions (kept up to date by its `freshclam`). MDM never
//! downloads or installs it; Options says where to get it.

use std::path::{Path, PathBuf};

/// What ClamAV said.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Clam {
    /// No `clamscan` on this computer.
    NotInstalled,
    /// It looked at every file and found nothing.
    Clean,
    /// What it found: each file and its signature's name.
    Found(Vec<(PathBuf, String)>),
    /// It couldn't scan (no virus definitions yet, an error): what it said.
    Failed(String),
}

/// `clamscan`, the first there of [`candidates`].
pub fn find_clamscan() -> Option<PathBuf> {
    candidates(std::env::var_os("PATH"))
        .into_iter()
        .find(|p| p.is_file())
}

/// Where `clamscan` may be: the PATH, then the package managers' folders, which a Mac app
/// started from Finder doesn't have on its PATH (Homebrew's on Apple silicon and on Intel,
/// MacPorts'), and the usual Linux ones.
fn candidates(path: Option<std::ffi::OsString>) -> Vec<PathBuf> {
    let name = if cfg!(windows) {
        "clamscan.exe"
    } else {
        "clamscan"
    };
    let mut out: Vec<PathBuf> = path
        .iter()
        .flat_map(std::env::split_paths)
        .map(|d| d.join(name))
        .collect();
    let more: &[&str] = if cfg!(windows) {
        &[
            "C:\\Program Files\\ClamAV",
            "C:\\Program Files (x86)\\ClamAV",
        ]
    } else {
        &[
            "/opt/homebrew/bin",
            "/usr/local/bin",
            "/opt/local/bin",
            "/usr/bin",
        ]
    };
    for d in more {
        let p = Path::new(d).join(name);
        if !out.contains(&p) {
            out.push(p);
        }
    }
    out
}

/// Scan `files` with `clamscan`. Its own limits stand (files over 100 MB are passed over, as
/// a video's size makes it no likelier to hold a virus than its first 100 MB).
pub fn scan(clamscan: &Path, files: &[PathBuf]) -> Clam {
    if files.is_empty() {
        return Clam::Clean;
    }
    let out = std::process::Command::new(clamscan)
        .args(["--no-summary", "--infected", "--stdout", "--"])
        .args(files)
        .stdin(std::process::Stdio::null())
        .output();
    match out {
        Ok(o) => read(
            o.status.code(),
            &String::from_utf8_lossy(&o.stdout),
            &String::from_utf8_lossy(&o.stderr),
        ),
        Err(e) => Clam::Failed(format!("clamscan didn't run: {e}")),
    }
}

/// `clamscan`'s answer: exit 0 clean, 1 found (`<path>: <signature> FOUND` lines), 2 an
/// error.
pub(crate) fn read(code: Option<i32>, stdout: &str, stderr: &str) -> Clam {
    match code {
        Some(0) => Clam::Clean,
        Some(1) => Clam::Found(
            stdout
                .lines()
                .filter_map(|l| l.strip_suffix(" FOUND"))
                .filter_map(|l| l.rsplit_once(": "))
                .map(|(path, sig)| (PathBuf::from(path), sig.trim().to_owned()))
                .collect(),
        ),
        _ => {
            let said = stderr
                .lines()
                .chain(stdout.lines())
                .map(str::trim)
                .find(|l| !l.is_empty())
                .unwrap_or("clamscan failed")
                .to_owned();
            Clam::Failed(said)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Clam, candidates, read};
    use std::path::PathBuf;

    /// Found, clean and failed, as clamscan words them.
    #[test]
    fn clamscans_answers_are_read() {
        assert_eq!(read(Some(0), "", ""), Clam::Clean);
        let found = read(
            Some(1),
            "/q/abc/Codec Pack.exe: Win.Test.EICAR_HDB-1 FOUND\n/q/abc/b.zip: Doc.Dropper.Agent-1 FOUND\n",
            "",
        );
        assert_eq!(
            found,
            Clam::Found(vec![
                (
                    PathBuf::from("/q/abc/Codec Pack.exe"),
                    "Win.Test.EICAR_HDB-1".into()
                ),
                (PathBuf::from("/q/abc/b.zip"), "Doc.Dropper.Agent-1".into()),
            ])
        );
        assert_eq!(
            read(
                Some(2),
                "",
                "LibClamAV Error: cli_loaddbdir(): No supported database files found\n"
            ),
            Clam::Failed(
                "LibClamAV Error: cli_loaddbdir(): No supported database files found".into()
            )
        );
    }

    /// A Mac app started from Finder has no Homebrew on its PATH: Homebrew's folders are looked
    /// in all the same, after the PATH's.
    #[cfg(not(windows))]
    #[test]
    fn homebrews_folders_are_looked_in() {
        let c = candidates(Some("/usr/bin:/bin".into()));
        assert_eq!(c[0], PathBuf::from("/usr/bin/clamscan"));
        assert!(c.contains(&PathBuf::from("/opt/homebrew/bin/clamscan")));
        assert!(c.contains(&PathBuf::from("/usr/local/bin/clamscan")));
        assert_eq!(
            c.iter()
                .filter(|p| p.as_path() == std::path::Path::new("/usr/bin/clamscan"))
                .count(),
            1
        );
    }
}
