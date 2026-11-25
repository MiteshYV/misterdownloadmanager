//! macOS: whether an app or installer is signed by an identified developer, as Gatekeeper
//! asks before it first opens: `spctl --assess` (Gatekeeper's own decision: signed, by a
//! Developer ID, notarized), `pkgutil --check-signature` for an installer. Elsewhere nothing is
//! checked (an app or installer is already a concern of its own, `Concern::Runs`).

use std::path::Path;

use crate::Kind;

/// Why `path` (an app or a Mac installer) isn't signed as Gatekeeper wants, or None when it is
/// (or this isn't a Mac).
pub fn unsigned(path: &Path, kind: Kind) -> Option<String> {
    if !cfg!(target_os = "macos") {
        return None;
    }
    let run = |program: &str, args: &[&str]| {
        std::process::Command::new(program)
            .args(args)
            .arg(path)
            .stdin(std::process::Stdio::null())
            .output()
            .ok()
    };
    match kind {
        Kind::MacInstaller => {
            let o = run("/usr/sbin/pkgutil", &["--check-signature"])?;
            pkgutil(&String::from_utf8_lossy(&o.stdout))
                .or_else(|| (!o.status.success()).then(|| "no valid signature".to_owned()))
        }
        _ => {
            let o = run("/usr/sbin/spctl", &["--assess", "--type", "execute", "-vv"])?;
            (!o.status.success()).then(|| spctl(&String::from_utf8_lossy(&o.stderr)))
        }
    }
}

/// `pkgutil --check-signature`'s verdict: None when it is signed and trusted.
pub(crate) fn pkgutil(out: &str) -> Option<String> {
    let status = out
        .lines()
        .find_map(|l| l.trim().strip_prefix("Status: "))
        .unwrap_or("");
    if status.starts_with("signed by a developer certificate issued by Apple")
        || status.starts_with("signed by a certificate trusted by")
        || status.starts_with("signed Apple Software")
    {
        None
    } else if status.is_empty() {
        Some("no signature".into())
    } else {
        Some(status.to_owned())
    }
}

/// `spctl --assess -vv`'s reason (its first line after the path), for a rejection.
pub(crate) fn spctl(err: &str) -> String {
    err.lines()
        .filter_map(|l| l.split_once(": ").map(|(_, why)| why.trim()))
        .find(|why| !why.is_empty())
        .or_else(|| err.lines().map(str::trim).find(|l| !l.is_empty()))
        .unwrap_or("rejected by Gatekeeper")
        .to_owned()
}

#[cfg(test)]
mod tests {
    /// pkgutil's statuses: signed and trusted, or not.
    #[test]
    fn installer_signatures_are_read() {
        let signed = "Package \"x.pkg\":\n   Status: signed by a developer certificate issued by Apple for distribution\n   Notarization: trusted by the Apple notary service\n";
        assert_eq!(super::pkgutil(signed), None);
        assert_eq!(
            super::pkgutil("Package \"x.pkg\":\n   Status: no signature\n").as_deref(),
            Some("no signature")
        );
        assert_eq!(super::pkgutil("").as_deref(), Some("no signature"));
    }

    /// spctl's reason, without the path before it.
    #[test]
    fn gatekeepers_reason_is_read() {
        assert_eq!(
            super::spctl("/q/Codec.app: rejected\nsource=no usable signature\n"),
            "rejected"
        );
        assert_eq!(
            super::spctl("source=Unnotarized Developer ID\n"),
            "source=Unnotarized Developer ID"
        );
        assert_eq!(super::spctl(""), "rejected by Gatekeeper");
    }
}
