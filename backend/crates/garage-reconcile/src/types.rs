//! Stable plan values shared by the human and JSON renderers.

use serde::Serialize;

/// One path retained after package-owner filtering.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DesiredPath {
    /// `$HOME`-relative path.
    pub path: String,
    /// Manifest kind, using its on-disk spelling.
    pub kind: String,
    /// Package owner when the manifest names one.
    pub owner: Option<String>,
}

/// One unit declaration reported but never enforced.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Unit {
    /// Unit name including suffix.
    pub name: String,
    /// `running` or `oneshot`.
    pub kind: String,
}

/// The five stow outcomes, counted over desired stow leaves.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ActualState {
    /// Live links into this checkout.
    pub linked: usize,
    /// Live links into another Garage checkout.
    pub other: usize,
    /// Dangling or unrelated links.
    pub broken: usize,
    /// Real paths where links belong.
    pub plain: usize,
    /// Absent paths.
    pub missing: usize,
}

/// A filesystem operation that would converge one managed path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Action {
    /// Create a missing link.
    Link,
    /// Replace an existing symlink with one into this checkout.
    Relink,
    /// Move a plain path into the backup tree, then link it.
    BackupAndLink,
}

/// One ordered filesystem change in a plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PlanItem {
    /// Operation to perform.
    pub action: Action,
    /// `$HOME`-relative target.
    pub path: String,
    /// Why the current state requires the operation.
    pub reason: String,
    /// Checkout source the target will link to.
    pub source: String,
    /// Backup destination for [`Action::BackupAndLink`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backup: Option<String>,
    /// Manifest kind written to the future ledger.
    pub kind: String,
    /// Package owner written to the future ledger.
    pub owner: Option<String>,
}

/// The read-only desired/actual diff.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Diff {
    /// Every desired path, including non-stow paths which are reported but not linked.
    pub desired: Vec<DesiredPath>,
    /// Units declared by `units.list`; reporting only.
    pub units: Vec<Unit>,
    /// Current stow outcome counts.
    pub actual: ActualState,
    /// Changes needed to make every desired stow leaf link into this checkout.
    pub plan: Vec<PlanItem>,
}
