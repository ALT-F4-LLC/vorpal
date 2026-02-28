pub mod archives;
pub mod hashes;
pub mod notary;
pub mod paths;
pub mod temps;

/// Buffer size for the duplex channel bridging async and blocking I/O.
pub const DUPLEX_BUF_SIZE: usize = 256 * 1024;
