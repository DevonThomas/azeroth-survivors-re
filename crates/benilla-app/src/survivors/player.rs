//! Heroes as WoW player characters: the race and sex's own body, the face, hair and skin picked,
//! and the gear worn drawn on it, as a streamed player is drawn. A player's worn items are item
//! entries the client looks up as templates (normally the server's); a local hero's are made up
//! here from the `ItemDisplayInfo` art alone, in an entry range no real item uses.

use benilla_protocol::messages::ItemInfo;
use benilla_protocol::messages::ObjectFields;
use bevy::prelude::*;
use survivors_sim::{Appearance, GEAR_SLOTS};

use super::data::{ClassDef, Weapon};
use super::units::FACTION_FRIENDLY;
use super::RunEntity;
use crate::items::Items;
use crate::net::{Guid, NetEntity, ObjectStore};

/// Local templates live up here: armour at `ARMOUR | display`, weapons at `WEAPON | display`.
const ARMOUR: u32 = 0x00F0_0000;
const WEAPON: u32 = 0x00E0_0000;

const FIELD_TYPE: u16 = 2;
const FIELD_SCALE: u16 = 4;
const FIELD_HEALTH: u16 = 22;
const FIELD_MAX_HEALTH: u16 = 28;
const FIELD_LEVEL: u16 = 34;
const FIELD_FACTION: u16 = 35;
const FIELD_BYTES_0: u16 = 36;
const FIELD_DISPLAY: u16 = 131;
const FIELD_NATIVE_DISPLAY: u16 = 132;
const FIELD_BYTES_2: u16 = 164;
const FIELD_PLAYER_BYTES: u16 = 193;
const FIELD_PLAYER_BYTES_2: u16 = 194;
/// `PLAYER_VISIBLE_ITEM_1_0`: the entry worn in equipment slot `s` is at `+ 12 * s`.
const FIELD_VISIBLE_ITEM: u16 = 260;
/// `TYPEMASK_OBJECT | TYPEMASK_UNIT | TYPEMASK_PLAYER`.
const TYPE_PLAYER: u32 = 0x19;

/// The equipment slot and inventory type each [`survivors_sim::Loadout::gear`] entry wears in:
/// head, shoulders, chest, waist, legs, feet, wrists, hands, main hand, off hand, back.
const GEAR_EQUIP: [(u16, u32); GEAR_SLOTS] = [
    (0, 1),
    (2, 3),
    (4, 5),
    (5, 6),
    (6, 7),
    (7, 8),
    (8, 9),
    (9, 10),
    (15, 21),
    (16, 22),
    (14, 16),
];

/// Who to draw.
pub(super) struct HeroBody<'a> {
    pub serial: u64,
    pub class: &'static ClassDef,
    pub look: Appearance,
    /// `ItemDisplayInfo` ids worn, by [`GEAR_EQUIP`] order, 0 for bare.
    pub gear: [u32; GEAR_SLOTS],
    pub body: u32,
    pub pos: Vec3,
    pub items: &'a mut Items,
}

fn template(display: u32, inventory_type: u32) -> ItemInfo {
    ItemInfo {
        class: 4,
        subclass: 0,
        name: String::new(),
        display_info_id: display,
        quality: 4,
        flags: 0,
        buy_price: 0,
        sell_price: 0,
        inventory_type,
        allowable_class: -1,
        allowable_race: -1,
        item_level: 0,
        required_level: 0,
        required_skill: 0,
        required_skill_rank: 0,
        required_spell: 0,
        required_honor_rank: 0,
        required_city_rank: 0,
        required_rep_faction: 0,
        required_rep_rank: 0,
        max_count: 0,
        stackable: 1,
        container_slots: 0,
        stats: Vec::new(),
        damages: Vec::new(),
        dmg_min: 0.0,
        dmg_max: 0.0,
        dmg_type: 0,
        armor: 0,
        resistances: [0; 6],
        delay_ms: 0,
        ammo_type: 0,
        ranged_mod_range: 0.0,
        spells: Vec::new(),
        spell_charges_0: 0,
        use_spell: None,
        bonding: 0,
        description: String::new(),
        page_text: 0,
        language_id: 0,
        page_material: 0,
        start_quest: 0,
        lock_id: 0,
        material: 0,
        sheath: 0,
        random_property: 0,
        block: 0,
        item_set: 0,
        max_durability: 0,
        area: 0,
        map: 0,
        bag_family: 0,
    }
}

/// The entry a worn armour display goes by, its template made if new.
fn armour_entry(items: &mut Items, display: u32, inventory_type: u32) -> u32 {
    let entry = ARMOUR | (display & 0x000F_FFFF);
    if items.template_cached(entry).is_none() {
        items.insert_template(entry, Some(template(display, inventory_type)));
    }
    entry
}

/// The entry a class weapon goes by: the class's own `(slot, display, class, subclass, material,
/// inventory type, sheath)`.
fn weapon_entry(items: &mut Items, w: &Weapon) -> u32 {
    let &(_, display, class, sub, material, inv, sheath) = w;
    let entry = WEAPON | (display & 0x000F_FFFF);
    if items.template_cached(entry).is_none() {
        let mut t = template(display, u32::from(inv));
        t.class = u32::from(class);
        t.subclass = u32::from(sub);
        t.material = u32::from(material);
        t.sheath = u32::from(sheath);
        items.insert_template(entry, Some(t));
    }
    entry
}

/// The worn-item fields for `gear` and the class's weapons.
fn worn_pairs(items: &mut Items, class: &ClassDef, gear: &[u32; GEAR_SLOTS]) -> Vec<(u16, u32)> {
    let mut pairs = Vec::new();
    // The armour and the cloak; the hands are the weapons'.
    for (i, &(slot, inv)) in GEAR_EQUIP.iter().enumerate() {
        if (15..=16).contains(&slot) {
            continue;
        }
        let entry = if gear[i] != 0 {
            armour_entry(items, gear[i], inv)
        } else {
            0
        };
        pairs.push((FIELD_VISIBLE_ITEM + 12 * slot, entry));
    }
    for w in class.weapons {
        // The class tables number weapons 0 main hand, 1 off hand, 2 ranged. A dropped weapon is
        // the class's own kind in another look.
        let slot = 15 + w.0;
        let mut w = *w;
        if w.0 == 0 && gear[8] != 0 {
            w.1 = gear[8];
        }
        pairs.push((FIELD_VISIBLE_ITEM + 12 * slot, weapon_entry(items, &w)));
    }
    pairs
}

/// Spawn a hero as a player character; the caller adds its own components.
pub(super) fn spawn_player(commands: &mut Commands, b: HeroBody) -> Entity {
    let guid = b.serial;
    let l = b.look;
    let bytes0 = u32::from(l.race)
        | u32::from(b.class.id) << 8
        | u32::from(l.sex) << 16
        | u32::from(b.class.power) << 24;
    let mut pairs: Vec<(u16, u32)> = vec![
        (0, guid as u32),
        (1, (guid >> 32) as u32),
        (FIELD_TYPE, TYPE_PLAYER),
        (FIELD_SCALE, 1.0f32.to_bits()),
        (FIELD_HEALTH, 100),
        (FIELD_MAX_HEALTH, 100),
        (FIELD_LEVEL, 60),
        (FIELD_FACTION, FACTION_FRIENDLY),
        (FIELD_BYTES_0, bytes0),
        (FIELD_DISPLAY, b.body),
        (FIELD_NATIVE_DISPLAY, b.body),
        (FIELD_BYTES_2, u32::from(b.class.sheath)),
        (
            FIELD_PLAYER_BYTES,
            u32::from(l.skin)
                | u32::from(l.face) << 8
                | u32::from(l.hair) << 16
                | u32::from(l.hair_color) << 24,
        ),
        (FIELD_PLAYER_BYTES_2, u32::from(l.facial)),
    ];
    pairs.extend(worn_pairs(b.items, b.class, &b.gear));
    commands
        .spawn((
            Guid(guid),
            NetEntity {
                kind: benilla_protocol::EntityKind::Player,
                display_id: Some(b.body),
                scale: 1.0,
            },
            ObjectStore(ObjectFields::from_pairs(&pairs)),
            crate::creature_anim::MovementState::default(),
            crate::net::ScriptedFacing,
            Transform::from_translation(b.pos),
            Visibility::default(),
            RunEntity,
        ))
        .id()
}

/// Re-dress a standing hero in `gear`: the worn fields change and the body redraws.
pub(super) fn redress(
    store: &mut ObjectStore,
    items: &mut Items,
    class: &ClassDef,
    gear: &[u32; GEAR_SLOTS],
) {
    let pairs = worn_pairs(items, class, gear);
    store.0.merge(ObjectFields::from_pairs(&pairs));
}
