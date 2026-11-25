//! What a file is: by its first bytes, which a name can't change, and by its name where the
//! bytes don't say (text, scripts without a `#!`).

use std::io::Read;
use std::path::Path;

/// A file's kind, as far as quarantine cares.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    Video,
    Audio,
    Image,
    Pdf,
    Text,
    Zip,
    Rar,
    SevenZip,
    /// tar, gzip, xz, bzip2.
    Compressed,
    /// An ISO or another CD/DVD image.
    DiscImage,
    /// A macOS disk image (.dmg), which may hold an app.
    MacDiskImage,
    /// A Windows program (MZ: .exe, .dll, .scr…).
    WindowsProgram,
    /// A macOS program (Mach-O).
    MacProgram,
    /// A Linux program (ELF).
    LinuxProgram,
    /// A script: `#!`, or a script's extension (.sh, .command, .ps1, .bat, .vbs, .js…).
    Script,
    /// A macOS installer package (.pkg, xar).
    MacInstaller,
    /// A Windows installer (.msi) or another package that installs (.apk, .deb, .rpm, .jar).
    Installer,
    /// A macOS app (a `.app` folder).
    MacApp,
    /// A Windows shortcut (.lnk) or another file that points at a program.
    Shortcut,
    Other,
}

impl Kind {
    /// Runs something when opened: a program, script, installer, app or shortcut.
    pub fn runs(self) -> bool {
        matches!(
            self,
            Kind::WindowsProgram
                | Kind::MacProgram
                | Kind::LinuxProgram
                | Kind::Script
                | Kind::MacInstaller
                | Kind::Installer
                | Kind::MacApp
                | Kind::Shortcut
                | Kind::MacDiskImage
        )
    }

    /// Holds other files.
    pub fn archive(self) -> bool {
        matches!(
            self,
            Kind::Zip | Kind::Rar | Kind::SevenZip | Kind::Compressed
        )
    }

    /// A kind people open without thinking twice: media, pictures, documents, text.
    pub fn harmless(self) -> bool {
        matches!(
            self,
            Kind::Video | Kind::Audio | Kind::Image | Kind::Pdf | Kind::Text
        )
    }

    /// The kind in a few English words.
    pub fn noun(self) -> &'static str {
        match self {
            Kind::Video => "a video",
            Kind::Audio => "audio",
            Kind::Image => "a picture",
            Kind::Pdf => "a PDF",
            Kind::Text => "text",
            Kind::Zip => "a ZIP archive",
            Kind::Rar => "a RAR archive",
            Kind::SevenZip => "a 7z archive",
            Kind::Compressed => "a compressed file",
            Kind::DiscImage => "a disc image",
            Kind::MacDiskImage => "a Mac disk image",
            Kind::WindowsProgram => "a Windows program",
            Kind::MacProgram => "a Mac program",
            Kind::LinuxProgram => "a Linux program",
            Kind::Script => "a script",
            Kind::MacInstaller => "a Mac installer",
            Kind::Installer => "an installer",
            Kind::MacApp => "a Mac app",
            Kind::Shortcut => "a shortcut to a program",
            Kind::Other => "a file",
        }
    }
}

/// The extensions of files that run something, by kind.
const RUNS: &[(&str, Kind)] = &[
    ("exe", Kind::WindowsProgram),
    ("scr", Kind::WindowsProgram),
    ("com", Kind::WindowsProgram),
    ("pif", Kind::WindowsProgram),
    ("dll", Kind::WindowsProgram),
    ("cpl", Kind::WindowsProgram),
    ("bat", Kind::Script),
    ("cmd", Kind::Script),
    ("vbs", Kind::Script),
    ("vbe", Kind::Script),
    ("js", Kind::Script),
    ("jse", Kind::Script),
    ("wsf", Kind::Script),
    ("hta", Kind::Script),
    ("ps1", Kind::Script),
    ("sh", Kind::Script),
    ("command", Kind::Script),
    ("applescript", Kind::Script),
    ("scpt", Kind::Script),
    ("py", Kind::Script),
    ("msi", Kind::Installer),
    ("msix", Kind::Installer),
    ("appx", Kind::Installer),
    ("apk", Kind::Installer),
    ("deb", Kind::Installer),
    ("rpm", Kind::Installer),
    ("jar", Kind::Installer),
    ("pkg", Kind::MacInstaller),
    ("mpkg", Kind::MacInstaller),
    ("dmg", Kind::MacDiskImage),
    ("app", Kind::MacApp),
    ("lnk", Kind::Shortcut),
    ("url", Kind::Shortcut),
    ("webloc", Kind::Shortcut),
];

/// Harmless kinds by extension.
const NAMED: &[(&[&str], Kind)] = &[
    (
        &[
            "mp4", "m4v", "mkv", "avi", "mov", "wmv", "webm", "flv", "mpg", "mpeg", "ts", "m2ts",
            "3gp",
        ],
        Kind::Video,
    ),
    (
        &[
            "mp3", "m4a", "aac", "flac", "wav", "ogg", "opus", "wma", "alac", "aiff",
        ],
        Kind::Audio,
    ),
    (
        &[
            "jpg", "jpeg", "png", "gif", "webp", "bmp", "tif", "tiff", "heic", "svg",
        ],
        Kind::Image,
    ),
    (&["pdf"], Kind::Pdf),
    (
        &[
            "txt", "srt", "ass", "ssa", "vtt", "sub", "nfo", "md", "csv", "json", "xml", "log",
        ],
        Kind::Text,
    ),
    (&["zip"], Kind::Zip),
    (&["rar"], Kind::Rar),
    (&["7z"], Kind::SevenZip),
    (&["tar", "gz", "tgz", "xz", "bz2", "zst"], Kind::Compressed),
    (&["iso", "img", "bin", "cue"], Kind::DiscImage),
];

fn ext(name: &str) -> Option<String> {
    let (stem, e) = name.rsplit_once('.')?;
    (!stem.is_empty() && !e.is_empty()).then(|| e.to_ascii_lowercase())
}

/// What a file's name says it is.
pub fn of_name(name: &str) -> Kind {
    let Some(e) = ext(name) else {
        return Kind::Other;
    };
    if let Some((_, k)) = RUNS.iter().find(|(x, _)| *x == e) {
        return *k;
    }
    NAMED
        .iter()
        .find(|(exts, _)| exts.contains(&e.as_str()))
        .map_or(Kind::Other, |(_, k)| *k)
}

/// A name that ends in a program's extension right after a harmless one (`film.mp4.exe`,
/// `photo.jpg .scr`), or hides it behind spaces.
pub fn double_extension(name: &str) -> bool {
    let Some((stem, _)) = name.rsplit_once('.') else {
        return false;
    };
    of_name(name).runs() && of_name(stem.trim_end()).harmless()
}

/// What a file is: its first bytes first, then its name.
pub fn of_file(path: &Path, name: &str) -> Kind {
    let mut head = [0u8; 4096];
    let n = std::fs::File::open(path)
        .and_then(|mut f| {
            let mut n = 0;
            while n < head.len() {
                let got = f.read(&mut head[n..])?;
                if got == 0 {
                    break;
                }
                n += got;
            }
            Ok(n)
        })
        .unwrap_or(0);
    let by_name = of_name(name);
    match of_bytes(&head[..n]) {
        // A ZIP that installs (an .apk, a .jar, an .msix) stays one.
        Some(Kind::Zip) if by_name == Kind::Installer => Kind::Installer,
        // Microsoft's compound file is an .msi, and a Word or Excel file too.
        Some(Kind::Other) => by_name,
        Some(k) => k,
        None if by_name == Kind::MacDiskImage || name.to_ascii_lowercase().ends_with(".dmg") => {
            Kind::MacDiskImage
        }
        None => by_name,
    }
}

/// The kind the first bytes say, if they say one.
pub fn of_bytes(head: &[u8]) -> Option<Kind> {
    let at = |i: usize, sig: &[u8]| head.get(i..i + sig.len()) == Some(sig);
    if at(0, b"MZ") {
        return Some(Kind::WindowsProgram);
    }
    if at(0, b"\x7fELF") {
        return Some(Kind::LinuxProgram);
    }
    // Mach-O, 32 and 64 bits, both byte orders; a universal binary (CA FE BA BE, which Java
    // classes share: those are named .class and found by name).
    if [
        &[0xFE, 0xED, 0xFA, 0xCE][..],
        &[0xFE, 0xED, 0xFA, 0xCF],
        &[0xCE, 0xFA, 0xED, 0xFE],
        &[0xCF, 0xFA, 0xED, 0xFE],
        &[0xCA, 0xFE, 0xBA, 0xBE],
    ]
    .iter()
    .any(|sig| at(0, sig))
    {
        return Some(Kind::MacProgram);
    }
    if at(0, b"#!") {
        return Some(Kind::Script);
    }
    if at(0, b"xar!") {
        return Some(Kind::MacInstaller);
    }
    if at(0, b"PK\x03\x04") || at(0, b"PK\x05\x06") {
        return Some(Kind::Zip);
    }
    if at(0, b"Rar!\x1a\x07") {
        return Some(Kind::Rar);
    }
    if at(0, &[b'7', b'z', 0xBC, 0xAF, 0x27, 0x1C]) {
        return Some(Kind::SevenZip);
    }
    if at(0, &[0x1F, 0x8B]) || at(0, &[0xFD, b'7', b'z', b'X', b'Z', 0]) || at(0, b"BZh") {
        return Some(Kind::Compressed);
    }
    if at(0, b"%PDF") {
        return Some(Kind::Pdf);
    }
    if at(0, &[0xFF, 0xD8, 0xFF])
        || at(0, b"\x89PNG")
        || at(0, b"GIF8")
        || (at(0, b"RIFF") && at(8, b"WEBP"))
    {
        return Some(Kind::Image);
    }
    // (ISO 9660's "CD001" is at 0x8001, past the 4 KB read: an .iso is known by its name.)
    if at(0, &[0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1]) {
        // Compound file: an .msi, or an Office document: the name tells.
        return Some(Kind::Other);
    }
    if at(0, &[0x4C, 0x00, 0x00, 0x00, 0x01, 0x14, 0x02, 0x00]) {
        return Some(Kind::Shortcut);
    }
    match mdm_sniff(head) {
        Some("mp3" | "m4a" | "flac" | "ogg" | "wav" | "aac") => Some(Kind::Audio),
        Some(_) => Some(Kind::Video),
        None => None,
    }
}

/// The media kinds the engine knows by their bytes (`mdm_engine::naming::sniff`'s), kept here
/// without the engine: MP4/MOV, Matroska/WebM, Ogg, FLAC, MP3 with ID3, WAV, AVI.
fn mdm_sniff(head: &[u8]) -> Option<&'static str> {
    let at = |i: usize, sig: &[u8]| head.get(i..i + sig.len()) == Some(sig);
    if at(4, b"ftyp") {
        return Some(match head.get(8..12).unwrap_or_default() {
            b"M4A " | b"M4B " => "m4a",
            _ => "mp4",
        });
    }
    if at(0, &[0x1A, 0x45, 0xDF, 0xA3]) {
        return Some("mkv");
    }
    if at(0, b"OggS") {
        return Some("ogg");
    }
    if at(0, b"fLaC") {
        return Some("flac");
    }
    if at(0, b"ID3") {
        return Some("mp3");
    }
    if at(0, b"RIFF") && at(8, b"WAVE") {
        return Some("wav");
    }
    if at(0, b"RIFF") && at(8, b"AVI ") {
        return Some("avi");
    }
    None
}
