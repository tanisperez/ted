//! Undo and redo. Every change to a buffer is recorded as an [`Edit`] (the replaced text and its
//! replacement), grouped into transactions so that typing a word, or deleting a run of characters,
//! undoes in one step.

use crate::Buffer;

/// A cursor with an optional selection: the selection runs from `anchor` to `head` (in either
/// order), and the cursor is at `head`. Byte offsets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Selection {
    pub anchor: usize,
    pub head: usize,
}

impl Selection {
    /// A cursor with nothing selected.
    pub fn cursor(offset: usize) -> Self {
        Self {
            anchor: offset,
            head: offset,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.anchor == self.head
    }

    /// The selected bytes, start before end.
    pub fn range(&self) -> std::ops::Range<usize> {
        self.anchor.min(self.head)..self.anchor.max(self.head)
    }
}

/// One replacement: `old_text` at `start` became `new_text`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edit {
    pub start: usize,
    pub old_text: String,
    pub new_text: String,
}

impl Edit {
    /// Applies the edit to `buffer`, whose text at `start` must be `old_text`.
    pub fn apply(&self, buffer: &mut Buffer) {
        buffer.replace(self.start..self.start + self.old_text.len(), &self.new_text);
    }

    /// The edit that undoes this one.
    pub fn inverse(&self) -> Edit {
        Edit {
            start: self.start,
            old_text: self.new_text.clone(),
            new_text: self.old_text.clone(),
        }
    }
}

/// What kind of change an edit is, for grouping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditKind {
    /// Characters typed: consecutive ones on the same line group together, one word at a time.
    Typing,
    /// Backspace or Delete: consecutive ones group together.
    Deleting,
    /// Anything else (paste, cut, new line, indent): always its own step.
    Other,
}

#[derive(Debug, Clone)]
struct Transaction {
    edit: Edit,
    kind: EditKind,
    before: Selection,
    after: Selection,
}

/// The undo and redo stacks of one buffer.
#[derive(Debug, Clone, Default)]
pub struct History {
    undo: Vec<Transaction>,
    redo: Vec<Transaction>,
    /// Set when the cursor moves, so the next edit starts a new step.
    broken: bool,
}

impl History {
    /// Applies `edit` to `buffer` and records it. `before` and `after` are the selections to
    /// restore on undo and redo.
    pub fn edit(
        &mut self,
        buffer: &mut Buffer,
        edit: Edit,
        kind: EditKind,
        before: Selection,
        after: Selection,
    ) {
        if edit.old_text.is_empty() && edit.new_text.is_empty() {
            return;
        }
        edit.apply(buffer);
        self.redo.clear();
        if !self.broken
            && let Some(last) = self.undo.last_mut()
            && merge(last, &edit, kind)
        {
            last.after = after;
            return;
        }
        self.broken = false;
        self.undo.push(Transaction {
            edit,
            kind,
            before,
            after,
        });
    }

    /// Starts a new undo step with the next edit (after a cursor movement or a click).
    pub fn break_group(&mut self) {
        self.broken = true;
    }

    /// Undoes the last step and returns the selection from before it.
    pub fn undo(&mut self, buffer: &mut Buffer) -> Option<Selection> {
        let transaction = self.undo.pop()?;
        transaction.edit.inverse().apply(buffer);
        let selection = transaction.before;
        self.redo.push(transaction);
        self.broken = true;
        Some(selection)
    }

    /// Redoes the last undone step and returns the selection from after it.
    pub fn redo(&mut self, buffer: &mut Buffer) -> Option<Selection> {
        let transaction = self.redo.pop()?;
        transaction.edit.apply(buffer);
        let selection = transaction.after;
        self.undo.push(transaction);
        self.broken = true;
        Some(selection)
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
}

/// Folds `edit` into `last` when both are part of the same typing or deleting run.
fn merge(last: &mut Transaction, edit: &Edit, kind: EditKind) -> bool {
    if kind != last.kind {
        return false;
    }
    let previous = &mut last.edit;
    match kind {
        EditKind::Typing => {
            // A word typed after whitespace starts a new step, so undo goes back a word at a time.
            let starts_word = previous.new_text.ends_with(char::is_whitespace)
                && edit.new_text.starts_with(|ch: char| !ch.is_whitespace());
            let follows = edit.old_text.is_empty()
                && edit.start == previous.start + previous.new_text.len()
                && !edit.new_text.contains('\n')
                && !starts_word;
            if follows {
                previous.new_text.push_str(&edit.new_text);
            }
            follows
        }
        EditKind::Deleting if !edit.new_text.is_empty() || !previous.new_text.is_empty() => false,
        // Backspace: the deleted text ends where the previous one started.
        EditKind::Deleting if edit.start + edit.old_text.len() == previous.start => {
            previous.old_text.insert_str(0, &edit.old_text);
            previous.start = edit.start;
            true
        }
        // Delete: the same start, the text after it goes.
        EditKind::Deleting if edit.start == previous.start => {
            previous.old_text.push_str(&edit.old_text);
            true
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    fn typed(history: &mut History, buffer: &mut Buffer, at: usize, text: &str) {
        let edit = Edit {
            start: at,
            old_text: String::new(),
            new_text: text.to_string(),
        };
        let after = Selection::cursor(at + text.len());
        history.edit(buffer, edit, EditKind::Typing, Selection::cursor(at), after);
    }

    fn deleted(history: &mut History, buffer: &mut Buffer, range: std::ops::Range<usize>) {
        let edit = Edit {
            start: range.start,
            old_text: buffer.text_in(range.clone()),
            new_text: String::new(),
        };
        let before = Selection::cursor(range.end);
        let after = Selection::cursor(range.start);
        history.edit(buffer, edit, EditKind::Deleting, before, after);
    }

    #[test]
    fn typing_a_word_undoes_in_one_step() {
        let mut buffer = Buffer::from_text("");
        let mut history = History::default();
        for (at, ch) in "hey".char_indices() {
            typed(&mut history, &mut buffer, at, &ch.to_string());
        }
        assert_eq!(buffer.to_text(), "hey");
        assert_eq!(history.undo(&mut buffer), Some(Selection::cursor(0)));
        assert_eq!(buffer.to_text(), "");
        assert_eq!(history.redo(&mut buffer), Some(Selection::cursor(3)));
        assert_eq!(buffer.to_text(), "hey");
    }

    #[test]
    fn undo_goes_back_a_word_at_a_time() {
        let mut buffer = Buffer::from_text("");
        let mut history = History::default();
        for (at, ch) in "ab cd".char_indices() {
            typed(&mut history, &mut buffer, at, &ch.to_string());
        }
        history.undo(&mut buffer);
        assert_eq!(buffer.to_text(), "ab ");
        history.undo(&mut buffer);
        assert_eq!(buffer.to_text(), "");
    }

    #[test]
    fn a_new_line_or_a_cursor_move_starts_a_new_step() {
        let mut buffer = Buffer::from_text("");
        let mut history = History::default();
        typed(&mut history, &mut buffer, 0, "a");
        typed(&mut history, &mut buffer, 1, "\n");
        typed(&mut history, &mut buffer, 2, "b");
        history.break_group();
        typed(&mut history, &mut buffer, 3, "c");
        history.undo(&mut buffer);
        assert_eq!(buffer.to_text(), "a\nb");
        history.undo(&mut buffer);
        assert_eq!(buffer.to_text(), "a\n");
        history.undo(&mut buffer);
        assert_eq!(buffer.to_text(), "a");
        history.undo(&mut buffer);
        assert_eq!(buffer.to_text(), "");
        assert!(!history.can_undo());
    }

    #[test]
    fn backspace_and_delete_runs_group() {
        let mut buffer = Buffer::from_text("abcdef");
        let mut history = History::default();
        deleted(&mut history, &mut buffer, 5..6);
        deleted(&mut history, &mut buffer, 4..5);
        assert_eq!(buffer.to_text(), "abcd");
        deleted(&mut history, &mut buffer, 1..2);
        deleted(&mut history, &mut buffer, 1..2);
        assert_eq!(buffer.to_text(), "ad");
        history.undo(&mut buffer);
        assert_eq!(buffer.to_text(), "abcd");
        history.undo(&mut buffer);
        assert_eq!(buffer.to_text(), "abcdef");
    }

    #[test]
    fn a_new_edit_clears_the_redo_stack() {
        let mut buffer = Buffer::from_text("");
        let mut history = History::default();
        typed(&mut history, &mut buffer, 0, "a");
        history.undo(&mut buffer);
        typed(&mut history, &mut buffer, 0, "b");
        assert!(!history.can_redo());
        assert_eq!(history.redo(&mut buffer), None);
        assert_eq!(buffer.to_text(), "b");
    }

    /// Random edits, then undo everything: the text must come back to the start, and redo
    /// everything must reach the end.
    #[test]
    fn undo_all_and_redo_all_round_trip() {
        let mut rng = fastrand::Rng::with_seed(11);
        let start = "fn main() {\n    println!(\"ñ😀\");\n}\n";
        let mut buffer = Buffer::from_text(start);
        let mut history = History::default();
        for _ in 0..500 {
            let len = buffer.len();
            let a = buffer.rope().floor_char_boundary(rng.usize(..=len));
            let b = buffer.rope().floor_char_boundary(rng.usize(a..=len));
            let kind = [EditKind::Typing, EditKind::Deleting, EditKind::Other][rng.usize(..3)];
            let new_text = ["", "x", "\n", "ñ", "abc"][rng.usize(..5)].to_string();
            let edit = Edit {
                start: a,
                old_text: buffer.text_in(a..b),
                new_text,
            };
            if rng.u8(..10) == 0 {
                history.break_group();
            }
            history.edit(
                &mut buffer,
                edit,
                kind,
                Selection::cursor(a),
                Selection::cursor(a),
            );
        }
        let end = buffer.to_text();
        while history.undo(&mut buffer).is_some() {}
        assert_eq!(buffer.to_text(), start);
        while history.redo(&mut buffer).is_some() {}
        assert_eq!(buffer.to_text(), end);
    }
}
