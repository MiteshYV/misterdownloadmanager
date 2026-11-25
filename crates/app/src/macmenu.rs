//! macOS's menu bar: MDM's menus at the top of the screen, as every Mac app has
//! them, instead of a bar inside the window (`chrome::menu_h` is then 0). The same entries as
//! the in-window menus (`App::menu_entries`), the same commands; the app menu first (About,
//! Settings…, Services, Hide, Quit), as macOS lays it out.
//!
//! [`nodes`] turns the entries into a tree with a stable id for each item, its label as the
//! in-window menu shows it (translated, without the access key's `&` or a shortcut in
//! brackets) and its state; it is plain data, tested on every system. [`Bar`] (macOS) builds
//! the system's menus from it through muda, again only when their shape changed (a label, an
//! item added or gone), and otherwise only sets each item's enabled and checked state.

// Only macOS has a menu bar of the system's to fill; elsewhere the tests use the tree.
#![cfg_attr(not(target_os = "macos"), allow(dead_code))]

use crate::chrome::MenuEntry;

/// One entry of a menu, ready for the system's menu bar.
#[derive(Clone, Debug, PartialEq)]
pub enum Node<C> {
    /// An item: its id (`m<menu>.<index>[.<index>…]`), label, enabled, checked (`None`: not a
    /// check item; a radio item is a check item here, as macOS draws both with a tick).
    Item {
        id: String,
        label: String,
        enabled: bool,
        checked: Option<bool>,
        cmd: C,
    },
    /// A submenu: its label, enabled, children.
    Sub {
        label: String,
        enabled: bool,
        children: Vec<Node<C>>,
    },
    Sep,
}

/// A menu label as the in-window menu shows it, from its English source: translated, the
/// access key's `&` taken out, and a shortcut in brackets ("(Ctrl+F)") left to the menu's own
/// column.
pub fn label(source: &str) -> String {
    let (text, _) = crate::chrome::plain(crate::i18n::tc("menu", source));
    crate::chrome::without_shortcut(&text).to_owned()
}

/// Menu `menu`'s entries as a tree for the system's menu bar. A radio item is a check item; an
/// `Item` is one only while it is checked (an unchecked one can't be told from a plain item, in
/// the in-window menu either), so ticking it rebuilds the menus.
pub fn nodes<C: Copy>(menu: usize, entries: &[MenuEntry<C>]) -> Vec<Node<C>> {
    walk(&format!("m{menu}"), entries)
}

fn walk<C: Copy>(prefix: &str, entries: &[MenuEntry<C>]) -> Vec<Node<C>> {
    entries
        .iter()
        .enumerate()
        .map(|(i, e)| {
            let id = format!("{prefix}.{i}");
            match e {
                MenuEntry::Item(l, c, enabled, checked) => Node::Item {
                    id,
                    label: label(l),
                    enabled: *enabled,
                    checked: checked.then_some(true),
                    cmd: *c,
                },
                MenuEntry::Radio(l, c, enabled, on) => Node::Item {
                    id,
                    label: label(l),
                    enabled: *enabled,
                    checked: Some(*on),
                    cmd: *c,
                },
                MenuEntry::Sub(l, enabled, children) => Node::Sub {
                    label: label(l),
                    enabled: *enabled,
                    children: walk(&id, children),
                },
                MenuEntry::Sep => Node::Sep,
            }
        })
        .collect()
}

/// What the system's menus must be rebuilt for when it changes: every id, label, and whether
/// each item is a check item (a state change alone doesn't rebuild them).
pub fn shape<C>(menus: &[(String, Vec<Node<C>>)]) -> u64 {
    use std::hash::{Hash, Hasher};
    fn add<C>(h: &mut std::collections::hash_map::DefaultHasher, nodes: &[Node<C>]) {
        for n in nodes {
            match n {
                Node::Item {
                    id, label, checked, ..
                } => (0u8, id, label, checked.is_some()).hash(h),
                Node::Sub {
                    label, children, ..
                } => {
                    (1u8, label).hash(h);
                    add(h, children);
                    2u8.hash(h);
                }
                Node::Sep => 3u8.hash(h),
            }
        }
    }
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for (title, nodes) in menus {
        title.hash(&mut h);
        add(&mut h, nodes);
    }
    h.finish()
}

/// The ids of the app menu's own items, for the commands they run.
pub const APP_SETTINGS: &str = "app.settings";
pub const APP_ABOUT: &str = "app.about";
pub const APP_QUIT: &str = "app.quit";

#[cfg(target_os = "macos")]
pub use mac::Bar;

#[cfg(target_os = "macos")]
mod mac {
    use std::collections::HashMap;

    use muda::accelerator::{Accelerator, Code, Modifiers};
    use muda::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};

    use super::{APP_ABOUT, APP_QUIT, APP_SETTINGS, Node};

    /// An item MDM made, to set its state later.
    enum Made {
        Plain(MenuItem),
        Check(CheckMenuItem),
        Sub(Submenu),
    }

    /// The system's menu bar, made from MDM's menus.
    pub struct Bar<C> {
        menu: Option<Menu>,
        made: HashMap<String, Made>,
        cmds: HashMap<String, C>,
        app: [(String, C); 3],
        shape: u64,
    }

    impl<C: Copy> Bar<C> {
        /// `about`, `settings`, `quit`: the app menu's commands (About, Settings…, Quit).
        pub fn new(about: C, settings: C, quit: C) -> Self {
            Bar {
                menu: None,
                made: HashMap::new(),
                cmds: HashMap::new(),
                app: [
                    (APP_ABOUT.to_owned(), about),
                    (APP_SETTINGS.to_owned(), settings),
                    (APP_QUIT.to_owned(), quit),
                ],
                shape: 0,
            }
        }

        /// The menus as they are now (each its translated title and nodes): rebuilt when their
        /// shape changed, else only each item's state set. `wake` is called when an item is
        /// chosen, so the window takes it at its next frame.
        pub fn sync(
            &mut self,
            menus: &[(String, Vec<Node<C>>)],
            wake: impl Fn() + Send + Sync + 'static,
        ) {
            let shape = super::shape(menus);
            if self.menu.is_none() || shape != self.shape {
                self.build(menus);
                self.shape = shape;
                MenuEvent::set_event_handler(Some(std::sync::Arc::new(move |_| wake())));
                return;
            }
            for (_, nodes) in menus {
                self.update(nodes);
            }
        }

        /// The command of an item chosen since the last call, if any.
        pub fn chosen(&self) -> Option<C> {
            let event = MenuEvent::receiver().try_recv().ok()?;
            let id = event.id().0.as_str();
            self.app
                .iter()
                .find(|(app_id, _)| app_id == id)
                .map(|(_, c)| *c)
                .or_else(|| self.cmds.get(id).copied())
        }

        fn build(&mut self, menus: &[(String, Vec<Node<C>>)]) {
            self.made.clear();
            self.cmds.clear();
            let bar = Menu::new();
            let name = crate::MAIN_TITLE;
            let app = Submenu::new(name, true);
            let item = |id: &str, text: String, key: Option<Accelerator>| {
                MenuItem::with_id(id, text, true, key)
            };
            let _ = app.append_items(&[
                &item(
                    APP_ABOUT,
                    super::label("&About Mister Download Manager"),
                    None,
                ),
                &PredefinedMenuItem::separator(),
                &item(
                    APP_SETTINGS,
                    // i18n: macOS: the app menu's item that opens Options (every Mac app's Settings…)
                    crate::i18n::t("Settings…").to_owned(),
                    Some(Accelerator::new(Modifiers::META, Code::Comma)),
                ),
                &PredefinedMenuItem::separator(),
                &PredefinedMenuItem::services(None),
                &PredefinedMenuItem::separator(),
                &PredefinedMenuItem::hide(None),
                &PredefinedMenuItem::hide_others(None),
                &PredefinedMenuItem::show_all(None),
                &PredefinedMenuItem::separator(),
                &item(
                    APP_QUIT,
                    // i18n: macOS: the app menu's last item, which quits MDM as File → Exit does
                    crate::i18n::t("Quit Mister Download Manager").to_owned(),
                    Some(Accelerator::new(Modifiers::META, Code::KeyQ)),
                ),
            ]);
            let _ = bar.append(&app);
            for (title, nodes) in menus {
                let sub = Submenu::new(title, true);
                self.fill(&sub, nodes);
                let _ = bar.append(&sub);
            }
            bar.init_for_nsapp();
            self.menu = Some(bar);
        }

        fn fill(&mut self, into: &Submenu, nodes: &[Node<C>]) {
            for n in nodes {
                match n {
                    Node::Item {
                        id,
                        label,
                        enabled,
                        checked: None,
                        cmd,
                    } => {
                        let item = MenuItem::with_id(id.as_str(), label, *enabled, None);
                        let _ = into.append(&item);
                        self.made.insert(id.clone(), Made::Plain(item));
                        self.cmds.insert(id.clone(), *cmd);
                    }
                    Node::Item {
                        id,
                        label,
                        enabled,
                        checked: Some(on),
                        cmd,
                    } => {
                        let item = CheckMenuItem::with_id(id.as_str(), label, *enabled, *on, None);
                        let _ = into.append(&item);
                        self.made.insert(id.clone(), Made::Check(item));
                        self.cmds.insert(id.clone(), *cmd);
                    }
                    Node::Sub {
                        label,
                        enabled,
                        children,
                    } => {
                        let sub = Submenu::new(label, *enabled);
                        self.fill(&sub, children);
                        let _ = into.append(&sub);
                        self.made.insert(format!("sub:{label}"), Made::Sub(sub));
                    }
                    Node::Sep => {
                        let _ = into.append(&PredefinedMenuItem::separator());
                    }
                }
            }
        }

        fn update(&self, nodes: &[Node<C>]) {
            for n in nodes {
                match n {
                    Node::Item {
                        id,
                        enabled,
                        checked,
                        ..
                    } => match (self.made.get(id), checked) {
                        (Some(Made::Plain(item)), _) => item.set_enabled(*enabled),
                        (Some(Made::Check(item)), Some(on)) => {
                            item.set_enabled(*enabled);
                            item.set_checked(*on);
                        }
                        _ => {}
                    },
                    Node::Sub {
                        label,
                        enabled,
                        children,
                    } => {
                        if let Some(Made::Sub(sub)) = self.made.get(&format!("sub:{label}")) {
                            sub.set_enabled(*enabled);
                        }
                        self.update(children);
                    }
                    Node::Sep => {}
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Node, nodes, shape};
    use crate::chrome::MenuEntry;

    fn entries(checked: bool) -> Vec<MenuEntry<u8>> {
        vec![
            MenuEntry::Item("&Add new download", 1, true, false),
            MenuEntry::Sep,
            MenuEntry::Sub(
                "&Theme",
                true,
                vec![
                    MenuEntry::Radio("Classic", 2, true, !checked),
                    MenuEntry::Radio("Liquid Glass", 3, true, checked),
                ],
            ),
            MenuEntry::Item("E&xit", 4, false, false),
        ]
    }

    /// Each item keeps the in-window menu's command and state, its label without the access
    /// key, and an id from where it is, which a submenu's items carry on.
    #[test]
    fn menus_become_nodes_with_ids_from_their_place() {
        let n = nodes(1, &entries(true));
        assert_eq!(n.len(), 4);
        let Node::Item {
            id,
            label,
            enabled,
            checked,
            cmd,
        } = &n[0]
        else {
            panic!("{:?}", n[0]);
        };
        assert_eq!(
            (id.as_str(), label.as_str(), *enabled, *checked, *cmd),
            ("m1.0", "Add new download", true, None, 1)
        );
        assert_eq!(n[1], Node::Sep);
        let Node::Sub {
            label, children, ..
        } = &n[2]
        else {
            panic!("{:?}", n[2]);
        };
        assert_eq!(label, "Theme");
        let Node::Item { id, checked, .. } = &children[1] else {
            panic!();
        };
        assert_eq!((id.as_str(), *checked), ("m1.2.1", Some(true)));
        let Node::Item { label, enabled, .. } = &n[3] else {
            panic!();
        };
        assert_eq!((label.as_str(), *enabled), ("Exit", false));
    }

    /// A radio's tick moving, or an item greyed, doesn't rebuild the menus; a label that
    /// changes does.
    #[test]
    fn only_a_change_of_shape_rebuilds_the_menus() {
        let a = vec![("File".to_owned(), nodes(1, &entries(true)))];
        let b = vec![("File".to_owned(), nodes(1, &entries(false)))];
        assert_eq!(shape(&a), shape(&b));
        let mut changed = entries(true);
        changed[0] = MenuEntry::Item("&Add batch download", 1, true, false);
        let c = vec![("File".to_owned(), nodes(1, &changed))];
        assert_ne!(shape(&a), shape(&c));
    }
}
