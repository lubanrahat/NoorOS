//! Noor Shell command-line parser.

/// A parsed command line: the command name and the untouched argument text.
pub struct Command<'a> {
    pub name: &'a str,
    pub args: &'a str,
}

impl<'a> Command<'a> {
    pub fn is_empty(&self) -> bool {
        self.name.is_empty()
    }

    pub fn matches(&self, name: &str) -> bool {
        self.name.eq_ignore_ascii_case(name)
    }
}

/// Splits `line` into a command name and the rest of the line.
///
/// Arguments are not tokenised again so `echo` can reproduce the user's spacing.
pub fn parse<'a>(line: &'a str) -> Command<'a> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Command { name: "", args: "" };
    }
    // Rust 2024: `find` returns a byte offset, so slicing is always on a
    // character boundary because we search for an ASCII space.
    match trimmed.find(' ') {
        Some(index) => Command {
            name: &trimmed[..index],
            args: trimmed[index + 1..].trim_start(),
        },
        None => Command {
            name: trimmed,
            args: "",
        },
    }
}
