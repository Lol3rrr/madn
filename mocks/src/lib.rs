//! This Crate contains a collection of Mock/Test implementations used for testing
#![warn(missing_docs)]
mod rng;
pub use rng::MockRand;

mod socket;
pub use socket::MockSocket;

/// A simple Distribution that takes a Random Number and maps into onto 1-6
pub struct DumbDistr {}

impl rand::distributions::Distribution<usize> for DumbDistr {
    fn sample<R: rand::Rng + ?Sized>(&self, rng: &mut R) -> usize {
        let raw: usize = rng.gen();
        (raw % 6) + 1
    }
}
