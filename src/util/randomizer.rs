use data_encoding::BASE32;
use rand::Rng;

pub fn generate_random_base32(length: usize) -> String {
    let mut rng = rand::rng();
    let mut bytes = vec![0u8; length];
    rng.fill_bytes(&mut bytes);
    BASE32.encode(&bytes).replace("=", "")
}
