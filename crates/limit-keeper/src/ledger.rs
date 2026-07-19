use {
    anyhow::{Context, Result},
    std::{fs, path::Path},
};

pub fn load_cursor(path: impl AsRef<Path>) -> Result<Option<u32>> {
    let path = path.as_ref();
    match fs::read_to_string(path) {
        Ok(value) => Ok(Some(value.trim().parse().context("parse keeper cursor")?)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error).with_context(|| format!("read keeper cursor {}", path.display())),
    }
}

pub fn save_cursor(path: impl AsRef<Path>, ledger: u32) -> Result<()> {
    let path = path.as_ref();
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent).with_context(|| format!("create cursor directory {}", parent.display()))?;
    }
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, format!("{ledger}\n")).with_context(|| format!("write keeper cursor {}", tmp.display()))?;
    fs::rename(&tmp, path).with_context(|| format!("replace keeper cursor {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::{load_cursor, save_cursor};

    #[test]
    fn saves_and_loads_cursor() {
        let path = std::env::temp_dir().join(format!("limit-keeper-{}.cursor", std::process::id()));
        save_cursor(&path, 123).unwrap();
        assert_eq!(load_cursor(&path).unwrap(), Some(123));
        std::fs::remove_file(path).unwrap();
    }
}
