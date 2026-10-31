//! Plain data behind the branch menu of the status bar (ported from Postino): the branches
//! grouped as Recent, Local and one Remote group per remote, each row with its last commit and
//! sync state. Built once when the menu opens or the branches change, never in render.

use rust_i18n::t;
use ted_workspace::natural_cmp;
use telaio_git::{BranchInfo, RefKind};

/// Seconds in a day.
const DAY: i64 = 24 * 60 * 60;

/// How many branches the Recent group shows at most.
pub const RECENT_LIMIT: usize = 5;

/// What a menu group holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BranchGroupKind {
    /// The local branches checked out last, the current one first.
    Recent,
    /// The other local branches.
    Local,
    /// The branches of one remote.
    Remote(String),
}

impl BranchGroupKind {
    /// The group's label, "Remote · origin".
    pub fn label(&self) -> String {
        match self {
            BranchGroupKind::Recent => t!("git.branches.recent").into_owned(),
            BranchGroupKind::Local => t!("git.branches.local").into_owned(),
            BranchGroupKind::Remote(remote) => {
                format!("{} \u{b7} {remote}", t!("git.branches.remote"))
            }
        }
    }
}

/// One branch row of the menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BranchRow {
    /// The name git knows it by: `main`, `origin/main`.
    pub name: String,
    /// Whether it is a remote tracking branch.
    pub remote: bool,
    /// Whether it is checked out.
    pub current: bool,
    /// The last commit's subject and when it was made, "Add refresh token · 12 min ago".
    pub subtitle: String,
    /// The sync state against its upstream, "↑1 ↓2", empty when in sync or without one.
    pub sync: String,
}

/// A group of rows with its kind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BranchGroup {
    /// What the group holds.
    pub kind: BranchGroupKind,
    /// Its rows, in display order.
    pub rows: Vec<BranchRow>,
}

/// Groups `branches` for the menu. `recent` is the order of the last checkouts (most recent
/// first, as `Repo::recent_branches` gives it), `now` the current unix time. Tags are left out: they are checked out from the log. Empty groups are dropped.
pub fn branch_groups(branches: &[BranchInfo], recent: &[String], now: i64) -> Vec<BranchGroup> {
    let row = |branch: &BranchInfo| BranchRow {
        name: branch.name.clone(),
        remote: matches!(branch.kind, RefKind::Remote { .. }),
        current: branch.is_head,
        subtitle: subtitle(branch, now),
        sync: sync_label(branch.ahead, branch.behind),
    };
    let locals: Vec<&BranchInfo> = branches
        .iter()
        .filter(|branch| branch.kind == RefKind::Local)
        .collect();

    // The current branch always leads Recent, even before it is in the reflog.
    let mut recent_names: Vec<&str> = locals
        .iter()
        .filter(|branch| branch.is_head)
        .map(|branch| branch.name.as_str())
        .collect();
    for name in recent {
        if recent_names.len() >= RECENT_LIMIT {
            break;
        }
        if !recent_names.contains(&name.as_str()) && locals.iter().any(|b| &b.name == name) {
            recent_names.push(name);
        }
    }
    let recent_rows: Vec<BranchRow> = recent_names
        .iter()
        .filter_map(|name| locals.iter().find(|branch| branch.name == *name))
        .map(|branch| row(branch))
        .collect();

    let mut local_rows: Vec<BranchRow> = locals
        .iter()
        .filter(|branch| !recent_names.contains(&branch.name.as_str()))
        .map(|branch| row(branch))
        .collect();
    sort_rows(&mut local_rows);

    let mut groups = vec![
        BranchGroup {
            kind: BranchGroupKind::Recent,
            rows: recent_rows,
        },
        BranchGroup {
            kind: BranchGroupKind::Local,
            rows: local_rows,
        },
    ];

    let mut remotes: Vec<&str> = branches
        .iter()
        .filter_map(|branch| match &branch.kind {
            RefKind::Remote { remote } => Some(remote.as_str()),
            _ => None,
        })
        .collect();
    remotes.sort_by(|a, b| natural_cmp(a, b));
    remotes.dedup();
    for remote in remotes {
        let mut rows: Vec<BranchRow> = branches
            .iter()
            .filter(|branch| matches!(&branch.kind, RefKind::Remote { remote: r } if r == remote))
            .map(row)
            .collect();
        sort_rows(&mut rows);
        groups.push(BranchGroup {
            kind: BranchGroupKind::Remote(remote.to_string()),
            rows,
        });
    }
    groups.retain(|group| !group.rows.is_empty());
    groups
}

/// By name in natural order.
fn sort_rows(rows: &mut [BranchRow]) {
    rows.sort_by(|a, b| natural_cmp(&a.name, &b.name));
}

/// "Subject · 12 min ago".
fn subtitle(branch: &BranchInfo, now: i64) -> String {
    let when = relative_time(branch.date, now);
    if branch.subject.is_empty() {
        when
    } else {
        format!("{} \u{b7} {when}", branch.subject)
    }
}

/// How long ago a moment was, for commits and branches: "just now", "12 min ago", "3 h ago",
/// "yesterday", "5 days ago", "4 months ago", "2 years ago". Times are unix seconds.
pub fn relative_time(then: i64, now: i64) -> String {
    let elapsed = now - then;
    let count = |count: i64| telaio_app::number::format_integer(count.max(0) as u64);
    if elapsed < 60 {
        t!("git.time.just_now").into_owned()
    } else if elapsed < 60 * 60 {
        t!("git.time.minutes_ago", count = count(elapsed / 60)).into_owned()
    } else if elapsed < DAY {
        t!("git.time.hours_ago", count = count(elapsed / 3600)).into_owned()
    } else if elapsed < 2 * DAY {
        t!("git.time.yesterday").into_owned()
    } else if elapsed < 60 * DAY {
        t!("git.time.days_ago", count = count(elapsed / DAY)).into_owned()
    } else if elapsed < 730 * DAY {
        t!("git.time.months_ago", count = count(elapsed / (30 * DAY))).into_owned()
    } else {
        t!("git.time.years_ago", count = count(elapsed / (365 * DAY))).into_owned()
    }
}

/// "↑1 ↓2", or the part that is not zero, or nothing.
pub fn sync_label(ahead: u32, behind: u32) -> String {
    let mut parts = Vec::new();
    if ahead > 0 {
        parts.push(format!("\u{2191}{ahead}"));
    }
    if behind > 0 {
        parts.push(format!("\u{2193}{behind}"));
    }
    parts.join(" ")
}

/// The names of the local branches, for "does this name exist" checks of the create row.
pub fn local_names(branches: &[BranchInfo]) -> Vec<&str> {
    branches
        .iter()
        .filter(|branch| branch.kind == RefKind::Local)
        .map(|branch| branch.name.as_str())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn branch(name: &str, kind: RefKind, head: bool) -> BranchInfo {
        BranchInfo {
            name: name.to_string(),
            kind,
            oid: "abc1234".to_string(),
            subject: format!("Last on {name}"),
            date: 1_000,
            upstream: None,
            ahead: 0,
            behind: 0,
            is_head: head,
        }
    }

    fn remote(name: &str, remote: &str) -> BranchInfo {
        branch(
            name,
            RefKind::Remote {
                remote: remote.to_string(),
            },
            false,
        )
    }

    fn names(group: &BranchGroup) -> Vec<&str> {
        group.rows.iter().map(|row| row.name.as_str()).collect()
    }

    #[test]
    fn groups_recent_local_and_each_remote() {
        let branches = vec![
            branch("main", RefKind::Local, false),
            branch("feature/b", RefKind::Local, true),
            branch("feature/a", RefKind::Local, false),
            branch("fix/c", RefKind::Local, false),
            remote("origin/main", "origin"),
            remote("upstream/main", "upstream"),
            branch("v1.0", RefKind::Tag, false),
        ];
        let recent = vec!["feature/a".to_string(), "gone".to_string()];
        let groups = branch_groups(&branches, &recent, 1_000);
        let kinds: Vec<_> = groups.iter().map(|group| group.kind.clone()).collect();
        assert_eq!(
            kinds,
            vec![
                BranchGroupKind::Recent,
                BranchGroupKind::Local,
                BranchGroupKind::Remote("origin".into()),
                BranchGroupKind::Remote("upstream".into()),
            ]
        );
        assert_eq!(names(&groups[0]), vec!["feature/b", "feature/a"]);
        // In natural order, nothing repeated from Recent.
        assert_eq!(names(&groups[1]), vec!["fix/c", "main"]);
        assert!(groups[0].rows[0].current);
        assert!(groups[2].rows[0].remote);
    }

    #[test]
    fn recent_is_capped() {
        let mut branches = vec![branch("main", RefKind::Local, true)];
        let mut recent = Vec::new();
        for i in 0..10 {
            branches.push(branch(&format!("b{i}"), RefKind::Local, false));
            recent.push(format!("b{i}"));
        }
        let groups = branch_groups(&branches, &recent, 1_000);
        assert_eq!(groups[0].rows.len(), RECENT_LIMIT);
        assert_eq!(groups[0].rows[0].name, "main");
    }

    #[test]
    fn relative_time_picks_the_unit() {
        let now = 100 * DAY;
        assert_eq!(relative_time(now - 30, now), t!("git.time.just_now"));
        assert_eq!(
            relative_time(now - 12 * 60, now),
            t!("git.time.minutes_ago", count = "12")
        );
        assert_eq!(relative_time(now - DAY - 5, now), t!("git.time.yesterday"));
        assert_eq!(
            relative_time(now - 5 * DAY, now),
            t!("git.time.days_ago", count = "5")
        );
        assert_eq!(
            relative_time(now - 90 * DAY, now),
            t!("git.time.months_ago", count = "3")
        );
    }

    #[test]
    fn sync_label_shows_what_is_not_zero() {
        assert_eq!(sync_label(0, 0), "");
        assert_eq!(sync_label(1, 0), "\u{2191}1");
        assert_eq!(sync_label(2, 3), "\u{2191}2 \u{2193}3");
    }
}
