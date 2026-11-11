//! Where a prompt came from, as the engine's `origin.kind` names it.

/// Typed into the input box (`composer`) or sent by a person over remote
/// control (`bridge`). Every other kind, and one this version does not know,
/// is not.
pub fn is_person(kind: &str) -> bool {
    matches!(kind, "composer" | "bridge")
}
