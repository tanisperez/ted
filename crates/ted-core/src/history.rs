//! Undo and redo. Every change to a buffer is recorded as an [`Edit`] (the replaced text and its
//! replacement), grouped into transactions so that typing a word, or deleting a run of characters,
//! undoes in one step. With several cursors, one transaction holds an edit per cursor.

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
    /// The selections to restore on undo and on redo, the primary one first.
    before: Vec<Selection>,
    after: Vec<Selection>,
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
        self.edit_many(buffer, vec![edit], kind, vec![before], vec![after]);
    }

    /// Applies several edits to `buffer` as one step (one per cursor, or trimming whitespace on
    /// save): `edits` sorted by start, not overlapping, in the coordinates of the current text.
    /// `before` and `after` are the selections to restore on undo and redo, the primary first.
    /// Typing or deleting at every cursor groups with the previous step as for one cursor.
    pub fn edit_many(
        &mut self,
        buffer: &mut Buffer,
        edits: Vec<Edit>,
        kind: EditKind,
        before: Vec<Selection>,
        after: Vec<Selection>,
    ) {
        let edits: Vec<Edit> = edits
            .into_iter()
            .filter(|edit| !(edit.old_text.is_empty() && edit.new_text.is_empty()))
            .collect();
        if edits.is_empty() {
            return;
        }
        let transaction = Transaction {
            id: 0,
            edits,
            kind,
            before,
            after,
        };
        transaction.apply(buffer);
        self.redo.clear();
        if !self.broken
            && let Some(last) = self.undo.last_mut()
            && merge(last, &transaction.edits, kind)
        {
            last.after = transaction.after;
            return;
        }
        self.push(
            transaction.edits,
            kind,
            transaction.before,
            transaction.after,
        );
    }

    fn push(
        &mut self,
        edits: Vec<Edit>,
        kind: EditKind,
        before: Vec<Selection>,
        after: Vec<Selection>,
    ) {
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

    /// Undoes the last step and returns the selections from before it, the primary first.
    pub fn undo(&mut self, buffer: &mut Buffer) -> Option<Vec<Selection>> {
        let transaction = self.undo.pop()?;
        transaction.revert(buffer);
        let selections = transaction.before.clone();
        self.redo.push(transaction);
        self.broken = true;
        Some(selections)
    }

    /// Redoes the last undone step and returns the selections from after it, the primary first.
    pub fn redo(&mut self, buffer: &mut Buffer) -> Option<Vec<Selection>> {
        let transaction = self.redo.pop()?;
        transaction.apply(buffer);
        let selections = transaction.after.clone();
        self.undo.push(transaction);
        self.broken = true;
        Some(selections)
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
}

/// Folds `edits` into `last` when they continue its typing or deleting run at every cursor:
/// the same number of edits, each one next to the edit of `last` at the same index.
///
/// `edits` are in the coordinates of the text after `last`, whose edits are in the coordinates of
/// the text before it, so edit `i` of `last` ends, in the new text, `shift` bytes later than its
/// start: the growth of the edits before it plus its own new text.
fn merge(last: &mut Transaction, edits: &[Edit], kind: EditKind) -> bool {
    if kind != last.kind || kind == EditKind::Other || edits.len() != last.edits.len() {
        return false;
    }
    // Where each edit of `last` starts in the current text.
    let mut starts = Vec::with_capacity(edits.len());
    let mut shift = 0isize;
    for previous in &last.edits {
        starts.push((previous.start as isize + shift) as usize);
        shift += previous.new_text.len() as isize - previous.old_text.len() as isize;
    }
    let mut plan = Vec::with_capacity(edits.len());
    for (index, (previous, edit)) in last.edits.iter().zip(edits).enumerate() {
        let at = starts[index];
        let step = match kind {
            EditKind::Typing => {
                // A word typed after whitespace starts a new step, so undo goes back a word at a
                // time.
                let starts_word = previous.new_text.ends_with(char::is_whitespace)
                    && edit.new_text.starts_with(|ch: char| !ch.is_whitespace());
                let follows = edit.old_text.is_empty()
                    && edit.start == at + previous.new_text.len()
                    && !edit.new_text.contains('\n')
                    && !starts_word;
                follows.then_some(Fold::Append)
            }
            _ if !edit.new_text.is_empty() || !previous.new_text.is_empty() => None,
            // Backspace: the deleted text ends where the previous one started, after the edit
            // before it.
            _ if edit.start + edit.old_text.len() == at
                && (index == 0 || edit.start >= starts[index - 1]) =>
            {
                Some(Fold::Prepend(previous.start - edit.old_text.len()))
            }
            // Delete: the same start, the text after it goes, up to the next edit.
            _ if edit.start == at
                && starts
                    .get(index + 1)
                    .is_none_or(|&next| edit.start + edit.old_text.len() <= next) =>
            {
                Some(Fold::Append)
            }
            _ => None,
        };
        match step {
            Some(step) => plan.push(step),
            None => return false,
        }
    }
    for ((previous, edit), step) in last.edits.iter_mut().zip(edits).zip(plan) {
        match step {
            Fold::Append if kind == EditKind::Typing => previous.new_text.push_str(&edit.new_text),
            Fold::Append => previous.old_text.push_str(&edit.old_text),
            Fold::Prepend(start) => {
                previous.old_text.insert_str(0, &edit.old_text);
                previous.start = start;
            }
        }
    }
    true
}

/// How one edit folds into the edit of the previous step at the same index.
enum Fold {
    /// Typing after it, or Delete at its start.
    Append,
    /// Backspace before it: the merged edit starts at this offset of the text before the step.
    Prepend(usize),
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
        assert_eq!(history.undo(&mut buffer), Some(vec![Selection::cursor(0)]));
        assert_eq!(buffer.to_text(), "");
        assert_eq!(history.redo(&mut buffer), Some(vec![Selection::cursor(3)]));
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
        let cursor = vec![Selection::cursor(0)];
        history.edit_many(&mut buffer, edits, EditKind::Other, cursor.clone(), cursor);
        assert_eq!(buffer.to_text(), "a\nb\nc\n");
        history.undo(&mut buffer);
        assert_eq!(buffer.to_text(), "a  \nb\t\nc");
        history.redo(&mut buffer);
        assert_eq!(buffer.to_text(), "a\nb\nc\n");
    }

    /// Inserts `text` at every offset of `at` (sorted, in the current text) as one step.
    fn typed_at_each(history: &mut History, buffer: &mut Buffer, at: &[usize], text: &str) {
        let edits = at
            .iter()
            .map(|&start| Edit {
                start,
                old_text: String::new(),
                new_text: text.to_string(),
            })
            .collect();
        let before = at.iter().map(|&at| Selection::cursor(at)).collect();
        let after = at
            .iter()
            .enumerate()
            .map(|(index, &at)| Selection::cursor(at + (index + 1) * text.len()))
            .collect();
        history.edit_many(buffer, edits, EditKind::Typing, before, after);
    }

    #[test]
    fn typing_at_several_cursors_undoes_a_word_at_a_time() {
        let mut buffer = Buffer::from_text("a\nb\n");
        let mut history = History::default();
        // Cursors after "a" and after "b": each keystroke shifts the second one by the first.
        typed_at_each(&mut history, &mut buffer, &[1, 3], "x");
        typed_at_each(&mut history, &mut buffer, &[2, 5], "y");
        typed_at_each(&mut history, &mut buffer, &[3, 7], " ");
        typed_at_each(&mut history, &mut buffer, &[4, 9], "z");
        assert_eq!(buffer.to_text(), "axy z\nbxy z\n");
        history.undo(&mut buffer);
        assert_eq!(buffer.to_text(), "axy \nbxy \n");
        assert_eq!(
            history.undo(&mut buffer),
            Some(vec![Selection::cursor(1), Selection::cursor(3)])
        );
        assert_eq!(buffer.to_text(), "a\nb\n");
        assert!(!history.can_undo());
    }

    #[test]
    fn backspace_and_delete_at_several_cursors_group() {
        let mut buffer = Buffer::from_text("abc\ndef");
        let mut history = History::default();
        let step = |history: &mut History, buffer: &mut Buffer, ranges: [(usize, usize); 2]| {
            let edits = ranges
                .iter()
                .map(|&(start, end)| Edit {
                    start,
                    old_text: buffer.text_in(start..end),
                    new_text: String::new(),
                })
                .collect();
            let cursors = vec![Selection::cursor(0); 2];
            history.edit_many(buffer, edits, EditKind::Deleting, cursors.clone(), cursors);
        };
        // Backspace at the ends of both lines, twice.
        step(&mut history, &mut buffer, [(2, 3), (6, 7)]);
        step(&mut history, &mut buffer, [(1, 2), (4, 5)]);
        assert_eq!(buffer.to_text(), "a\nd");
        // A third backspace on each line: "a" and "d" go, still the same step.
        step(&mut history, &mut buffer, [(0, 1), (2, 3)]);
        assert_eq!(buffer.to_text(), "\n");
        history.undo(&mut buffer);
        assert_eq!(buffer.to_text(), "abc\ndef");
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

    /// The same round trip with several edits per step, as several cursors make, so that
    /// grouping runs at every cursor keeps undo exact.
    #[test]
    fn undo_all_and_redo_all_round_trip_with_several_cursors() {
        let mut rng = fastrand::Rng::with_seed(29);
        let start = "one\ntwo ñ\nthree 😀\nfour\n";
        let mut buffer = Buffer::from_text(start);
        let mut history = History::default();
        for _ in 0..500 {
            let len = buffer.len();
            let mut cuts: Vec<usize> = (0..rng.usize(1..=6))
                .map(|_| buffer.rope().floor_char_boundary(rng.usize(..=len)))
                .collect();
            cuts.sort_unstable();
            cuts.dedup();
            let kind = [EditKind::Typing, EditKind::Deleting, EditKind::Other][rng.usize(..3)];
            let mut edits = Vec::new();
            let mut previous_end = 0;
            for &cut in &cuts {
                if cut < previous_end {
                    continue;
                }
                let end = if kind == EditKind::Typing {
                    cut
                } else {
                    let limit = cuts
                        .iter()
                        .find(|&&next| next > cut)
                        .copied()
                        .unwrap_or(len);
                    buffer.rope().floor_char_boundary(rng.usize(cut..=limit))
                };
                let new_text = match kind {
                    EditKind::Deleting => String::new(),
                    _ => ["x", " ", "\n", "ñ"][rng.usize(..4)].to_string(),
                };
                edits.push(Edit {
                    start: cut,
                    old_text: buffer.text_in(cut..end),
                    new_text,
                });
                previous_end = end;
            }
            if rng.u8(..10) == 0 {
                history.break_group();
            }
            let cursors = vec![Selection::cursor(0); edits.len()];
            history.edit_many(&mut buffer, edits, kind, cursors.clone(), cursors);
        }
        let end = buffer.to_text();
        while history.undo(&mut buffer).is_some() {}
        assert_eq!(buffer.to_text(), start);
        while history.redo(&mut buffer).is_some() {}
        assert_eq!(buffer.to_text(), end);
    }
}
