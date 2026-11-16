//! Shared reader for rekordbox and Serato USB libraries.
//!
//! The desktop app and the wall player both depend on this crate, so there is
//! one copy of the library format code.

#[cfg(test)]
mod tests {
    #[test]
    fn the_crate_builds() {
        assert_eq!(env!("CARGO_PKG_NAME"), "djusb-core");
    }
}
