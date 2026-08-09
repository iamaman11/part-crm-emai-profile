use std::path::Path;

pub fn remove_test_root(path: &Path) -> Result<(), std::io::Error> {
    std::fs::remove_dir_all(path)
}
