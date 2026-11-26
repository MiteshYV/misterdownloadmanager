//! Quarantine checks: a finished download's files looked at before they leave quarantine and
//! go to the person's folder (the app's `quarantine.rs`). Nothing is opened or run: each file's
//! first bytes are read, a ZIP's table of contents, and outside tools are asked (ClamAV, and on
//! macOS Apple's signature checks).
//!
//! What needs the person's attention ([`Concern`]):
//! - a file that isn't what its name says it is, when that hides a program, a script or an
//!   archive behind a media or document name ([`Concern::Disguised`]), or a name built to look
//!   like one (`movie.mp4.exe`, [`Concern::DoubleExtension`]);
//! - a program, a script, an installer or a Mac app anywhere in it ([`Concern::Runs`]): a
//!   torrent of a film has no reason to hold one, and one that should is the person's to let
//!   through;
//! - a ZIP locked with a password (a way past virus scanners), or one holding a program
//!   ([`Concern::LockedArchive`], [`Concern::RunsInside`]); RAR and 7z through 7-Zip when it is
//!   installed;
//! - on macOS, an app or installer that isn't signed by an identified developer
//!   ([`Concern::Unsigned`]);
//! - what ClamAV finds ([`Concern::Malware`]), when it is installed.
//!
//! A clean report ([`Report::clean`]) has no concern in any file and no ClamAV finding.

mod clamav;
pub mod freshclam;
mod kinds;
mod macsig;

use std::path::{Path, PathBuf};

pub use clamav::{Clam, find_clamscan};
pub use freshclam::Definitions;
pub use kinds::Kind;

/// Something about a file the person should look at before it leaves quarantine.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Concern {
    /// Named as one kind (`claims`, its extension) but is another that runs or hides files
    /// (`is`).
    Disguised { claims: Kind, is: Kind },
    /// A name that ends in a program's extension after a harmless one (`clip.mp4.exe`).
    DoubleExtension,
    /// A program, script, installer or app.
    Runs(Kind),
    /// A ZIP, RAR or 7z locked with a password: what is inside can't be checked.
    LockedArchive,
    /// A program, script or installer inside an archive (the first one's name).
    RunsInside(String),
    /// macOS: an app or installer not signed by an identified developer (what the check said).
    Unsigned(String),
    /// ClamAV's finding (its signature's name).
    Malware(String),
}

impl Concern {
    /// A short English line for the log and the tests; the app words its own.
    pub fn describe(&self) -> String {
        match self {
            Concern::Disguised { claims, is } => {
                format!("named as {} but is {}", claims.noun(), is.noun())
            }
            Concern::DoubleExtension => "a program named to look like another file".into(),
            Concern::Runs(k) => k.noun().into(),
            Concern::LockedArchive => "an archive locked with a password".into(),
            Concern::RunsInside(name) => format!("holds a program ({name})"),
            Concern::Unsigned(why) => format!("not signed by an identified developer ({why})"),
            Concern::Malware(sig) => format!("ClamAV found {sig}"),
        }
    }
}

/// One file's result.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileReport {
    /// Its path under the scanned folder.
    pub path: PathBuf,
    pub size: u64,
    /// What it is, by its bytes (by its name where its bytes don't say).
    pub kind: Kind,
    pub concerns: Vec<Concern>,
}

/// A whole scan's result.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Report {
    pub files: Vec<FileReport>,
    /// What ClamAV said of the folder (its findings are in the files' concerns too).
    pub clamav: Clam,
    /// Whether 7-Zip looked inside the RAR and 7z archives (false: it isn't installed, and
    /// they were only named).
    pub opened_rar_7z: bool,
    /// ClamAV's virus definitions when it scanned (None: it isn't used, or has none).
    pub definitions: Option<Definitions>,
}

impl Report {
    /// No file has a concern.
    pub fn clean(&self) -> bool {
        self.files.iter().all(|f| f.concerns.is_empty())
    }

    /// The files with no concern.
    pub fn clean_files(&self) -> impl Iterator<Item = &FileReport> {
        self.files.iter().filter(|f| f.concerns.is_empty())
    }

    /// The files with one.
    pub fn flagged(&self) -> impl Iterator<Item = &FileReport> {
        self.files.iter().filter(|f| !f.concerns.is_empty())
    }
}

/// How to scan.
#[derive(Clone, Debug, Default)]
pub struct Options {
    /// ClamAV's `clamscan` (None: not used).
    pub clamscan: Option<PathBuf>,
    /// 7-Zip, to look inside RAR and 7z archives (None: not opened).
    pub seven_zip: Option<PathBuf>,
}

impl Options {
    /// The tools found on this computer ([`find_clamscan`], `mdm_extract::find_7zip`).
    pub fn found() -> Self {
        Options {
            clamscan: find_clamscan(),
            seven_zip: mdm_extract::find_7zip(),
        }
    }
}

/// Scan `files` (paths under `root`; a folder among them is taken file by file, an app bundle
/// as one) and report each.
pub fn scan(root: &Path, files: &[PathBuf], opts: &Options) -> Report {
    let mut out = Vec::new();
    for rel in files {
        collect(root, rel, &mut out);
    }
    let mut reports: Vec<FileReport> = out
        .into_iter()
        .map(|(rel, path)| check_file(&rel, &path, opts))
        .collect();
    let clamav = match &opts.clamscan {
        Some(clam) => {
            let paths: Vec<PathBuf> = reports.iter().map(|f| root.join(&f.path)).collect();
            let found = clamav::scan(clam, &paths);
            if let Clam::Found(hits) = &found {
                for (path, sig) in hits {
                    if let Some(f) = reports.iter_mut().find(|f| root.join(&f.path) == *path) {
                        f.concerns.push(Concern::Malware(sig.clone()));
                    }
                }
            }
            found
        }
        None => Clam::NotInstalled,
    };
    let definitions = opts.clamscan.as_deref().and_then(freshclam::definitions);
    Report {
        files: reports,
        clamav,
        opened_rar_7z: opts.seven_zip.is_some(),
        definitions,
    }
}

/// `rel` under `root`, file by file (a folder's files, an app bundle whole).
fn collect(root: &Path, rel: &Path, out: &mut Vec<(PathBuf, PathBuf)>) {
    let path = root.join(rel);
    let Ok(meta) = std::fs::symlink_metadata(&path) else {
        return;
    };
    if meta.is_dir() && !is_bundle(&path) {
        let Ok(entries) = std::fs::read_dir(&path) else {
            return;
        };
        let mut names: Vec<PathBuf> = entries.flatten().map(|e| rel.join(e.file_name())).collect();
        names.sort();
        for n in names {
            collect(root, &n, out);
        }
    } else if meta.is_file() || is_bundle(&path) {
        out.push((rel.to_owned(), path));
    }
}

/// A macOS app bundle (a folder named `*.app`): one file to the person, and to the checks.
fn is_bundle(path: &Path) -> bool {
    path.is_dir()
        && path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("app"))
}

fn check_file(rel: &Path, path: &Path, opts: &Options) -> FileReport {
    let name = rel
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let (kind, size) = if is_bundle(path) {
        (Kind::MacApp, 0)
    } else {
        let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        (kinds::of_file(path, &name), size)
    };
    let mut concerns = name_concerns(&name, kind);
    if kind.runs() {
        concerns.push(Concern::Runs(kind));
    }
    match kind {
        Kind::Zip => concerns.extend(zip_concerns(path)),
        Kind::Rar | Kind::SevenZip => {
            if let Some(seven) = &opts.seven_zip {
                concerns.extend(seven_zip_concerns(seven, path));
            }
        }
        _ => {}
    }
    if matches!(kind, Kind::MacApp | Kind::MacInstaller)
        && let Some(why) = macsig::unsigned(path, kind)
    {
        concerns.push(Concern::Unsigned(why));
    }
    FileReport {
        path: rel.to_owned(),
        size,
        kind,
        concerns,
    }
}

/// What the name says against what the bytes are.
fn name_concerns(name: &str, is: Kind) -> Vec<Concern> {
    let mut out = Vec::new();
    let claims = kinds::of_name(name);
    if claims.harmless() && (is.runs() || is.archive()) && claims != is {
        out.push(Concern::Disguised { claims, is });
    }
    if kinds::double_extension(name) {
        out.push(Concern::DoubleExtension);
    }
    out
}

/// A ZIP's table of contents: a locked entry, or the first program in it.
fn zip_concerns(path: &Path) -> Vec<Concern> {
    let Ok(file) = std::fs::File::open(path) else {
        return Vec::new();
    };
    let Ok(mut zip) = zip::ZipArchive::new(file) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut locked = false;
    let mut inside = None;
    // A ZIP can name a million entries: the first 100 000 tell enough.
    for i in 0..zip.len().min(100_000) {
        let Ok(entry) = zip.by_index_raw(i) else {
            continue;
        };
        locked |= entry.encrypted();
        let Ok(name) = entry.name().map(|n| n.into_owned()) else {
            continue;
        };
        if inside.is_none() && kinds::of_name(&name).runs() {
            inside = Some(name);
        }
    }
    if locked {
        out.push(Concern::LockedArchive);
    }
    if let Some(name) = inside {
        out.push(Concern::RunsInside(name));
    }
    out
}

/// A RAR's or 7z's list through 7-Zip (`7z l -slt`, never extracted): a locked one, or the
/// first program in it.
fn seven_zip_concerns(seven: &Path, path: &Path) -> Vec<Concern> {
    let out = std::process::Command::new(seven)
        // An empty password: a locked archive then says so instead of waiting for one.
        .args(["l", "-slt", "-p", "-sccUTF-8", "--"])
        .arg(path)
        .stdin(std::process::Stdio::null())
        .output();
    match out {
        Ok(o) => seven_zip_listing(
            &String::from_utf8_lossy(&o.stdout),
            &String::from_utf8_lossy(&o.stderr),
        ),
        Err(e) => {
            tracing::warn!("scan: 7-Zip didn't run for {}: {e}", path.display());
            Vec::new()
        }
    }
}

/// 7-Zip's technical listing (`-slt`) read for [`seven_zip_concerns`].
fn seven_zip_listing(stdout: &str, stderr: &str) -> Vec<Concern> {
    let mut out = Vec::new();
    let locked = stdout.lines().any(|l| l.trim() == "Encrypted = +")
        || stderr.contains("Wrong password")
        || stderr.contains("Can not open encrypted archive");
    if locked {
        out.push(Concern::LockedArchive);
    }
    let first = stdout
        .lines()
        .filter_map(|l| l.strip_prefix("Path = "))
        .skip(1) // the archive's own path comes first
        .find(|name| kinds::of_name(name).runs());
    if let Some(name) = first {
        out.push(Concern::RunsInside(name.to_owned()));
    }
    out
}

#[cfg(test)]
mod tests;
