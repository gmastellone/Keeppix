use keeppix_domain::Job;

use crate::JobError;

/// Un handler per tipo di job. I tipi concreti arrivano dai task 6–10.
pub trait JobHandler: Send + Sync {
    fn ram_hint_bytes(&self, job: &Job) -> u64;

    fn handle(&self, job: &Job) -> impl std::future::Future<Output = Result<(), JobError>> + Send;
}

/// Stima di default: 64 MiB, abbastanza per un JPEG 20 MP decodificato.
pub const DEFAULT_RAM_HINT: u64 = 64 * 1024 * 1024;

#[must_use]
pub fn ram_hint_for_image(width: Option<i32>, height: Option<i32>) -> u64 {
    match (width, height) {
        (Some(w), Some(h)) if w > 0 && h > 0 => {
            u64::try_from(w)
                .unwrap_or(0)
                .saturating_mul(u64::try_from(h).unwrap_or(0))
                * 3
        }
        _ => DEFAULT_RAM_HINT,
    }
}
