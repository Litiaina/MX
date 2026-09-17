use std::fs::File;
use std::io::{self, Write};

pub fn create_env_file(path: &str) -> io::Result<()> {
    let mut file = File::create(path)?;

    writeln!(file, "JWT_SECRET=")?;
    writeln!(file, "AUTH_KEYS=")?;

    // Secret for the dedicated N1 ARIS fragment.
    writeln!(file, "N1_ARIS_SECRET=")?;

    Ok(())
}
