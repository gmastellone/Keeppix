#![allow(clippy::unwrap_used)]

#[test]
fn probe_does_not_claim_software_was_measured() {
    let caps = keeppix_media::probe();
    assert_eq!(
        caps.backend, "unprobed",
        "senza un rilevamento hardware il backend non può affermare 'software'"
    );
    assert!(caps.decode_fps.is_none());
}
