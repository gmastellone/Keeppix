//! Common limits for batch requests (metadata, flags).

use keeppix_domain::AssetId;

use crate::problem::Problem;

/// Hard ceiling on the HTTP side. `apply_batch` is measured to run in under
/// a second at 5,000; 10,000 leaves margin without opening the door to
/// multi-megabyte request vectors.
pub const MAX_BATCH_ASSETS: usize = 10_000;

/// # Errors
/// `400` `keeppix/batch-too-large` if `ids.len()` exceeds [`MAX_BATCH_ASSETS`].
pub fn reject_oversized_batch(ids: &[AssetId]) -> Result<(), Problem> {
    if ids.len() > MAX_BATCH_ASSETS {
        return Err(Problem::bad_request(
            "batch-too-large",
            "batch exceeds the maximum number of assets",
        )
        .with_detail(format!(
            "got {} asset ids; maximum is {MAX_BATCH_ASSETS}",
            ids.len()
        )));
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn accepts_batches_at_the_limit() {
        let ids: Vec<AssetId> = (0..MAX_BATCH_ASSETS).map(|_| AssetId::new()).collect();
        assert!(reject_oversized_batch(&ids).is_ok());
    }

    #[test]
    fn rejects_batches_above_the_limit() {
        let ids: Vec<AssetId> = (0..=MAX_BATCH_ASSETS).map(|_| AssetId::new()).collect();
        let err = reject_oversized_batch(&ids).expect_err("must reject");
        assert_eq!(err.status, axum::http::StatusCode::BAD_REQUEST.as_u16());
        assert!(err.type_slug.contains("batch-too-large"));
    }
}
