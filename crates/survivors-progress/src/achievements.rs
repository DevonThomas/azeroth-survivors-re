//! Achievements: milestones across every run. Each one earned is a perk point.

use survivors_sim::data;

use crate::account::Account;

pub struct Achievement {
    pub key: String,
    pub name: String,
    pub desc: String,
    /// A WoW Classic spell for the icon.
    pub icon_spell: u32,
    test: Test,
}

enum Test {
    Kills(u64),
    Bosses(u64),
    Runs(u64),
    Gold(u64),
    Level(u32),
    Items(usize),
    Talents(u32),
    /// Win this battleground (by index), any class.
    Zone(u16),
    /// Win anywhere as this class.
    ClassWin(u8),
}

impl Achievement {
    pub fn earned(&self, a: &Account) -> bool {
        match self.test {
            Test::Kills(n) => a.lifetime.kills >= n,
            Test::Bosses(n) => a.lifetime.bosses >= n,
            Test::Runs(n) => a.lifetime.runs >= n,
            Test::Gold(n) => a.lifetime.gold >= n,
            Test::Level(n) => a.lifetime.best_level >= n,
            Test::Items(n) => a.collection.len() >= n,
            Test::Talents(n) => a.classes.values().any(|c| c.talent_points_spent() >= n),
            Test::Zone(z) => a.classes.values().any(|c| c.won.contains(&z)),
            Test::ClassWin(c) => a.classes.get(&c).is_some_and(|c| !c.won.is_empty()),
        }
    }
}

fn a(key: &str, name: &str, desc: &str, icon_spell: u32, test: Test) -> Achievement {
    Achievement {
        key: key.into(),
        name: name.into(),
        desc: desc.into(),
        icon_spell,
        test,
    }
}

pub static ACHIEVEMENTS: std::sync::LazyLock<Vec<Achievement>> = std::sync::LazyLock::new(|| {
    let mut v = vec![
        a("runs_1", "First Steps", "Finish a run.", 2457, Test::Runs(1)),
        a("runs_25", "Seasoned", "Finish 25 runs.", 2458, Test::Runs(25)),
        a("kills_1k", "Thinning the Herd", "Defeat 1,000 enemies.", 845, Test::Kills(1_000)),
        a("kills_10k", "Exterminator", "Defeat 10,000 enemies.", 1680, Test::Kills(10_000)),
        a("kills_100k", "Scourge of Azeroth", "Defeat 100,000 enemies.", 11113, Test::Kills(100_000)),
        a("bosses_1", "Giant Slayer", "Defeat a boss.", 12294, Test::Bosses(1)),
        a("bosses_25", "Bane of Tyrants", "Defeat 25 bosses.", 20230, Test::Bosses(25)),
        a("gold_1k", "Purse Strings", "Collect 1,000 gold.", 921, Test::Gold(1_000)),
        a("gold_25k", "Hoarder", "Collect 25,000 gold.", 1804, Test::Gold(25_000)),
        a("level_25", "Rising Power", "Reach level 25 in a run.", 1459, Test::Level(25)),
        a("level_50", "Ascendant", "Reach level 50 in a run.", 12042, Test::Level(50)),
        a("items_8", "Well Dressed", "Collect 8 pieces of gear.", 6117, Test::Items(8)),
        a("items_32", "Armory", "Collect 32 pieces of gear.", 7302, Test::Items(32)),
        a("talents_31", "Deep Roots", "Spend 31 talent points as one class.", 16864, Test::Talents(31)),
        a("talents_51", "Master of the Craft", "Spend all 51 talent points as one class.", 12051, Test::Talents(51)),
    ];
    for (i, m) in data::MAPS.iter().enumerate() {
        v.push(a(
            &format!("zone_{i}"),
            &format!("Champion of {}", m.name),
            &format!("Win a run at {}.", m.name),
            642,
            Test::Zone(i as u16),
        ));
    }
    for c in data::CLASSES {
        v.push(a(
            &format!("class_{}", c.id),
            &format!("{} Triumphant", c.name),
            &format!("Win a run as a {}.", c.name),
            data::ability(c.start).spell,
            Test::ClassWin(c.id),
        ));
    }
    v
});
