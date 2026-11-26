//! Keeping ClamAV's virus definitions up to date: a scanner with old definitions misses what is
//! new. ClamAV's own updater, `freshclam` (beside `clamscan`), is run in the background when
//! the definitions are more than a day old, at most every few hours. MDM writes its own small
//! `freshclam.conf` in its data folder, so it works on a fresh `brew install clamav` (whose own
//! config is only a sample until the person edits it), and points it at the folder `clamscan`
//! reads its definitions from.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

/// Definitions older than this are refreshed (ClamAV publishes new ones several times a day).
pub const STALE: Duration = Duration::from_secs(24 * 3600);
/// After an attempt, the next one waits at least this long (ClamAV's mirrors block clients
/// that ask too often).
pub const RETRY: Duration = Duration::from_secs(6 * 3600);
/// The quarantine window warns about definitions this many days old or older.
pub const WARN_DAYS: u64 = 3;

/// The virus definitions `clamscan` has: their version and when they were published.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Definitions {
    pub version: u32,
    /// Seconds since 1970 (as ClamAV prints it, taken as UTC).
    pub published: u64,
}

impl Definitions {
    /// Whole days since they were published.
    pub fn days_old(&self, now: SystemTime) -> u64 {
        let now = now
            .duration_since(SystemTime::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        now.saturating_sub(self.published) / 86_400
    }
}

/// What a refresh did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refresh {
    /// The definitions were recent enough, or the last attempt was too recent.
    NotNeeded,
    /// `freshclam` ran and succeeded.
    Updated,
    /// It couldn't (no `freshclam`, no network, a folder it can't write): why.
    Failed(String),
}

/// The definitions `clamscan` has (None: none yet, or it didn't run).
pub fn definitions(clamscan: &Path) -> Option<Definitions> {
    let out = std::process::Command::new(clamscan)
        .arg("--version")
        .stdin(std::process::Stdio::null())
        .output()
        .ok()?;
    parse_version(&String::from_utf8_lossy(&out.stdout))
}

/// `ClamAV 1.4.1/27470/Tue Nov 25 08:35:00 2026`: the definitions' version and date (a
/// `clamscan` with no definitions prints only `ClamAV 1.4.1`).
pub(crate) fn parse_version(s: &str) -> Option<Definitions> {
    let mut parts = s.trim().splitn(3, '/');
    let _engine = parts.next()?;
    let version = parts.next()?.trim().parse().ok()?;
    let published = parse_ctime(parts.next()?)?;
    Some(Definitions { version, published })
}

/// `Tue Nov 25 08:35:00 2026` (C's ctime) as seconds since 1970.
fn parse_ctime(s: &str) -> Option<u64> {
    let f: Vec<&str> = s.split_whitespace().collect();
    let [_, month, day, time, year] = f.as_slice() else {
        return None;
    };
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let m = MONTHS.iter().position(|x| x == month)? as i64 + 1;
    let d: i64 = day.parse().ok()?;
    let y: i64 = year.parse().ok()?;
    let mut hms = time.split(':').map(|x| x.parse::<i64>());
    let (h, mi, se) = (hms.next()?.ok()?, hms.next()?.ok()?, hms.next()?.ok()?);
    if !(1..=31).contains(&d) || h > 23 || mi > 59 || se > 60 {
        return None;
    }
    let secs = days_from_civil(y, m, d) * 86_400 + h * 3600 + mi * 60 + se;
    u64::try_from(secs).ok()
}

/// Days from 1970-01-01 to a date (Howard Hinnant's algorithm).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// `freshclam` beside `clamscan`.
pub fn find_freshclam(clamscan: &Path) -> Option<PathBuf> {
    let name = if cfg!(windows) {
        "freshclam.exe"
    } else {
        "freshclam"
    };
    let p = clamscan.with_file_name(name);
    p.is_file().then_some(p)
}

/// Where `clamscan` (at `<prefix>/bin/clamscan`) reads its definitions: the first of
/// [`database_dirs`] that has them, else the first that exists, else Homebrew's.
pub fn database_dir(clamscan: &Path) -> PathBuf {
    let dirs = database_dirs(clamscan);
    let has = |d: &PathBuf| {
        ["main.cvd", "main.cld", "daily.cvd", "daily.cld"]
            .iter()
            .any(|f| d.join(f).is_file())
    };
    dirs.iter()
        .find(|d| has(d))
        .or_else(|| dirs.iter().find(|d| d.is_dir()))
        .unwrap_or(&dirs[0])
        .clone()
}

/// Homebrew's `<prefix>/var/lib/clamav`, MacPorts' `<prefix>/share/clamav`, Linux's
/// `/var/lib/clamav`, Windows' `database` beside the program.
pub(crate) fn database_dirs(clamscan: &Path) -> Vec<PathBuf> {
    let bin = clamscan.parent().unwrap_or(Path::new("."));
    if cfg!(windows) {
        return vec![bin.join("database")];
    }
    let prefix = bin.parent().unwrap_or(Path::new("/"));
    let mut out = vec![
        prefix.join("var/lib/clamav"),
        prefix.join("share/clamav"),
        PathBuf::from("/var/lib/clamav"),
    ];
    out.dedup();
    out
}

/// MDM's `freshclam.conf`: the official mirror, and the folder `clamscan` reads.
pub(crate) fn config(db: &Path) -> String {
    format!(
        "# Written by Mister Download Manager to keep ClamAV's virus definitions up to date.\n\
         DatabaseMirror database.clamav.net\n\
         DatabaseDirectory {}\n",
        db.display()
    )
}

/// Whether to run `freshclam` now: the definitions are missing or older than [`STALE`], and
/// the last attempt (`last`) was at least [`RETRY`] ago.
pub(crate) fn due(defs: Option<Definitions>, last: Option<SystemTime>, now: SystemTime) -> bool {
    let stale = match defs {
        None => true,
        Some(d) => {
            let published = SystemTime::UNIX_EPOCH + Duration::from_secs(d.published);
            now.duration_since(published).unwrap_or_default() >= STALE
        }
    };
    let waited = last.is_none_or(|t| now.duration_since(t).unwrap_or_default() >= RETRY);
    stale && waited
}

/// One refresh at a time (the app starts one, and each quarantine check may too).
static RUNNING: Mutex<()> = Mutex::new(());

/// Refresh the definitions if they are due ([`due`]). `data` is MDM's data folder: its
/// `clamav/` keeps the config and when the last attempt was. Blocks while `freshclam` runs, so
/// call it from a background thread.
pub fn refresh_if_stale(clamscan: &Path, data: &Path) -> Refresh {
    let Ok(_one) = RUNNING.try_lock() else {
        return Refresh::NotNeeded;
    };
    let dir = data.join("clamav");
    let stamp = dir.join("last-update");
    let last = std::fs::metadata(&stamp).and_then(|m| m.modified()).ok();
    if !due(definitions(clamscan), last, SystemTime::now()) {
        return Refresh::NotNeeded;
    }
    let Some(freshclam) = find_freshclam(clamscan) else {
        return Refresh::Failed("freshclam isn't installed beside clamscan".into());
    };
    let db = database_dir(clamscan);
    let conf = dir.join("freshclam.conf");
    let written = std::fs::create_dir_all(&dir)
        .and_then(|()| std::fs::create_dir_all(&db))
        .and_then(|()| std::fs::write(&conf, config(&db)))
        .and_then(|()| std::fs::write(&stamp, b""));
    if let Err(e) = written {
        return Refresh::Failed(format!("couldn't set up freshclam: {e}"));
    }
    let out = std::process::Command::new(freshclam)
        .arg(format!("--config-file={}", conf.display()))
        .args(["--stdout", "--quiet"])
        .stdin(std::process::Stdio::null())
        .output();
    match out {
        Ok(o) if o.status.success() => {
            tracing::info!("ClamAV: virus definitions updated");
            Refresh::Updated
        }
        Ok(o) => {
            let said = String::from_utf8_lossy(&o.stdout)
                .lines()
                .chain(String::from_utf8_lossy(&o.stderr).lines())
                .map(str::trim)
                .find(|l| !l.is_empty())
                .unwrap_or("freshclam failed")
                .to_owned();
            tracing::warn!("ClamAV: definitions not updated: {said}");
            Refresh::Failed(said)
        }
        Err(e) => Refresh::Failed(format!("freshclam didn't run: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamscans_version_is_read() {
        let d = parse_version("ClamAV 1.4.1/27470/Tue Nov 25 08:35:00 2026\n").unwrap();
        assert_eq!(d.version, 27470);
        // 2026-11-25 08:35:00 UTC
        assert_eq!(d.published, 1_795_595_700);
        // ctime pads a one-digit day with a space
        let d = parse_version("ClamAV 1.0.1/26812/Mon Feb  6 07:24:46 2023").unwrap();
        assert_eq!(d.published, 1_675_668_286);
        // no definitions yet
        assert_eq!(parse_version("ClamAV 1.4.1\n"), None);
        assert_eq!(parse_version(""), None);
        assert_eq!(
            parse_version("ClamAV 1.4.1/x/Tue Nov 25 08:35:00 2026"),
            None
        );
    }

    #[test]
    fn age_is_in_whole_days() {
        let d = Definitions {
            version: 1,
            published: 1_795_595_700,
        };
        let at = |s| SystemTime::UNIX_EPOCH + Duration::from_secs(s);
        assert_eq!(d.days_old(at(1_795_595_700 + 86_399)), 0);
        assert_eq!(d.days_old(at(1_795_595_700 + 3 * 86_400)), 3);
        assert_eq!(d.days_old(at(0)), 0);
    }

    #[test]
    fn refreshed_when_stale_and_not_asked_lately() {
        let at = |s| SystemTime::UNIX_EPOCH + Duration::from_secs(s);
        let now = at(10_000_000);
        let fresh = Some(Definitions {
            version: 1,
            published: 10_000_000 - 3600,
        });
        let old = Some(Definitions {
            version: 1,
            published: 10_000_000 - 30 * 3600,
        });
        assert!(!due(fresh, None, now));
        assert!(due(old, None, now));
        assert!(due(None, None, now));
        // tried an hour ago: wait
        assert!(!due(old, Some(at(10_000_000 - 3600)), now));
        assert!(!due(None, Some(at(10_000_000 - 3600)), now));
        // tried seven hours ago: again
        assert!(due(old, Some(at(10_000_000 - 7 * 3600)), now));
    }

    #[cfg(not(windows))]
    #[test]
    fn definitions_folder_follows_clamscans_prefix() {
        let dirs = database_dirs(Path::new("/opt/homebrew/bin/clamscan"));
        assert_eq!(dirs[0], PathBuf::from("/opt/homebrew/var/lib/clamav"));
        assert_eq!(dirs[1], PathBuf::from("/opt/homebrew/share/clamav"));
        assert_eq!(dirs[2], PathBuf::from("/var/lib/clamav"));

        let t = tempfile::tempdir().unwrap();
        let bin = t.path().join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let clamscan = bin.join("clamscan");
        // nothing there yet: Homebrew's place
        assert_eq!(database_dir(&clamscan), t.path().join("var/lib/clamav"));
        // definitions in MacPorts' place: that one
        let ports = t.path().join("share/clamav");
        std::fs::create_dir_all(&ports).unwrap();
        std::fs::write(ports.join("daily.cld"), b"x").unwrap();
        std::fs::create_dir_all(t.path().join("var/lib/clamav")).unwrap();
        assert_eq!(database_dir(&clamscan), ports);
    }

    #[test]
    fn config_names_the_mirror_and_folder() {
        let c = config(Path::new("/opt/homebrew/var/lib/clamav"));
        assert!(c.contains("DatabaseMirror database.clamav.net\n"));
        assert!(c.contains("DatabaseDirectory /opt/homebrew/var/lib/clamav\n"));
        assert!(!c.contains("Example"));
    }
}
