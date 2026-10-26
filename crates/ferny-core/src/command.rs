//! Command registry (`docs/DESIGN.md` §6).
//!
//! Every user action is a command with a stable id (`file.open`,
//! `edit.duplicate_line`, …). Menus, toolbar, keybindings, the command
//! palette and plugins all invoke commands through this registry, so they
//! share one list of actions and one place to check them.
//!
//! The core never produces user-facing prose: titles are English source
//! strings that the UI translates. A command is handled either in the core
//! (a Rust handler) or in the UI (the registry reports that the UI must
//! run it, e.g. to show a dialog).

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

/// Maximum length of a command or menu id, in bytes.
pub const MAX_ID_LEN: usize = 64;
/// Maximum length of a title, in bytes.
pub const MAX_TITLE_LEN: usize = 128;
/// Maximum length of one key sequence, in bytes.
pub const MAX_KEY_LEN: usize = 32;
/// Maximum number of default key sequences per command.
pub const MAX_KEYS_PER_COMMAND: usize = 4;

/// Why a registration or invocation was refused. Carries no user data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum CommandError {
    /// The id is not of the form `segment.segment[...]` (lowercase ASCII
    /// letters, digits and `_`, each segment starting with a letter).
    #[error("invalid command or menu id")]
    InvalidId,
    /// The title is empty, too long or contains control characters.
    #[error("invalid title")]
    InvalidTitle,
    /// A default key sequence is malformed, or there are too many.
    #[error("invalid key sequence")]
    InvalidKeys,
    /// The id is already registered.
    #[error("duplicate id")]
    Duplicate,
    /// No command or menu has this id.
    #[error("unknown id")]
    Unknown,
}

/// Validated identifier of a command or menu.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CommandId(String);

impl CommandId {
    /// Validates `id`. Commands need at least two segments (`file.open`);
    /// menu ids use [`CommandId::parse_menu`], which allows one.
    pub fn parse(id: &str) -> Result<Self, CommandError> {
        Self::parse_with_min_segments(id, 2)
    }

    /// Validates a menu id (one or more segments, e.g. `file`).
    pub fn parse_menu(id: &str) -> Result<Self, CommandError> {
        Self::parse_with_min_segments(id, 1)
    }

    fn parse_with_min_segments(id: &str, min_segments: usize) -> Result<Self, CommandError> {
        if id.is_empty() || id.len() > MAX_ID_LEN {
            return Err(CommandError::InvalidId);
        }
        let mut segments = 0;
        for segment in id.split('.') {
            let mut chars = segment.chars();
            match chars.next() {
                Some(c) if c.is_ascii_lowercase() => {}
                _ => return Err(CommandError::InvalidId),
            }
            if !chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_') {
                return Err(CommandError::InvalidId);
            }
            segments += 1;
        }
        if segments < min_segments {
            return Err(CommandError::InvalidId);
        }
        Ok(Self(id.to_owned()))
    }

    /// The id as a string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for CommandId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A handler that runs in the core.
pub type CoreHandler = Arc<dyn Fn() + Send + Sync>;

/// Where a command runs.
#[derive(Clone)]
pub enum Handler {
    /// The UI runs it (dialogs, window management).
    Ui,
    /// The core runs it.
    Core(CoreHandler),
}

impl fmt::Debug for Handler {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Ui => f.write_str("Ui"),
            Self::Core(_) => f.write_str("Core(..)"),
        }
    }
}

/// What the caller must do after [`Registry::invoke`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dispatch {
    /// The core ran the command; nothing else to do.
    Done,
    /// The UI must run the command.
    Ui,
}

/// A registered command.
#[derive(Debug, Clone)]
pub struct Command {
    id: CommandId,
    title: String,
    default_keys: Vec<String>,
    handler: Handler,
}

impl Command {
    /// Creates a command, validating every field.
    ///
    /// `title` is an English source string with an optional `&` mnemonic
    /// (`"&Open…"`); `default_keys` use Qt's portable notation (`"Ctrl+O"`,
    /// where `Ctrl` is Command on macOS).
    pub fn new(
        id: &str,
        title: &str,
        default_keys: &[&str],
        handler: Handler,
    ) -> Result<Self, CommandError> {
        Ok(Self {
            id: CommandId::parse(id)?,
            title: validate_title(title)?,
            default_keys: validate_keys(default_keys)?,
            handler,
        })
    }

    /// The command id.
    pub fn id(&self) -> &CommandId {
        &self.id
    }

    /// The untranslated title.
    pub fn title(&self) -> &str {
        &self.title
    }

    /// Default key sequences, in priority order.
    pub fn default_keys(&self) -> &[String] {
        &self.default_keys
    }
}

/// An entry of a menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MenuItem {
    /// A registered command.
    Command(CommandId),
    /// A separator line.
    Separator,
}

/// A top-level menu, declared as data and rendered by the UI.
#[derive(Debug, Clone)]
pub struct Menu {
    id: CommandId,
    title: String,
    items: Vec<MenuItem>,
}

impl Menu {
    /// The menu id (e.g. `file`).
    pub fn id(&self) -> &CommandId {
        &self.id
    }

    /// The untranslated title, with `&` mnemonic.
    pub fn title(&self) -> &str {
        &self.title
    }

    /// The entries, in display order.
    pub fn items(&self) -> &[MenuItem] {
        &self.items
    }
}

/// The set of commands and menus.
#[derive(Debug, Default)]
pub struct Registry {
    commands: BTreeMap<CommandId, Command>,
    menus: Vec<Menu>,
}

impl Registry {
    /// An empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a command; ids are unique.
    pub fn register(&mut self, command: Command) -> Result<(), CommandError> {
        if self.commands.contains_key(&command.id) {
            return Err(CommandError::Duplicate);
        }
        self.commands.insert(command.id.clone(), command);
        Ok(())
    }

    /// Adds a top-level menu after the existing ones. Every command in it
    /// must already be registered; menu ids are unique.
    pub fn add_menu(
        &mut self,
        id: &str,
        title: &str,
        items: &[Option<&str>],
    ) -> Result<(), CommandError> {
        let id = CommandId::parse_menu(id)?;
        if self.menus.iter().any(|menu| menu.id == id) {
            return Err(CommandError::Duplicate);
        }
        let title = validate_title(title)?;
        let items = items
            .iter()
            .map(|item| match item {
                None => Ok(MenuItem::Separator),
                Some(command) => {
                    let command = CommandId::parse(command)?;
                    if self.commands.contains_key(&command) {
                        Ok(MenuItem::Command(command))
                    } else {
                        Err(CommandError::Unknown)
                    }
                }
            })
            .collect::<Result<Vec<_>, _>>()?;
        self.menus.push(Menu { id, title, items });
        Ok(())
    }

    /// Looks up a command by id string.
    pub fn get(&self, id: &str) -> Option<&Command> {
        let id = CommandId::parse(id).ok()?;
        self.commands.get(&id)
    }

    /// All commands, ordered by id.
    pub fn commands(&self) -> impl Iterator<Item = &Command> {
        self.commands.values()
    }

    /// Top-level menus, in display order.
    pub fn menus(&self) -> &[Menu] {
        &self.menus
    }

    /// Looks up a menu by id string.
    pub fn menu(&self, id: &str) -> Option<&Menu> {
        self.menus.iter().find(|menu| menu.id.as_str() == id)
    }

    /// Runs the command `id`: core handlers run here; UI commands are
    /// reported back with [`Dispatch::Ui`]. Unknown or malformed ids fail
    /// closed with [`CommandError::Unknown`].
    pub fn invoke(&self, id: &str) -> Result<Dispatch, CommandError> {
        let command = self.get(id).ok_or(CommandError::Unknown)?;
        match &command.handler {
            Handler::Ui => Ok(Dispatch::Ui),
            Handler::Core(handler) => {
                handler();
                Ok(Dispatch::Done)
            }
        }
    }
}

/// The commands and menus built into Ferny (milestones H0.5, H1.1, H1.2, H1.3b, H1.4).
pub fn builtin_registry() -> Registry {
    let mut registry = Registry::new();
    let commands: [(&str, &str, &[&str]); 85] = [
        ("file.new", "&New", &["Ctrl+N"]),
        ("file.open", "&Open…", &["Ctrl+O"]),
        ("file.new_from_template", "New from &Template…", &[]),
        ("file.save", "&Save", &["Ctrl+S"]),
        ("file.save_all", "Save &All", &["Ctrl+Alt+S"]),
        ("file.save_as", "Save &As…", &["Ctrl+Shift+S"]),
        ("file.print", "&Print…", &["Ctrl+P"]),
        ("file.export_pdf", "Export as P&DF…", &[]),
        ("file.export_html", "Export as &HTML…", &[]),
        ("file.export_latex", "Export as &LaTeX…", &[]),
        ("file.reload", "&Reload", &[]),
        ("file.reload_with_encoding", "Reload &With Encoding…", &[]),
        ("file.close", "&Close", &["Ctrl+W"]),
        ("file.close_others", "Close &Other Documents", &[]),
        ("file.close_all", "C&lose All", &["Ctrl+Shift+W"]),
        (
            "file.reopen_closed",
            "Reopen Closed &Tab",
            &["Ctrl+Shift+T"],
        ),
        ("file.import_geany", "&Import Geany Settings…", &[]),
        ("app.quit", "&Quit", &["Ctrl+Q"]),
        ("edit.undo", "&Undo", &["Ctrl+Z"]),
        ("edit.redo", "&Redo", &["Ctrl+Shift+Z", "Ctrl+Y"]),
        ("edit.cut", "Cu&t", &["Ctrl+X"]),
        ("edit.copy", "&Copy", &["Ctrl+C"]),
        ("edit.paste", "&Paste", &["Ctrl+V"]),
        ("edit.select_all", "Select &All", &["Ctrl+A"]),
        (
            "edit.duplicate_line",
            "D&uplicate Line or Selection",
            &["Ctrl+D"],
        ),
        ("edit.delete_line", "D&elete Line", &["Ctrl+K"]),
        ("edit.move_line_up", "Move Line U&p", &["Alt+Up"]),
        ("edit.move_line_down", "Move Line Dow&n", &["Alt+Down"]),
        (
            "edit.toggle_comment",
            "Toggle Line &Commentation",
            &["Ctrl+E"],
        ),
        ("edit.toggle_marker", "Toggle &Marker", &["Ctrl+M"]),
        (
            "edit.mark_occurrences",
            "Mark &Occurrences",
            &["Ctrl+Shift+M"],
        ),
        ("edit.next_marker", "Go to Ne&xt Marker", &["Ctrl+."]),
        (
            "edit.previous_marker",
            "Go to Pre&vious Marker",
            &["Ctrl+,"],
        ),
        ("app.preferences", "Pre&ferences…", &["Ctrl+Alt+P"]),
        ("app.keybindings", "&Keyboard Shortcuts…", &[]),
        ("search.find", "&Find…", &["Ctrl+F"]),
        ("search.find_next", "Find &Next", &["Ctrl+G", "F3"]),
        (
            "search.find_previous",
            "Find &Previous",
            &["Ctrl+Shift+G", "Shift+F3"],
        ),
        ("search.replace", "&Replace…", &["Ctrl+H"]),
        ("search.replace_all", "Replace &All", &[]),
        ("search.find_in_files", "Find in F&iles…", &["Ctrl+Shift+F"]),
        ("search.goto_line", "&Go to Line…", &["Ctrl+L"]),
        ("search.goto_symbol", "Go to S&ymbol…", &[]),
        ("search.goto_definition", "Go to &Definition", &["Ctrl+T"]),
        ("search.find_usages", "Find &Usages", &["Ctrl+Shift+E"]),
        ("search.rename_symbol", "Re&name Symbol…", &["F2"]),
        (
            "search.goto_workspace_symbol",
            "Go to &Workspace Symbol…",
            &["Ctrl+Alt+T"],
        ),
        ("view.navigate_back", "Navigate &Back", &["Alt+Left"]),
        ("view.navigate_forward", "Navigate &Forward", &["Alt+Right"]),
        ("edit.complete_word", "Complete &Word", &["Ctrl+Space"]),
        (
            "app.command_palette",
            "Command &Palette…",
            &["Ctrl+Shift+P"],
        ),
        ("view.next_tab", "&Next Tab", &["Ctrl+PgDown"]),
        ("view.previous_tab", "&Previous Tab", &["Ctrl+PgUp"]),
        ("view.show_invisible", "Show &Invisible Characters", &[]),
        ("view.auto_close", "Auto-Close &Brackets", &[]),
        ("view.word_wrap", "&Word Wrap", &[]),
        ("view.indent_guides", "&Indentation Guides", &[]),
        ("view.zoom_in", "Zoom &In", &[]),
        ("view.zoom_out", "Zoom &Out", &[]),
        ("view.zoom_reset", "&Normal Size", &[]),
        ("view.sidebar", "Show &Sidebar", &[]),
        ("view.message_window", "Show &Message Window", &[]),
        ("view.markdown_preview", "Mark&down Preview", &[]),
        ("view.spell_check", "Check &Spelling", &[]),
        ("view.split_side", "Split Side by &Side", &[]),
        ("view.split_stacked", "Split Top and &Bottom", &[]),
        (
            "view.split_current",
            "Split View Shows &Current Document",
            &[],
        ),
        ("view.unsplit", "&Unsplit", &[]),
        ("document.set_encoding", "Set &Encoding…", &[]),
        ("document.trust_folder", "&Trust Folder…", &[]),
        ("document.git_blame", "Git &Blame", &[]),
        ("document.format", "F&ormat Document", &[]),
        ("project.new", "&New Project…", &[]),
        ("project.open", "&Open Project…", &[]),
        ("project.close", "&Close Project", &[]),
        ("project.edit", "&Edit Project File", &[]),
        ("project.import_geany", "&Import Geany Project…", &[]),
        ("build.compile", "&Compile", &["F8"]),
        ("build.build", "&Build", &["F9"]),
        ("build.lint", "&Lint", &[]),
        ("build.make", "&Make", &["Shift+F9"]),
        ("build.execute", "&Execute", &["F5"]),
        ("build.stop", "&Stop", &[]),
        ("build.set_commands", "Set Build &Commands…", &[]),
        ("help.about", "&About Ferny", &[]),
    ];
    for (id, title, keys) in commands {
        // macOS reserves Cmd+H (hide) and Cmd+M (minimise): no default
        // there (H1.5a). Option+arrows move by word and Cmd+Space is
        // Spotlight, so navigation and completion differ there (H2.3).
        // In Qt's portable text "Ctrl" is Cmd and "Meta" is Control on macOS.
        let keys: &[&str] = if cfg!(target_os = "macos") {
            match id {
                "edit.toggle_marker" => &[],
                "view.navigate_back" => &["Ctrl+Meta+Left"],
                "view.navigate_forward" => &["Ctrl+Meta+Right"],
                "edit.complete_word" => &["Alt+Esc"],
                _ => keys,
            }
        } else {
            keys
        };
        let command = Command::new(id, title, keys, Handler::Ui)
            .unwrap_or_else(|err| unreachable!("built-in command {id}: {err}"));
        registry
            .register(command)
            .unwrap_or_else(|err| unreachable!("built-in command {id}: {err}"));
    }
    let menus: [(&str, &str, &[Option<&str>]); 8] = [
        (
            "file",
            "&File",
            &[
                Some("file.new"),
                Some("file.new_from_template"),
                Some("file.open"),
                None,
                Some("file.save"),
                Some("file.save_all"),
                Some("file.save_as"),
                None,
                Some("file.print"),
                Some("file.export_pdf"),
                Some("file.export_html"),
                Some("file.export_latex"),
                None,
                Some("file.reload"),
                Some("file.reload_with_encoding"),
                None,
                Some("file.close"),
                Some("file.close_others"),
                Some("file.close_all"),
                Some("file.reopen_closed"),
                None,
                Some("file.import_geany"),
                None,
                Some("app.quit"),
            ],
        ),
        (
            "edit",
            "&Edit",
            &[
                Some("edit.undo"),
                Some("edit.redo"),
                None,
                Some("edit.cut"),
                Some("edit.copy"),
                Some("edit.paste"),
                Some("edit.select_all"),
                None,
                Some("edit.duplicate_line"),
                Some("edit.delete_line"),
                Some("edit.move_line_up"),
                Some("edit.move_line_down"),
                Some("edit.toggle_comment"),
                Some("edit.complete_word"),
                None,
                Some("edit.toggle_marker"),
                Some("edit.mark_occurrences"),
                Some("edit.next_marker"),
                Some("edit.previous_marker"),
                None,
                Some("app.preferences"),
                Some("app.keybindings"),
            ],
        ),
        (
            "search",
            "&Search",
            &[
                Some("search.find"),
                Some("search.find_next"),
                Some("search.find_previous"),
                None,
                Some("search.replace"),
                Some("search.replace_all"),
                None,
                Some("search.find_in_files"),
                None,
                Some("search.goto_line"),
                Some("search.goto_symbol"),
                Some("search.goto_definition"),
                Some("search.find_usages"),
                Some("search.rename_symbol"),
                Some("search.goto_workspace_symbol"),
            ],
        ),
        (
            "view",
            "&View",
            &[
                Some("app.command_palette"),
                None,
                Some("view.navigate_back"),
                Some("view.navigate_forward"),
                None,
                Some("view.next_tab"),
                Some("view.previous_tab"),
                None,
                Some("view.show_invisible"),
                Some("view.auto_close"),
                Some("view.word_wrap"),
                Some("view.indent_guides"),
                None,
                Some("view.zoom_in"),
                Some("view.zoom_out"),
                Some("view.zoom_reset"),
                None,
                Some("view.sidebar"),
                Some("view.message_window"),
                Some("view.markdown_preview"),
                Some("view.spell_check"),
                None,
                Some("view.split_side"),
                Some("view.split_stacked"),
                Some("view.split_current"),
                Some("view.unsplit"),
            ],
        ),
        (
            "document",
            "&Document",
            &[
                Some("document.set_encoding"),
                Some("document.format"),
                None,
                Some("document.trust_folder"),
                Some("document.git_blame"),
            ],
        ),
        (
            "project",
            "&Project",
            &[
                Some("project.new"),
                Some("project.open"),
                Some("project.close"),
                None,
                Some("project.edit"),
                None,
                Some("project.import_geany"),
            ],
        ),
        (
            "build",
            "&Build",
            &[
                Some("build.compile"),
                Some("build.build"),
                Some("build.lint"),
                Some("build.make"),
                None,
                Some("build.execute"),
                Some("build.stop"),
                None,
                Some("build.set_commands"),
            ],
        ),
        ("help", "&Help", &[Some("help.about")]),
    ];
    for (id, title, items) in menus {
        registry
            .add_menu(id, title, items)
            .unwrap_or_else(|err| unreachable!("built-in menu {id}: {err}"));
    }
    registry
}

fn validate_title(title: &str) -> Result<String, CommandError> {
    if title.trim().is_empty() || title.len() > MAX_TITLE_LEN || title.chars().any(char::is_control)
    {
        return Err(CommandError::InvalidTitle);
    }
    Ok(title.to_owned())
}

fn validate_keys(keys: &[&str]) -> Result<Vec<String>, CommandError> {
    if keys.len() > MAX_KEYS_PER_COMMAND {
        return Err(CommandError::InvalidKeys);
    }
    keys.iter()
        .map(|key| {
            if is_valid_key_sequence(key) {
                Ok((*key).to_owned())
            } else {
                Err(CommandError::InvalidKeys)
            }
        })
        .collect()
}

/// Accepts Qt's portable notation: up to four chords separated by `", "`,
/// each chord being zero or more of `Ctrl+`, `Shift+`, `Alt+`, `Meta+`
/// followed by one key — a single printable ASCII character other than
/// space, or a named key such as `F5`, `Home` or `PgUp`.
pub fn is_valid_key_sequence(sequence: &str) -> bool {
    if sequence.is_empty() || sequence.len() > MAX_KEY_LEN {
        return false;
    }
    let chords: Vec<&str> = sequence.split(", ").collect();
    chords.len() <= 4 && chords.iter().all(|chord| is_valid_chord(chord))
}

fn is_valid_chord(chord: &str) -> bool {
    const MODIFIERS: [&str; 4] = ["Ctrl", "Shift", "Alt", "Meta"];
    let mut rest = chord;
    while let Some(modifier) = MODIFIERS.iter().find(|m| {
        rest.len() > m.len() + 1 && rest.starts_with(*m) && rest[m.len()..].starts_with('+')
    }) {
        rest = &rest[modifier.len() + 1..];
    }
    let mut chars = rest.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) => c.is_ascii_graphic(),
        (Some(c), Some(_)) => {
            c.is_ascii_alphabetic() && rest.chars().all(|c| c.is_ascii_alphanumeric())
        }
        (None, _) => false,
    }
}
