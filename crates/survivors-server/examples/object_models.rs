//! List `GameObjectDisplayInfo` rows whose model path holds a pattern:
//! `cargo run -p survivors-server --example object_models -- <Data> coin`.

fn main() {
    let mut args = std::env::args().skip(1);
    let dir = args.next().expect("usage: object_models <WoW Data dir> <pattern>…");
    let pats: Vec<String> = args.map(|a| a.to_ascii_lowercase()).collect();
    let mut chain = benilla_formats::Chain::open(std::path::Path::new(&dir)).expect("patch chain");
    let catalog = benilla_formats::load_gameobject_catalog(&mut chain).expect("GameObjectDisplayInfo");
    let mut rows: Vec<(u32, String)> = catalog
        .iter()
        .filter(|(_, m)| pats.iter().any(|p| m.to_ascii_lowercase().contains(p)))
        .map(|(i, m)| (i, m.to_string()))
        .collect();
    rows.sort();
    for (id, m) in rows {
        println!("{id}\t{m}");
    }
}
