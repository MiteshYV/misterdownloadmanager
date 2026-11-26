//! Quarantine: a download is saved to a hidden folder beside where it goes, its files are
//! checked when it finishes (mdm-scan: a program hiding behind a video's name, installers and
//! apps, a locked archive or a program inside one, an app or installer not signed by an
//! identified developer, and what ClamAV finds when it is installed), and only then does it go
//! to its folder. Torrents always, with Options → Downloads → "Check torrents in quarantine"
//! (on); every other download too with "... and every download" (off).
//!
//! The hidden folder is `<its folder>/.mdm-quarantine/<key>`: on the same disk as where the
//! download goes, so a clean one gets there by a rename, however large; hidden in Finder and in
//! the file managers. Nothing in it is opened: only its first bytes are read, a ZIP's table of
//! contents, and outside tools asked.
//!
//! A clean download goes to its folder the way a finished one is moved (a torrent through its
//! host, which attaches it again there to keep sharing), and only then counts as finished: its
//! sound, Download complete and its extraction come once it is there. One with a concern stays
//! in quarantine, its row "Held in quarantine", a torrent stopped (it shares nothing held), and
//! a window says what was found, file by file: Release anyway, Keep in quarantine, or Delete
//! (the usual delete question, with its files).

use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, TryRecvError};

use serde::{Deserialize, Serialize};

use crate::dialogs::{Outcome, window};
use crate::model::{Item, Status};
use crate::winui::{self, ButtonKind};

/// The hidden folder in a download's folder that holds what is in quarantine.
pub const FOLDER: &str = ".mdm-quarantine";

/// A download held in quarantine (`Item::quarantine`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hold {
    /// Its folder in quarantine (`<release_to>/.mdm-quarantine/<key>`).
    pub dir: PathBuf,
    /// The folder it goes to once released.
    pub release_to: PathBuf,
    pub state: State,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum State {
    /// Downloading into quarantine.
    #[default]
    Downloading,
    /// Finished: its files are being checked.
    Checking,
    /// A check found something: held until the person decides.
    Held,
    /// Clean, or released by hand: going to its folder.
    Releasing,
}

impl Hold {
    /// The quarantine for a download going to `release_to`, under `key` (one per download).
    pub fn new(release_to: &Path, key: &str) -> Self {
        Hold {
            dir: folder_in(release_to, key),
            release_to: release_to.to_owned(),
            state: State::Downloading,
        }
    }

    /// What its row's Status says once it has finished (None: as any finished download).
    pub fn status_text(&self) -> Option<String> {
        Some(match self.state {
            State::Downloading => return None,
            // i18n: a download's status: its files are checked in quarantine before they go to its folder
            State::Checking => crate::i18n::t("Checking files...").into(),
            // i18n: a download's status: a check found something, so its files stay in quarantine until the user decides
            State::Held => crate::i18n::t("Held in quarantine").into(),
            // i18n: a download's status: its checked files are going from quarantine to its folder
            State::Releasing => crate::i18n::t("Moving...").into(),
        })
    }

    /// Where `entry` (a file or folder in quarantine, the download's own) goes when released.
    pub fn release_path(&self, entry: &Path) -> Option<PathBuf> {
        Some(self.release_to.join(entry.file_name()?))
    }
}

/// `<dir>/.mdm-quarantine/<key>`.
pub fn folder_in(dir: &Path, key: &str) -> PathBuf {
    dir.join(FOLDER).join(key)
}

/// A torrent's key: its info hash's first 16 digits (enough to tell torrents apart, short in a
/// path).
pub fn torrent_key(hash: &str) -> String {
    format!("t-{}", hash.get(..16).unwrap_or(hash))
}

/// A web download's key: its row's id.
pub fn download_key(id: u64) -> String {
    format!("d-{id}")
}

/// Whether a torrent can be held: its files in one place of their own, its file or its folder,
/// which a release moves whole. A multi-file torrent added without its own folder (straight
/// into a folder others share) can't be, and downloads where it was asked to.
pub fn torrent_can_hold(files: usize, create_subfolder: bool) -> bool {
    files <= 1 || create_subfolder
}

/// Whether a web download can be held when it starts: a plain file not started yet (a video, a
/// gallery, a ZIP's files or a debrid service's download has a way of its own).
pub fn web_can_hold(item: &Item) -> bool {
    item.quarantine.is_none()
        && item.torrent.is_none()
        && item.video.is_none()
        && item.gallery.is_none()
        && item.remote.is_none()
        && item.zip.is_none()
        && item.downloaded == 0
        && item.status != Status::Complete
}

/// Quarantine's folder emptied once its download is released: its own folder, and the shared
/// `.mdm-quarantine` when nothing else is in it. Only empty folders go.
pub fn tidy(hold: &Hold) {
    let _ = std::fs::remove_dir(&hold.dir);
    if let Some(parent) = hold.dir.parent()
        && parent.file_name().is_some_and(|n| n == FOLDER)
    {
        let _ = std::fs::remove_dir(parent);
    }
}

/// What came of a check.
pub enum Done {
    /// Nothing found: it goes to its folder now.
    Clean(u64),
    /// Something found: held, its window open (or waiting behind another's).
    Flagged(u64),
}

/// The window of what was found in one download.
pub struct Review {
    pub id: u64,
    name: String,
    report: mdm_scan::Report,
}

/// What the person chose in a [`Review`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Choice {
    Release,
    Keep,
    Delete,
}

/// The checks under way, and the findings waiting for the person.
#[derive(Default)]
pub struct Quarantine {
    running: Vec<(u64, Receiver<mdm_scan::Report>)>,
    /// The reports of downloads held, by id, until their window is answered.
    reports: Vec<(u64, String, mdm_scan::Report)>,
    /// The window showing now (one at a time).
    pub review: Option<Review>,
}

impl Quarantine {
    /// Download `id` just finished: whether it waits for its check (true: its finishing comes
    /// once it is released, [`Done::Clean`]).
    pub fn hold(&mut self, id: u64, items: &mut [Item], ctx: &egui::Context) -> bool {
        let Some(item) = items.iter_mut().find(|i| i.id == id) else {
            return false;
        };
        let Some(hold) = item.quarantine.as_mut() else {
            return false;
        };
        match hold.state {
            State::Downloading | State::Checking => {
                hold.state = State::Checking;
                let dir = hold.dir.clone();
                // i18n: in a download's log: its files are checked in quarantine before they go to its folder
                item.note(crate::i18n::t("Checking its files in quarantine..."));
                self.start(id, dir, ctx);
                true
            }
            // Held, or already on its way: its finishing waits for that.
            State::Held | State::Releasing => true,
        }
    }

    /// At start: a check MDM's closing cut short runs again; what was held stays held, its
    /// window shown again.
    pub fn resume(&mut self, items: &mut [Item], ctx: &egui::Context) {
        for item in items.iter_mut() {
            let Some(hold) = &item.quarantine else {
                continue;
            };
            if item.status == Status::Complete && hold.state == State::Checking {
                let dir = hold.dir.clone();
                self.start(item.id, dir, ctx);
            } else if hold.state == State::Held {
                let dir = hold.dir.clone();
                // Its findings again, for its window (a check is quick: no file is opened).
                self.start(item.id, dir, ctx);
            }
        }
    }

    fn start(&mut self, id: u64, dir: PathBuf, ctx: &egui::Context) {
        if self.running.iter().any(|(r, _)| *r == id) {
            return;
        }
        let (tx, rx) = std::sync::mpsc::channel();
        let wake = ctx.clone();
        crate::launch::spawn("quarantine check", move || {
            let report = check(&dir);
            let _ = tx.send(report);
            wake.request_repaint();
        });
        self.running.push((id, rx));
    }

    /// Checks that ended since the last call.
    pub fn poll(&mut self, items: &mut [Item]) -> Vec<Done> {
        let mut ended = Vec::new();
        self.running.retain(|(id, rx)| match rx.try_recv() {
            Ok(r) => {
                ended.push((*id, r));
                false
            }
            Err(TryRecvError::Empty) => true,
            Err(TryRecvError::Disconnected) => false,
        });
        let mut out = Vec::new();
        for (id, report) in ended {
            let Some(item) = items.iter_mut().find(|i| i.id == id) else {
                continue;
            };
            let Some(state) = item.quarantine.as_ref().map(|h| h.state) else {
                continue;
            };
            for f in report.flagged() {
                let why: Vec<String> = f.concerns.iter().map(|c| c.describe()).collect();
                tracing::warn!(
                    "quarantine: download {id}: {}: {}",
                    f.path.display(),
                    why.join("; ")
                );
            }
            let next = if report.clean() && state != State::Held {
                // i18n: in a download's log: its files were checked in quarantine and nothing was found
                item.note(crate::i18n::t("Checked in quarantine: nothing found"));
                out.push(Done::Clean(id));
                State::Releasing
            } else {
                if state != State::Held {
                    let n = report.flagged().count() as u64;
                    item.note(crate::i18n::tn(
                        n,
                        // i18n: in a download's log: the quarantine check found something in {n} of its files
                        "Held in quarantine: {n} file needs your attention",
                        "Held in quarantine: {n} files need your attention",
                    ));
                }
                let name = item.file_name.clone();
                self.reports.retain(|(r, ..)| *r != id);
                self.reports.push((id, name, report));
                out.push(Done::Flagged(id));
                State::Held
            };
            if let Some(hold) = item.quarantine.as_mut() {
                hold.state = next;
            }
        }
        if self.review.is_none()
            && let Some(at) = self
                .reports
                .iter()
                .position(|(id, ..)| items.iter().any(|i| i.id == *id))
        {
            let (id, name, report) = self.reports.remove(at);
            self.review = Some(Review { id, name, report });
        }
        // A download deleted meanwhile has no window to wait for.
        self.reports
            .retain(|(id, ..)| items.iter().any(|i| i.id == *id));
        out
    }

    /// Opens download `id`'s window again (its row's Open, while it is held).
    pub fn reopen(&mut self, id: u64, items: &mut [Item], ctx: &egui::Context) {
        if self.review.as_ref().is_some_and(|r| r.id == id) {
            return;
        }
        if let Some(at) = self.reports.iter().position(|(r, ..)| *r == id) {
            let (id, name, report) = self.reports.remove(at);
            if let Some(shown) = self.review.take() {
                self.reports.push((shown.id, shown.name, shown.report));
            }
            self.review = Some(Review { id, name, report });
        } else if let Some(item) = items.iter().find(|i| i.id == id)
            && let Some(hold) = &item.quarantine
        {
            let dir = hold.dir.clone();
            self.start(id, dir, ctx);
        }
    }
}

/// ClamAV's virus definitions refreshed when they are more than a day old (`freshclam`, at
/// most every few hours: `mdm_scan::freshclam`). Blocks while it runs: from a background
/// thread. Nothing when ClamAV isn't installed, there is no data folder, or in tests.
pub fn refresh_definitions() {
    if cfg!(test) {
        return;
    }
    let (Some(clamscan), Some(data)) = (mdm_scan::find_clamscan(), mdm_ipc::data_dir()) else {
        return;
    };
    let _ = mdm_scan::freshclam::refresh_if_stale(&clamscan, &data);
}

/// Every file in `dir` (a download's quarantine), checked with the tools this computer has
/// (ClamAV's definitions refreshed first when they are old).
pub fn check(dir: &Path) -> mdm_scan::Report {
    refresh_definitions();
    let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|r| r.flatten().map(|e| PathBuf::from(e.file_name())).collect())
        .unwrap_or_default();
    entries.sort();
    mdm_scan::scan(dir, &entries, &mdm_scan::Options::found())
}

/// A concern in the interface's words.
fn concern_text(c: &mdm_scan::Concern) -> String {
    use mdm_scan::Concern;
    match c {
        Concern::Disguised { is, .. } => crate::i18n::fmt(
            // i18n: quarantine: a file whose name says video/picture/document but whose content is something else; {kind} names what it really is (a program, an archive)
            crate::i18n::t("Not what its name says: it is {kind}"),
            &[("kind", &kind_text(*is))],
        ),
        // i18n: quarantine: a file named like "film.mp4.exe", made to look like a harmless file
        Concern::DoubleExtension => {
            crate::i18n::t("A program named to look like another file").into()
        }
        Concern::Runs(k) => kind_text(*k),
        // i18n: quarantine: an archive locked with a password, whose contents can't be checked
        Concern::LockedArchive => {
            crate::i18n::t("Locked with a password: its contents can't be checked").into()
        }
        Concern::RunsInside(name) => crate::i18n::fmt(
            // i18n: quarantine: an archive with a program inside; {name} is the program's name
            crate::i18n::t("Holds a program: {name}"),
            &[("name", &crate::i18n::isolate(name))],
        ),
        Concern::Unsigned(why) => crate::i18n::fmt(
            // i18n: quarantine (macOS): an app or installer not signed by an identified developer; {why} is what macOS said
            crate::i18n::t("Not signed by an identified developer ({why})"),
            &[("why", why)],
        ),
        Concern::Malware(sig) => crate::i18n::fmt(
            // i18n: quarantine: ClamAV, a virus scanner, found something; {name} is its name for it
            crate::i18n::t("ClamAV found {name}"),
            &[("name", sig)],
        ),
    }
}

fn kind_text(k: mdm_scan::Kind) -> String {
    use mdm_scan::Kind;
    crate::i18n::t(match k {
        // i18n: quarantine: what a file is
        Kind::WindowsProgram => "A Windows program",
        // i18n: quarantine: what a file is
        Kind::MacProgram => "A Mac program",
        // i18n: quarantine: what a file is
        Kind::LinuxProgram => "A Linux program",
        // i18n: quarantine: what a file is
        Kind::Script => "A script",
        // i18n: quarantine: what a file is
        Kind::MacInstaller => "A Mac installer",
        // i18n: quarantine: what a file is
        Kind::Installer => "An installer",
        // i18n: quarantine: what a file is
        Kind::MacApp => "A Mac app",
        // i18n: quarantine: what a file is
        Kind::MacDiskImage => "A Mac disk image, which may hold an app",
        // i18n: quarantine: what a file is
        Kind::Shortcut => "A shortcut to a program",
        // i18n: quarantine: what a file is
        Kind::Zip | Kind::Rar | Kind::SevenZip | Kind::Compressed => "An archive",
        // i18n: quarantine: what a file is
        _ => "Another kind of file",
    })
    .to_owned()
}

/// What ClamAV did, for the window's last line, with a warning when its virus definitions
/// are old (they miss what is new).
fn clam_text(r: &mdm_scan::Report) -> String {
    let said = clam_said(&r.clamav);
    let days = r
        .definitions
        .map(|d| d.days_old(std::time::SystemTime::now()));
    match days {
        Some(n)
            if n >= mdm_scan::freshclam::WARN_DAYS && r.clamav != mdm_scan::Clam::NotInstalled =>
        {
            crate::i18n::fmt(
                // i18n: quarantine window: after what ClamAV said ({said}), its virus definitions are {n} days old (n is 3 or more)
                crate::i18n::t("{said}; its virus definitions are {n} days old"),
                &[("said", &said), ("n", &n.to_string())],
            )
        }
        _ => said,
    }
}

fn clam_said(c: &mdm_scan::Clam) -> String {
    use mdm_scan::Clam;
    match c {
        // i18n: quarantine window: ClamAV, a free virus scanner, isn't on this computer
        Clam::NotInstalled => {
            crate::i18n::t("Virus scan: ClamAV isn't installed (it is free: clamav.net)").into()
        }
        // i18n: quarantine window: ClamAV checked every file
        Clam::Clean => crate::i18n::t("Virus scan: ClamAV found nothing").into(),
        Clam::Found(hits) => crate::i18n::tn(
            hits.len() as u64,
            // i18n: quarantine window: ClamAV found something in {n} files
            "Virus scan: ClamAV found something in {n} file",
            "Virus scan: ClamAV found something in {n} files",
        ),
        Clam::Failed(why) => crate::i18n::fmt(
            // i18n: quarantine window: ClamAV couldn't check; {why} is what it said
            crate::i18n::t("Virus scan: ClamAV couldn't check ({why})"),
            &[("why", why)],
        ),
    }
}

/// The rows the window lists: the flagged files first, then the clean ones, at most `max`.
fn rows(report: &mdm_scan::Report, max: usize) -> Vec<(String, Option<String>)> {
    report
        .flagged()
        .map(|f| {
            let why: Vec<String> = f.concerns.iter().map(concern_text).collect();
            (f.path.display().to_string(), Some(why.join("; ")))
        })
        .chain(
            report
                .clean_files()
                .map(|f| (f.path.display().to_string(), None)),
        )
        .take(max)
        .collect()
}

impl Review {
    /// `Done(choice)`, or Closed (as Keep).
    pub fn show(&mut self, ctx: &egui::Context) -> Outcome<Choice> {
        const SHOWN: usize = 8;
        let flagged = self.report.flagged().count();
        let total = self.report.files.len();
        let list = rows(&self.report, SHOWN);
        let more = total.saturating_sub(list.len());
        let height = 150.0 + 30.0 * list.len() as f32 + if more > 0 { 16.0 } else { 0.0 };
        window(
            ctx,
            "quarantine-review",
            // i18n: window title: a download held in quarantine because its check found something
            "Held in quarantine",
            [520.0, height],
            false,
            winui::LIGHT_BG(),
            |ui, o| {
                let p = ui.painter().clone();
                let title = crate::i18n::fmt(
                    crate::i18n::tn(
                        flagged as u64,
                        // i18n: in the quarantine window: {name} is the download, {n} how many of its files the check flagged
                        "{name} is held for review: {n} file needs your attention.",
                        "{name} is held for review: {n} files need your attention.",
                    )
                    .as_str(),
                    &[("name", &crate::i18n::isolate(&self.name))],
                );
                winui::paragraph(
                    &p,
                    o.x + 12.0,
                    o.y + 22.0,
                    &title,
                    winui::LIGHT_TEXT(),
                    o.x + 508.0,
                    15.0,
                );
                winui::paragraph(
                    &p,
                    o.x + 12.0,
                    o.y + 56.0,
                    // i18n: in the quarantine window, under its title
                    crate::i18n::t(
                        "Nothing in it has been opened. It stays in quarantine until you decide.",
                    ),
                    winui::MUTED(),
                    o.x + 508.0,
                    14.0,
                );
                let mut y = o.y + 86.0;
                for (path, why) in &list {
                    let ink = if why.is_some() {
                        winui::ERROR()
                    } else {
                        winui::LIGHT_TEXT()
                    };
                    let name = winui::ellipsize(&p, &winui::segoe(), path, 490.0);
                    winui::text(&p, o.x + 12.0, y, &name, ink);
                    let note = why.clone().unwrap_or_else(|| {
                        // i18n: in the quarantine window: a file whose check found nothing
                        crate::i18n::t("Nothing found").to_owned()
                    });
                    let note = winui::ellipsize(&p, &winui::segoe(), &note, 490.0);
                    winui::text(&p, o.x + 24.0, y + 14.0, &note, winui::MUTED());
                    y += 30.0;
                }
                if more > 0 {
                    let line = crate::i18n::tn(
                        more as u64,
                        // i18n: in the quarantine window: how many more files (all with nothing found) aren't listed
                        "and {n} more file with nothing found",
                        "and {n} more files with nothing found",
                    );
                    winui::text(&p, o.x + 12.0, y, &line, winui::MUTED());
                    y += 16.0;
                }
                winui::text(
                    &p,
                    o.x + 12.0,
                    y + 6.0,
                    &clam_text(&self.report),
                    winui::MUTED(),
                );
                let row = winui::buttons_light_right(
                    ui,
                    o,
                    508.0,
                    y + 22.0 - o.y,
                    &[
                        // i18n: quarantine window button: move the download to its folder although something was found
                        (crate::i18n::mark("Release anyway"), ButtonKind::Normal),
                        // i18n: quarantine window button: leave it in quarantine and close the window
                        (crate::i18n::mark("Keep in quarantine"), ButtonKind::Default),
                        (crate::i18n::mark("Delete"), ButtonKind::Normal),
                    ],
                );
                if row[0].clicked() {
                    Outcome::Done(Choice::Release)
                } else if row[1].clicked() || winui::enter_pressed(ui) || winui::escape_pressed(ui)
                {
                    Outcome::Done(Choice::Keep)
                } else if row[2].clicked() {
                    Outcome::Done(Choice::Delete)
                } else {
                    Outcome::Open
                }
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The quarantine is in the download's own folder (the same disk), hidden, one per download.
    #[test]
    fn a_downloads_quarantine_is_hidden_beside_its_folder() {
        let to = Path::new("/Users/me/Downloads/Video");
        let h = Hold::new(to, &torrent_key("0123456789abcdef0123456789abcdef01234567"));
        assert_eq!(
            h.dir,
            PathBuf::from("/Users/me/Downloads/Video/.mdm-quarantine/t-0123456789abcdef")
        );
        assert_eq!(h.state, State::Downloading);
        assert_eq!(
            h.release_path(&h.dir.join("Film (2026)")),
            Some(to.join("Film (2026)"))
        );
        assert_eq!(download_key(42), "d-42");
    }

    /// A torrent's files must be one place of their own to be moved out whole.
    #[test]
    fn only_a_torrent_with_its_own_place_is_held() {
        assert!(torrent_can_hold(1, false));
        assert!(torrent_can_hold(5, true));
        assert!(!torrent_can_hold(5, false));
    }

    /// A clean report releases; a flagged one holds and opens its window; a flagged download
    /// stays held when checked again (at the next start).
    #[test]
    fn checks_release_or_hold() {
        let dir = tempfile::tempdir().unwrap();
        let clean = dir.path().join("clean");
        std::fs::create_dir_all(&clean).unwrap();
        std::fs::write(clean.join("a.mp4"), b"\x00\x00\x00\x18ftypmp42").unwrap();
        assert!(check(&clean).clean());
        let bad = dir.path().join("bad");
        std::fs::create_dir_all(&bad).unwrap();
        std::fs::write(bad.join("film.mp4"), b"MZ\x90\x00").unwrap();
        let report = check(&bad);
        assert!(!report.clean());
        let rows = rows(&report, 8);
        assert_eq!(rows.len(), 1);
        assert!(rows[0].1.is_some());
    }

    /// Statuses: none while downloading, each state's word after.
    #[test]
    fn a_held_download_says_where_it_is() {
        let mut h = Hold::new(Path::new("/d"), "d-1");
        assert_eq!(h.status_text(), None);
        h.state = State::Held;
        assert_eq!(h.status_text().as_deref(), Some("Held in quarantine"));
        h.state = State::Checking;
        assert_eq!(h.status_text().as_deref(), Some("Checking files..."));
    }

    /// Tidying takes only empty folders.
    #[test]
    fn tidying_never_takes_a_file() {
        let dir = tempfile::tempdir().unwrap();
        let h = Hold::new(dir.path(), "d-7");
        std::fs::create_dir_all(&h.dir).unwrap();
        std::fs::write(h.dir.join("left.bin"), b"x").unwrap();
        tidy(&h);
        assert!(h.dir.join("left.bin").exists());
        std::fs::remove_file(h.dir.join("left.bin")).unwrap();
        tidy(&h);
        assert!(!h.dir.exists());
        assert!(!dir.path().join(FOLDER).exists());
    }
}
