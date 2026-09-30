#[derive(Clone, Copy)]
pub(crate) struct RelationTypeDefinition {
    pub category: &'static str,
    pub inverse: Option<&'static str>,
    pub transitive: bool,
    pub source_kinds: &'static [&'static str],
    pub target_kinds: &'static [&'static str],
}

const fn definition(
    category: &'static str,
    inverse: Option<&'static str>,
    source_kinds: &'static [&'static str],
    target_kinds: &'static [&'static str],
) -> RelationTypeDefinition {
    RelationTypeDefinition {
        category,
        inverse,
        transitive: false,
        source_kinds,
        target_kinds,
    }
}
const EPISTEMIC: RelationTypeDefinition = definition(
    "epistemic",
    /*inverse*/ None,
    &["character"],
    &["proposition"],
);
const CAUSAL: RelationTypeDefinition = definition(
    "causal",
    /*inverse*/ None,
    &["event", "proposition"],
    &["event", "proposition"],
);
const NARRATIVE: RelationTypeDefinition = definition(
    "narrative",
    /*inverse*/ None,
    &["event", "proposition"],
    &["event", "proposition"],
);
const EVIDENCE: RelationTypeDefinition = definition(
    "evidence",
    /*inverse*/ None,
    &["proposition"],
    &["proposition"],
);

pub(crate) const RELATION_TYPES: &[(&str, RelationTypeDefinition)] = &[
    (
        "memberOf",
        definition(
            "world",
            Some("hasMember"),
            &["character"],
            &["organization"],
        ),
    ),
    (
        "locatedAt",
        definition(
            "world",
            Some("contains"),
            &["character", "organization", "item", "event"],
            &["location"],
        ),
    ),
    (
        "owns",
        definition(
            "world",
            Some("ownedBy"),
            &["character", "organization"],
            &["item"],
        ),
    ),
    (
        "parentOf",
        definition("world", Some("childOf"), &["character"], &["character"]),
    ),
    ("knows", EPISTEMIC),
    ("believes", EPISTEMIC),
    ("suspects", EPISTEMIC),
    ("conceals", EPISTEMIC),
    ("causes", CAUSAL),
    ("enables", CAUSAL),
    ("prevents", CAUSAL),
    ("motivates", CAUSAL),
    ("foreshadows", NARRATIVE),
    ("paysOff", NARRATIVE),
    ("contrasts", NARRATIVE),
    ("echoes", NARRATIVE),
    ("reveals", NARRATIVE),
    ("supports", EVIDENCE),
    ("contradicts", EVIDENCE),
    ("supersedes", EVIDENCE),
    ("inputDependency", EVIDENCE),
];

pub(crate) fn relation_definition(name: &str) -> Option<RelationTypeDefinition> {
    RELATION_TYPES
        .iter()
        .find(|(candidate, _)| *candidate == name)
        .map(|(_, definition)| *definition)
}
