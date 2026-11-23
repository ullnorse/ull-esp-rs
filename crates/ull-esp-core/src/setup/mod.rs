//! The pieces of setup mode that need no hardware.
//!
//! In setup mode the board opens its own Wi-Fi network. A phone joins it,
//! gets an address from [`dhcp`], has every name answered with the board's
//! address by [`dns`] (so the phone shows the setup page by itself, like a
//! hotel sign-in page), and fills in the form that [`page`] renders and
//! [`form`] reads back.

pub mod dhcp;
pub mod dns;
pub mod form;
pub mod page;
