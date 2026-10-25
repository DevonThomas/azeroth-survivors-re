//! Survey the client's item art for armour families: every `ItemDisplayInfo` row whose model or
//! texture names contain a pattern, with its icon, so a gear table can be built from the art the
//! player's own client has. `cargo run -p survivors-server --example gear_survey -- <Data> RaidMage`.

fn main() {
    let mut args = std::env::args().skip(1);
    let dir = args.next().expect("usage: gear_survey <WoW Data dir> <pattern>…");
    let pats: Vec<String> = args.map(|a| a.to_ascii_lowercase()).collect();
    let mut chain = benilla_formats::Chain::open(std::path::Path::new(&dir)).expect("patch chain");
    let displays = benilla_formats::load_item_display_catalog(&mut chain).expect("ItemDisplayInfo");
    let mut rows: Vec<(u32, String)> = displays
        .iter()
        .filter_map(|(id, d)| {
            let names: Vec<&str> = d
                .model
                .iter()
                .chain(d.model_texture.iter())
                .chain(d.region_textures.iter())
                .flatten()
                .map(String::as_str)
                .collect();
            let joined = names.join(" ");
            let low = joined.to_ascii_lowercase();
            pats.iter().any(|p| low.contains(p)).then(|| {
                let icon = d.icon.as_deref().unwrap_or("").rsplit('\\').next().unwrap_or("");
                (id, format!("{icon}\t{joined}"))
            })
        })
        .collect();
    rows.sort_by_key(|r| r.0);
    for (id, line) in rows {
        println!("{id}\t{line}");
    }
}
