use assert_cmd::Command;

#[test]
fn cli_invokes_with_matcher() {
    let mut cmd = Command::cargo_bin("qk").unwrap();
    cmd.arg("no-such-pod").assert().failure();
}