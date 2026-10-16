//! The riff of peers: Wave B of riff 3.0 (#803). Two or more people
//! share one riff, and no server holds it. The book page "riff 3.0:
//! peers" has the picture.
//!
//! # Design
//!
//! Each change of the riff is a signed [`record::Record`] of one daemon,
//! its author. The records of one author make one chain. Each daemon
//! keeps the bytes of each record, and the state of the riff comes from
//! the set of records.
//!
//! | Part | Module |
//! |---|---|
//! | The bytes, the signature, the ID, the chain and the checks of a record | [`record`] |
//! | The JSON body of each type of record, its writer and its largest size | [`body`] |

pub mod body;
pub mod record;
