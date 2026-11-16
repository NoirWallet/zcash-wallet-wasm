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
