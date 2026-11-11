//! Identifier newtypes for the v2 preview contract spine.

use serde::{Deserialize, Serialize};

macro_rules! string_newtype {
    (
        $(#[$meta:meta])*
        $name:ident
    ) => {
        $(#[$meta])*
        #[derive(
            Debug, Clone, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
        )]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            pub fn as_str(&self) -> &str {
                &self.0
            }

            pub fn into_inner(self) -> String {
                self.0
            }
        }

        impl From<&str> for $name {
            fn from(s: &str) -> Self {
                Self(s.to_owned())
            }
        }

        impl From<String> for $name {
            fn from(s: String) -> Self {
                Self(s)
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(&self.0)
            }
        }
    };
}

string_newtype!(
    /// Stable v2 operation identifier.
    OperationId
);

string_newtype!(
    /// Stable event identifier within an operation stream.
    OperationEventId
);

string_newtype!(
    /// Correlation identifier shared across related operations.
    CorrelationId
);

string_newtype!(
    /// Optional causality identifier linking an operation to a parent operation/event.
    CausalityId
);

string_newtype!(
    /// Public actor identifier for audit and policy context.
    ActorId
);

string_newtype!(
    /// Tenant or workspace identifier.
    TenantId
);

string_newtype!(
    /// Artifact reference identifier.
    ArtifactId
);

string_newtype!(
    /// Batch reference identifier.
    BatchId
);

string_newtype!(
    /// Fact-set reference identifier.
    FactSetId
);

string_newtype!(
    /// Plan reference identifier.
    PlanId
);

string_newtype!(
    /// Packet reference identifier.
    PacketId
);

string_newtype!(
    /// Schema reference identifier.
    SchemaId
);

string_newtype!(
    /// Capability Catalog entry identifier.
    CapabilityId
);

string_newtype!(
    /// Runtime route reference identifier.
    RouteId
);
