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
id_type!(LibraryId);
id_type!(FolderId);
id_type!(AssetId);
id_type!(BatchId);
id_type!(StackId);
id_type!(TrashEntryId);
id_type!(UploadSessionId);
// `SessionId` identifica una famiglia di refresh token (`sessions.family_id`),
// cioè un dispositivo/login dal punto di vista dell'utente: la stessa
// identità attraversa ogni rotazione (`SessionRepo::rotate`) senza cambiare,
// a differenza dell'id di riga (`sessions.id`), che cambia a ogni rotazione.
id_type!(SessionId);

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
