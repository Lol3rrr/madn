mod rng;
pub use rng::MockRand;

mod socket;
pub use socket::MockSocket;

pub struct DumbDistr {}

impl rand::distributions::Distribution<usize> for DumbDistr {
    fn sample<R: rand::Rng + ?Sized>(&self, rng: &mut R) -> usize {
        let raw: usize = rng.gen();
        (raw % 6) + 1
    }
}
