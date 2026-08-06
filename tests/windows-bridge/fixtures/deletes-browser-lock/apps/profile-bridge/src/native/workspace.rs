// Deliberately forbidden fixture: browser runtime lock files are never deleted blindly.
fn delete_browser_lock() {
    let _ = std::fs::remove_file("parent.lock");
}
