use crate::Key;

/// Key sequences bound to command names, as a tree: each key leads to a command or to the keys
/// that may come next.
///
/// A sequence is bound to a command or starts longer sequences, never both, so dispatch never
/// waits to see whether another key comes (ADR 0033). A binding replaces whatever its keys
/// overlap, which is how user config overrides a preset.
#[derive(Clone, Debug, Default)]
pub struct Keymap {
    root: Prefix,
}

impl Keymap {
    /// A keymap with nothing bound.
    pub fn new() -> Self {
        Self::default()
    }

    /// Binds `keys` to `command`, replacing whatever they overlap: their own binding, the longer
    /// sequences they start, and a command bound to a shorter sequence they start with, whose
    /// keys become a prefix.
    ///
    /// # Panics
    ///
    /// Panics if `keys` is empty.
    pub fn bind(&mut self, keys: &[Key], command: impl Into<String>) {
        let Some((&last, path)) = keys.split_last() else {
            panic!("an empty key sequence can't be bound");
        };
        let mut prefix = &mut self.root;
        for &key in path {
            let node = prefix.node_mut(key);
            if let Node::Command(_) = node {
                *node = Node::Prefix(Prefix::default());
            }
            let Node::Prefix(next) = node else {
                unreachable!("a command on the way was replaced by a prefix");
            };
            prefix = next;
        }
        *prefix.node_mut(last) = Node::Command(command.into());
    }

    /// What `keys` are bound to, or `None` if no binding starts with them. The empty sequence
    /// starts every binding.
    pub fn get(&self, keys: &[Key]) -> Option<Binding<'_>> {
        let mut binding = Binding::Prefix(&self.root);
        for &key in keys {
            // A command ends its sequence: nothing is bound past it.
            let Binding::Prefix(prefix) = binding else {
                return None;
            };
            binding = prefix.get(key)?;
        }
        Some(binding)
    }
}

/// What a key sequence is bound to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Binding<'a> {
    /// The sequence runs the command with this name.
    Command(&'a str),
    /// The sequence starts longer ones; the prefix holds the keys that may come next.
    Prefix(&'a Prefix),
}

/// The keys that may come next after a sequence that starts longer ones.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Prefix {
    /// In the order the keys were first bound.
    bindings: Vec<(Key, Node)>,
}

impl Prefix {
    /// The keys that may come next, each with what it is bound to, in the order they were first
    /// bound: pending-key hints list them so. Rebinding a key keeps its place.
    pub fn bindings(&self) -> impl Iterator<Item = (Key, Binding<'_>)> {
        self.bindings
            .iter()
            .map(|(key, node)| (*key, node.binding()))
    }

    fn get(&self, key: Key) -> Option<Binding<'_>> {
        self.bindings
            .iter()
            .find(|(bound, _)| *bound == key)
            .map(|(_, node)| node.binding())
    }

    /// The node `key` leads to; an unbound key gets an empty prefix.
    fn node_mut(&mut self, key: Key) -> &mut Node {
        let index = match self.bindings.iter().position(|(bound, _)| *bound == key) {
            Some(index) => index,
            None => {
                self.bindings.push((key, Node::Prefix(Prefix::default())));
                self.bindings.len() - 1
            }
        };
        &mut self.bindings[index].1
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Node {
    Command(String),
    Prefix(Prefix),
}

impl Node {
    fn binding(&self) -> Binding<'_> {
        match self {
            Self::Command(command) => Binding::Command(command),
            Self::Prefix(prefix) => Binding::Prefix(prefix),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One unmodified key per char.
    fn keys(chars: &str) -> Vec<Key> {
        chars.chars().map(Key::char).collect()
    }

    #[test]
    fn keys_lead_to_a_command_a_prefix_or_nothing() {
        let mut keymap = Keymap::new();
        keymap.bind(&keys("gg"), "goto_file_start");
        assert_eq!(
            keymap.get(&keys("gg")),
            Some(Binding::Command("goto_file_start"))
        );
        assert!(matches!(keymap.get(&keys("g")), Some(Binding::Prefix(_))));
        assert_eq!(keymap.get(&keys("ge")), None);
        // A command ends its sequence.
        assert_eq!(keymap.get(&keys("ggg")), None);
    }

    #[test]
    fn a_binding_replaces_what_its_keys_overlap() {
        let mut keymap = Keymap::new();
        keymap.bind(&keys("gg"), "goto_file_start");
        // A command where longer sequences started unbinds them.
        keymap.bind(&keys("g"), "goto_line");
        assert_eq!(keymap.get(&keys("g")), Some(Binding::Command("goto_line")));
        assert_eq!(keymap.get(&keys("gg")), None);
        // A sequence through a command's keys unbinds the command.
        keymap.bind(&keys("gd"), "goto_definition");
        assert_eq!(
            keymap.get(&keys("gd")),
            Some(Binding::Command("goto_definition"))
        );
        assert!(matches!(keymap.get(&keys("g")), Some(Binding::Prefix(_))));
    }

    #[test]
    fn bindings_list_keys_in_the_order_they_were_first_bound() {
        let mut keymap = Keymap::new();
        keymap.bind(&keys("gg"), "goto_file_start");
        keymap.bind(&keys("ge"), "goto_last_line");
        keymap.bind(&keys("gg"), "goto_first_line");
        let Some(Binding::Prefix(g)) = keymap.get(&keys("g")) else {
            panic!("g starts longer sequences");
        };
        assert_eq!(
            g.bindings().collect::<Vec<_>>(),
            [
                (Key::char('g'), Binding::Command("goto_first_line")),
                (Key::char('e'), Binding::Command("goto_last_line")),
            ]
        );
    }
}
