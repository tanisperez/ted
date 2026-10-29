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
    /// Unique among the transactions of one history, for the save point.
    id: u64,
    /// The edits, sorted by `start` and not overlapping, in the coordinates of the text before
    /// the transaction.
    edits: Vec<Edit>,
    kind: EditKind,
    before: Selection,
    after: Selection,
}

impl Transaction {
    /// Applies the edits, last first so the earlier offsets stay valid.
    fn apply(&self, buffer: &mut Buffer) {
        for edit in self.edits.iter().rev() {
            edit.apply(buffer);
        }
    }

    /// Reverts the edits: each inverse at its offset in the text after the transaction.
    fn revert(&self, buffer: &mut Buffer) {
        let mut shift = 0isize;
        let mut inverses = Vec::with_capacity(self.edits.len());
        for edit in &self.edits {
            let mut inverse = edit.inverse();
            inverse.start = (edit.start as isize + shift) as usize;
            shift += edit.new_text.len() as isize - edit.old_text.len() as isize;
            inverses.push(inverse);
        }
        for inverse in inverses.iter().rev() {
            inverse.apply(buffer);
        }
    }
}

/// The undo and redo stacks of one buffer, and the step the file on disk matches.
#[derive(Debug, Clone, Default)]
pub struct History {
    undo: Vec<Transaction>,
    redo: Vec<Transaction>,
    /// Set when the cursor moves, so the next edit starts a new step.
    broken: bool,
    next_id: u64,
    /// The id of the last step applied when the file was saved or loaded (`None`: no step).
    saved: Option<u64>,
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
            && last.edits.len() == 1
            && merge(last, &edit, kind)
        {
            last.after = after;
            return;
        }
        self.push(vec![edit], kind, before, after);
    }

    /// Applies several edits to `buffer` as one step: `edits` sorted by start, not overlapping,
    /// in the coordinates of the current text (trimming whitespace on save).
    pub fn edit_many(
        &mut self,
        buffer: &mut Buffer,
        edits: Vec<Edit>,
        before: Selection,
        after: Selection,
    ) {
        if edits.is_empty() {
            return;
        }
        let transaction = Transaction {
            id: 0,
            edits,
            kind: EditKind::Other,
            before,
            after,
        };
        transaction.apply(buffer);
        self.redo.clear();
        self.push(transaction.edits, EditKind::Other, before, after);
    }

    fn push(&mut self, edits: Vec<Edit>, kind: EditKind, before: Selection, after: Selection) {
        self.broken = false;
        self.next_id += 1;
        self.undo.push(Transaction {
            id: self.next_id,
            edits,
            kind,
            before,
            after,
        });
    }

    /// Starts a new undo step with the next edit (after a cursor movement or a click).
    pub fn break_group(&mut self) {
        self.broken = true;
    }

    /// The current step, to hand to [`Self::set_saved`] once the text taken now is on disk. The
    /// next edit starts a new step, so the checkpoint keeps naming exactly this text.
    pub fn checkpoint(&mut self) -> Option<u64> {
        self.broken = true;
        self.undo.last().map(|transaction| transaction.id)
    }

    /// Records that the file on disk holds the text of `checkpoint`.
    pub fn set_saved(&mut self, checkpoint: Option<u64>) {
        self.saved = checkpoint;
    }

    /// Whether the text differs from the file on disk: undoing back to the saved step makes it
    /// clean again.
    pub fn is_dirty(&self) -> bool {
        self.undo.last().map(|transaction| transaction.id) != self.saved
    }

    /// Undoes the last step and returns the selection from before it.
    pub fn undo(&mut self, buffer: &mut Buffer) -> Option<Selection> {
        let transaction = self.undo.pop()?;
        transaction.revert(buffer);
        let selection = transaction.before;
        self.redo.push(transaction);
        self.broken = true;
        Some(selection)
    }

    /// Redoes the last undone step and returns the selection from after it.
    pub fn redo(&mut self, buffer: &mut Buffer) -> Option<Selection> {
        let transaction = self.redo.pop()?;
        transaction.apply(buffer);
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
    let previous = &mut last.edits[0];
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

    #[test]
    fn several_edits_undo_and_redo_as_one_step() {
        let mut buffer = Buffer::from_text("a  \nb\t\nc");
        let mut history = History::default();
        let edits = vec![
            Edit {
                start: 1,
                old_text: "  ".into(),
                new_text: String::new(),
            },
            Edit {
                start: 5,
                old_text: "\t".into(),
                new_text: String::new(),
            },
            Edit {
                start: 8,
                old_text: String::new(),
                new_text: "\n".into(),
            },
        ];
        let cursor = Selection::cursor(0);
        history.edit_many(&mut buffer, edits, cursor, cursor);
        assert_eq!(buffer.to_text(), "a\nb\nc\n");
        history.undo(&mut buffer);
        assert_eq!(buffer.to_text(), "a  \nb\t\nc");
        history.redo(&mut buffer);
        assert_eq!(buffer.to_text(), "a\nb\nc\n");
    }

    #[test]
    fn the_save_point_follows_undo_and_redo() {
        let mut buffer = Buffer::from_text("");
        let mut history = History::default();
        assert!(!history.is_dirty());
        typed(&mut history, &mut buffer, 0, "a");
        assert!(history.is_dirty());
        let checkpoint = history.checkpoint();
        typed(&mut history, &mut buffer, 1, "b");
        history.set_saved(checkpoint);
        // "b" was typed after the checkpoint: still dirty, and it did not merge into "a".
        assert!(history.is_dirty());
        history.undo(&mut buffer);
        assert_eq!(buffer.to_text(), "a");
        assert!(!history.is_dirty());
        history.undo(&mut buffer);
        assert!(history.is_dirty());
        history.redo(&mut buffer);
        assert!(!history.is_dirty());
        // A new edit after undoing past the save point: the saved text cannot come back.
        history.undo(&mut buffer);
        typed(&mut history, &mut buffer, 0, "z");
        history.undo(&mut buffer);
        assert!(history.is_dirty());
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
