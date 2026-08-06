#[derive(Debug)]
pub struct GenerationDek([u8; 32]);

pub fn leak_key(key: &GenerationDek) {
    println!("{key:?}");
}
