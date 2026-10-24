//! A uniform grid over the living horde, rebuilt once a step: separation, splash, areas and the
//! "everything near here" queries walk only the cells they touch instead of the whole horde.

use glam::Vec2;

const CELL: f32 = 3.0;

#[derive(Default)]
pub(crate) struct Grid {
    /// `(cell key, index into the enemy list)`, sorted by key.
    entries: Vec<(u64, u32)>,
}

fn cell(p: Vec2) -> (i32, i32) {
    ((p.x / CELL).floor() as i32, (p.y / CELL).floor() as i32)
}

fn key(c: (i32, i32)) -> u64 {
    (u64::from(c.0 as u32) << 32) | u64::from(c.1 as u32)
}

impl Grid {
    /// Index the points `items` yields, by their position in the iteration.
    pub fn rebuild(&mut self, items: impl Iterator<Item = Option<Vec2>>) {
        self.entries.clear();
        for (i, p) in items.enumerate() {
            if let Some(p) = p {
                self.entries.push((key(cell(p)), i as u32));
            }
        }
        self.entries.sort_unstable_by_key(|e| e.0);
    }

    fn cell_items(&self, c: (i32, i32)) -> &[(u64, u32)] {
        let k = key(c);
        let lo = self.entries.partition_point(|e| e.0 < k);
        let hi = lo + self.entries[lo..].partition_point(|e| e.0 == k);
        &self.entries[lo..hi]
    }

    /// Every indexed item in the cells within `r` of `center` (a superset of the circle).
    pub fn near(&self, center: Vec2, r: f32, mut f: impl FnMut(usize)) {
        let lo = cell(center - Vec2::splat(r));
        let hi = cell(center + Vec2::splat(r));
        // A huge radius would walk empty cells: past this, scan the list instead.
        if (hi.0 - lo.0 + 1) as i64 * (hi.1 - lo.1 + 1) as i64 > self.entries.len() as i64 {
            for e in &self.entries {
                f(e.1 as usize);
            }
            return;
        }
        for x in lo.0..=hi.0 {
            for z in lo.1..=hi.1 {
                for e in self.cell_items((x, z)) {
                    f(e.1 as usize);
                }
            }
        }
    }
}
