//! One worked example of the round-1-bound prologue, for an implementation that does not link this crate.
//!
//! Regenerate with:
//! `cargo test --test prologue_vector -- --ignored --nocapture print_vector`

use frost_vault_core::seal::ceremony_prologue_with_transcript;

fn statics() -> Vec<(u8, Vec<u8>)> {
    vec![(0, vec![0x11; 32]), (1, vec![0x12; 32]), (2, vec![0x13; 32])]
}

fn round1() -> Vec<(u8, Vec<u8>)> {
    vec![(0, vec![0xa0; 40]), (1, vec![0xa1; 40]), (2, vec![0xa2; 40])]
}

#[test]
#[ignore]
fn print_vector() {
    let prologue = ceremony_prologue_with_transcript(&statics(), &round1());
    println!("length = {}", prologue.len());
    println!("hex = {}", prologue.iter().map(|b| format!("{b:02x}")).collect::<String>());
}

/// The published bytes. Pinned rather than printed: a test that only checks "shuffling does not matter"
/// keeps passing after the layout changes, which is exactly when a second implementation stops matching.
///
/// The domain string `noir-frost-vault/round1-transcript/v1` is 37 bytes — easy to miscount, and a
/// hand-copied version of this hex was once one byte short.
const EXPECTED: &str = "\
030020111111111111111111111111111111111111111111111111111111111111111101201212121212121212121212\
121212121212121212121212121212121212121212022013131313131313131313131313131313131313131313131313\
131313131313136e6f69722d66726f73742d7661756c742f726f756e64312d7472616e7363726970742f763103000000\
0028a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a00100000028a1\
a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a10200000028a2a2a2a2\
a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2";

#[test]
fn the_published_vector_is_byte_exact() {
    let prologue = ceremony_prologue_with_transcript(&statics(), &round1());
    assert_eq!(prologue.len(), 276);
    let hex: String = prologue.iter().map(|b| format!("{b:02x}")).collect();
    assert_eq!(hex, EXPECTED);
    // The middle field is the domain string, raw UTF-8 with no length prefix.
    let domain = b"noir-frost-vault/round1-transcript/v1";
    assert_eq!(domain.len(), 37);
    assert!(prologue.windows(domain.len()).any(|w| w == domain));
}

#[test]
fn the_vector_is_stable() {
    // Shuffled input must produce the same bytes: the encoding sorts by slot.
    let mut shuffled = round1();
    shuffled.reverse();
    let mut shuffled_statics = statics();
    shuffled_statics.reverse();
    assert_eq!(
        ceremony_prologue_with_transcript(&statics(), &round1()),
        ceremony_prologue_with_transcript(&shuffled_statics, &shuffled)
    );

    // Dropping a participant changes it.
    assert_ne!(
        ceremony_prologue_with_transcript(&statics(), &round1()[..2].to_vec()),
        ceremony_prologue_with_transcript(&statics(), &round1())
    );
}
