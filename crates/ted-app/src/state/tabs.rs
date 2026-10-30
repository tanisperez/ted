//! The open documents of folder mode, in tab order, and which one is active. Plain data: the
//! views keep one item per open file (its editor and its load), this decides where opening,
//! closing, switching and reordering leave the active tab.

/// The open items, left to right, and the active one.
#[derive(Debug)]
pub struct Tabs<T> {
    items: Vec<T>,
    active: Option<usize>,
}

impl<T> Default for Tabs<T> {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            active: None,
        }
    }
}

impl<T> Tabs<T> {
    pub fn items(&self) -> &[T] {
        &self.items
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn active_index(&self) -> Option<usize> {
        self.active
    }

    pub fn active(&self) -> Option<&T> {
        self.active.map(|index| &self.items[index])
    }

    pub fn get_mut(&mut self, index: usize) -> Option<&mut T> {
        self.items.get_mut(index)
    }

    /// The index of the first item that matches.
    pub fn position(&self, matches: impl Fn(&T) -> bool) -> Option<usize> {
        self.items.iter().position(matches)
    }

    /// Adds `item` right after the active tab (at the end with none) and makes it active.
    /// Returns its index.
    pub fn open(&mut self, item: T) -> usize {
        let index = self.active.map_or(self.items.len(), |active| active + 1);
        self.items.insert(index, item);
        self.active = Some(index);
        index
    }

    /// Makes the tab at `index` active. Returns whether that changed anything.
    pub fn activate(&mut self, index: usize) -> bool {
        if index >= self.items.len() || self.active == Some(index) {
            return false;
        }
        self.active = Some(index);
        true
    }

    /// Removes the tab at `index`. Closing the active tab activates the one that takes its
    /// place, or the one to its left when it was the last.
    pub fn close(&mut self, index: usize) -> Option<T> {
        if index >= self.items.len() {
            return None;
        }
        let item = self.items.remove(index);
        self.active = match self.active {
            _ if self.items.is_empty() => None,
            Some(active) if active > index => Some(active - 1),
            Some(active) if active == index => Some(index.min(self.items.len() - 1)),
            other => other,
        };
        Some(item)
    }

    /// Removes every tab.
    pub fn clear(&mut self) -> Vec<T> {
        self.active = None;
        std::mem::take(&mut self.items)
    }

    /// Activates the next tab (`forward`) or the previous one, wrapping around. Returns whether
    /// the active tab changed.
    pub fn cycle(&mut self, forward: bool) -> bool {
        let (Some(active), count) = (self.active, self.items.len()) else {
            return false;
        };
        let next = if forward {
            (active + 1) % count
        } else {
            (active + count - 1) % count
        };
        self.activate(next)
    }

    /// Moves the tab at `from` to `to`, keeping the same tab active.
    pub fn move_tab(&mut self, from: usize, to: usize) {
        let count = self.items.len();
        if from >= count || from == to {
            return;
        }
        let to = to.min(count - 1);
        let item = self.items.remove(from);
        self.items.insert(to, item);
        self.active = self.active.map(|active| {
            if active == from {
                to
            } else if from < active && active <= to {
                active - 1
            } else if to <= active && active < from {
                active + 1
            } else {
                active
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    fn tabs(items: &[&'static str], active: usize) -> Tabs<&'static str> {
        Tabs {
            items: items.to_vec(),
            active: Some(active),
        }
    }

    #[test]
    fn opening_adds_after_the_active_tab_and_activates() {
        let mut tabs = Tabs::default();
        assert_eq!(tabs.open("a"), 0);
        assert_eq!(tabs.open("b"), 1);
        tabs.activate(0);
        assert_eq!(tabs.open("c"), 1);
        assert_eq!(tabs.items(), ["a", "c", "b"]);
        assert_eq!(tabs.active(), Some(&"c"));
    }

    #[test]
    fn closing_the_active_tab_activates_its_neighbour() {
        let mut middle = tabs(&["a", "b", "c"], 1);
        assert_eq!(middle.close(1), Some("b"));
        assert_eq!(middle.active(), Some(&"c"));

        let mut last = tabs(&["a", "b", "c"], 2);
        last.close(2);
        assert_eq!(last.active(), Some(&"b"));

        let mut only = tabs(&["a"], 0);
        only.close(0);
        assert_eq!(only.active(), None);
        assert!(only.is_empty());
    }

    #[test]
    fn closing_another_tab_keeps_the_active_one() {
        let mut before = tabs(&["a", "b", "c"], 2);
        before.close(0);
        assert_eq!(before.active(), Some(&"c"));

        let mut after = tabs(&["a", "b", "c"], 0);
        after.close(2);
        assert_eq!(after.active(), Some(&"a"));

        assert_eq!(after.close(5), None);
    }

    #[test]
    fn cycling_wraps_around() {
        let mut tabs = tabs(&["a", "b", "c"], 2);
        assert!(tabs.cycle(true));
        assert_eq!(tabs.active(), Some(&"a"));
        assert!(tabs.cycle(false));
        assert_eq!(tabs.active(), Some(&"c"));

        let mut single = super::tests::tabs(&["a"], 0);
        assert!(!single.cycle(true));
        assert!(!Tabs::<&str>::default().cycle(true));
    }

    #[test]
    fn moving_keeps_the_same_tab_active() {
        let mut dragged = tabs(&["a", "b", "c", "d"], 1);
        dragged.move_tab(1, 3);
        assert_eq!(dragged.items(), ["a", "c", "d", "b"]);
        assert_eq!(dragged.active(), Some(&"b"));

        let mut right = tabs(&["a", "b", "c", "d"], 2);
        right.move_tab(0, 3);
        assert_eq!(right.items(), ["b", "c", "d", "a"]);
        assert_eq!(right.active(), Some(&"c"));

        let mut left = tabs(&["a", "b", "c", "d"], 1);
        left.move_tab(3, 0);
        assert_eq!(left.items(), ["d", "a", "b", "c"]);
        assert_eq!(left.active(), Some(&"b"));
    }

    #[test]
    fn position_and_clear() {
        let mut tabs = tabs(&["a", "b"], 1);
        assert_eq!(tabs.position(|item| *item == "b"), Some(1));
        assert_eq!(tabs.position(|item| *item == "z"), None);
        assert_eq!(tabs.clear(), ["a", "b"]);
        assert_eq!(tabs.active(), None);
    }
}
