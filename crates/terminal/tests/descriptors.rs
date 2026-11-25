//! Alone in its own test binary: other tests running in parallel would change the descriptor count.
#![cfg(target_os = "linux")]

mod common;

#[test]
fn repeated_spawn_and_close_releases_descriptors() {
    let open_descriptors = || std::fs::read_dir("/proc/self/fd").unwrap().count();
    let cycle = || {
        let (mut session, wake) = common::spawn(common::options("exit 0", &[]));
        common::wait_for(&mut session, &wake, |_, update| update.exit_status);
        session.close().unwrap();
    };
    // The first session may open process-wide descriptors that stay open by design.
    cycle();
    let start = open_descriptors();
    for _ in 0..20 {
        cycle();
    }
    assert_eq!(open_descriptors(), start);
}
