use temper_domain::ModTime;
use temper_domain::PathStyle::Posix;

use super::list_plans;
use crate::fakes::{FakeFiles, FakePorts};

const PLANS: &str = "/w/.temper/plans";

fn at(ms: i128) -> ModTime {
    ModTime::from_unix_nanos(ms * 1_000_000)
}

fn plan(files: FakeFiles, name: &str, ms: i128) -> FakeFiles {
    let path = format!("{PLANS}/{name}");
    files.with_file(&path, "# plan\n").with_mtime(&path, at(ms))
}

#[test]
fn the_five_newest_markdown_plans_ties_in_directory_order() {
    let files = [("a.md", 5), ("b.md", 9), ("c.txt", 99), ("d.md", 5), ("e.md", 1), ("f.md", 7), ("g.md", 8)]
        .into_iter()
        .fold(FakeFiles::new(Posix), |files, (name, ms)| plan(files, name, ms));
    let ports = FakePorts::new(files);

    assert_eq!(
        list_plans(&ports.files, Posix, "/w"),
        [
            ".temper/plans/b.md",
            ".temper/plans/g.md",
            ".temper/plans/f.md",
            ".temper/plans/a.md",
            ".temper/plans/d.md",
        ],
        "newest first, a before d on a tie, at most five, .md only"
    );
}

#[test]
fn any_stat_failure_or_no_directory_gives_none() {
    let dangling = plan(FakeFiles::new(Posix), "a.md", 5).with_link(&format!("{PLANS}/x.md"), "/nowhere");
    let files = dangling.with_listing(PLANS, &[("a.md", crate::ports::EntryKind::File), ("x.md", crate::ports::EntryKind::Symlink)]);

    assert_eq!(list_plans(&files, Posix, "/w"), Vec::<String>::new(), "a dangling x.md");
    assert_eq!(list_plans(&FakeFiles::new(Posix), Posix, "/w"), Vec::<String>::new(), "no dir");
}
