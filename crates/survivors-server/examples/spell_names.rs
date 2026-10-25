//! Print the `Spell.dbc` name and rank of each spell id given, to check a table against the
//! client: `cargo run -p survivors-server --example spell_names -- <Data dir> 116 133 …`.

fn main() {
    let mut args = std::env::args().skip(1);
    let dir = args.next().expect("usage: spell_names <WoW Data dir> <spell id>…");
    let mut chain = benilla_formats::Chain::open(std::path::Path::new(&dir)).expect("patch chain");
    let catalog = benilla_formats::load_spell_catalog(&mut chain).expect("Spell.dbc");
    for id in args.filter_map(|a| a.parse::<u32>().ok()) {
        match catalog.get(id) {
            Some(d) => println!("{id}\t{}\t{}", d.name, d.rank.as_deref().unwrap_or("")),
            None => println!("{id}\t<missing>"),
        }
    }
}
