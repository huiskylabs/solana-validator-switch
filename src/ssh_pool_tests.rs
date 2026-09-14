#[cfg(test)]
mod tests {
    //! Source-structure guards for the SSH session pool (`ssh.rs`).
    //!
    //! These lock in the reliability fixes for the failure mode where a
    //! Meshmap backup node stayed reachable (fresh SSH + local getHealth both
    //! worked) yet `svs` reported getHealth/swap-readiness failing for hours
    //! with "the remote process has terminated". The pool was handing back a
    //! half-alive multiplex master and not recovering.

    /// The pool must probe cached sessions through the same shell + stdout path
    /// real commands use, not a bare `true`. A half-alive multiplex master
    /// passes `true` while `bash -c "..."` commands fail with
    /// "the remote process has terminated"; that false-positive is what wedged
    /// monitoring for hours.
    #[test]
    fn test_liveness_probe_uses_shell_and_marker() {
        let src = include_str!("ssh.rs");
        assert!(
            src.contains("fn probe_session_alive"),
            "probe_session_alive must exist"
        );
        assert!(
            src.contains("__svs_alive__"),
            "liveness probe must assert on an echoed marker so it exercises stdout"
        );
        // Guard against regressing to the trivial probe that caused the bug.
        assert!(
            !src.contains("session.command(\"true\").output()"),
            "liveness probe must not fall back to a bare `true` exec"
        );
    }

    /// Reconnects must be single-flighted per connection key so the several
    /// concurrent per-node background loops don't stampede and build a pile of
    /// competing SSH masters while a node is unhealthy.
    #[test]
    fn test_reconnect_is_single_flighted() {
        let src = include_str!("ssh.rs");
        assert!(
            src.contains("reconnect_locks"),
            "pool must hold per-connection-key reconnect locks"
        );
        assert!(
            src.contains("fn reconnect_lock_for"),
            "get_session must serialize reconnects via a per-key lock"
        );
    }

    /// getHealth/getIdentity curls must be time-bounded so a hung localhost RPC
    /// can't hold an SSH channel open and pile up while a node is unhealthy.
    #[test]
    fn test_rpc_curl_is_time_bounded() {
        let src = include_str!("validator_rpc.rs");
        assert!(
            src.contains("curl -s -m "),
            "RPC curl must pass an explicit -m (max-time) timeout"
        );
    }

    /// Shell detection must prefer bash over PowerShell. These validators are
    /// Linux; a node with pwsh installed but broken (crashes on every spawn)
    /// must not be driven via pwsh, which surfaces as "the remote process has
    /// terminated" for every command and blinds node monitoring.
    #[test]
    fn test_shell_detection_prefers_bash() {
        let src = include_str!("ssh.rs");
        let bash_at = src
            .find("svs_bash_ok")
            .expect("detect_remote_shell must probe bash first");
        let pwsh_at = src
            .find("PSVersionTable")
            .expect("PowerShell fallback must remain for Windows hosts");
        assert!(
            bash_at < pwsh_at,
            "detect_remote_shell must try bash before falling back to PowerShell"
        );
    }
}
