#![allow(clippy::expect_used)]

use keeppix_media::sandbox;

#[test]
fn a_child_exit_does_not_kill_the_parent() {
    let out = sandbox::run("true", &[] as &[&str], 8 * 1024 * 1024, 1).expect("spawn true");
    assert!(out.status.success());
    let out = sandbox::run("false", &[] as &[&str], 8 * 1024 * 1024, 1).expect("spawn false");
    assert!(!out.status.success());
    assert_eq!(2 + 2, 4, "il padre è ancora vivo");
}
