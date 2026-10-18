//! Stable, `u64`-backed identifiers for project entities.
//!
//! Ids carry explicit values (stored in the project file), never anything
//! random, so that offline renders stay deterministic (SPEC §5).

/// Defines one or more `u64`-backed identifier newtypes with a shared shape.
macro_rules! define_ids {
    ($($(#[$meta:meta])* $name:ident),+ $(,)?) => {
        $(
            $(#[$meta])*
            #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
            pub struct $name(u64);

            impl $name {
                #[doc = concat!("Creates a `", stringify!($name), "` from a raw value.")]
                #[must_use]
                pub const fn new(value: u64) -> Self {
                    Self(value)
                }

                #[doc = concat!("Returns the raw value of this `", stringify!($name), "`.")]
                #[must_use]
                pub const fn get(self) -> u64 {
                    self.0
                }
            }
        )+
    };
}

define_ids! {
    /// Identifies a pattern (a grid of channels × steps).
    PatternId,
    /// Identifies a sequencer channel / instrument.
    ChannelId,
    /// Identifies an audio track.
    TrackId,
    /// Identifies a clip (a placed audio take or pattern instance).
    ClipId,
    /// Identifies a note in the piano roll.
    NoteId,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_roundtrip_and_compare() {
        assert_eq!(PatternId::new(42).get(), 42);
        assert_eq!(ChannelId::new(0), ChannelId::new(0));
        assert_ne!(TrackId::new(1), TrackId::new(2));
        assert!(NoteId::new(1) < NoteId::new(2));
    }
}
