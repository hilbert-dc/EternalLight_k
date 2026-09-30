/// Bounded source-to-target review rules, not inference of new Canon facts.
pub(crate) const DEPENDENCY_RULE_VERSION: &str = "explicit-dependencies-v1";

pub(crate) const DEPENDENCY_RELATIONS: &[&str] = &[
    "causes",
    "enables",
    "prevents",
    "motivates",
    "foreshadows",
    "paysOff",
    "reveals",
    "inputDependency",
];

#[derive(Clone, Copy)]
pub(crate) enum RelationTraversalPolicy {
    AllRelations,
    ExplicitDependencies,
}
