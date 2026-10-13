//! Recovery domain: current public contracts and transaction-preserving persistence.

pub(crate) mod recovery_operation_read;
pub(crate) mod recovery_operation_reconcile;
pub(crate) mod recovery_operations;
pub(crate) mod recovery_snapshot;

pub use recovery_operations::RecoveryOperationBinding;

pub use recovery_operation_reconcile::RecoveryOperationReconcileRepository;

pub use recovery_snapshot::RecoverySnapshotRepository;

pub use recovery_operations::RecoveryOperationsRepository;

pub use recovery_operation_read::RecoveryOperationReadRepository;
