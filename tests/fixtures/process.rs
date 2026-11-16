fn main() {
    if std::env::args().any(|arg| arg == "--hold") {
        std::thread::sleep(std::time::Duration::from_secs(90));
    } else {
        let exe = std::env::current_exe().unwrap();
        std::fs::write(
            exe.with_extension("started"),
            if cfg!(fixture_old) { "old" } else { "new" },
        )
        .unwrap();
    }
}
