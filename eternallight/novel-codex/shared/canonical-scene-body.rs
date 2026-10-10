/// The canonical chapter body for already shape-validated scene bodies.
/// Schema, target, scope and storage validation remain with their callers.
pub fn canonical_scene_body<'a>(bodies: impl IntoIterator<Item = &'a str>) -> String {
    bodies.into_iter().map(str::trim).collect::<Vec<_>>().join("\n\n---\n\n")
}
