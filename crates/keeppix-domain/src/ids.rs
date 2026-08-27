use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

macro_rules! id_type {
    ($name:ident) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(Uuid);

        impl $name {
            #[must_use]
            pub fn new() -> Self {
                Self(Uuid::now_v7())
            }

            #[must_use]
            pub const fn from_uuid(id: Uuid) -> Self {
                Self(id)
            }

            #[must_use]
            pub const fn as_uuid(&self) -> Uuid {
                self.0
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.0)
            }
        }

        impl FromStr for $name {
            type Err = uuid::Error;
            fn from_str(s: &str) -> Result<Self, Self::Err> {
                Ok(Self(Uuid::from_str(s)?))
            }
        }
    };
}

id_type!(UserId);
id_type!(GroupId);
id_type!(AlbumId);
id_type!(TagId);
id_type!(LibraryId);
id_type!(FolderId);
id_type!(AssetId);
id_type!(BatchId);
id_type!(StackId);
id_type!(TrashEntryId);
id_type!(UploadSessionId);
id_type!(OperationId);
// `SessionId` identifies a family of refresh tokens (`sessions.family_id`),
// i.e. a device/login from the user's point of view: the same identity
// persists across every rotation (`SessionRepo::rotate`) without changing,
// unlike the row id (`sessions.id`), which changes on every rotation.
id_type!(SessionId);
// `FaceId` identifies a detection on ONE asset, `PersonId` an identity that
// persists over time across multiple assets and multiple detections.
id_type!(FaceId);
id_type!(PersonId);
id_type!(PersonGroupId);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_time_ordered() {
        let a = UserId::new();
        let b = UserId::new();
        assert!(a.as_uuid() < b.as_uuid(), "UUID v7 must be monotonic");
    }

    #[test]
    #[allow(clippy::expect_used)]
    fn id_roundtrips_through_string() {
        let a = UserId::new();
        assert_eq!(a, a.to_string().parse().expect("valid UUID string"));
    }
}
