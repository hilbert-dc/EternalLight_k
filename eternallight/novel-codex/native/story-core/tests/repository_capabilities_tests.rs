//! Keep the connection owner from regrowing an implicit, all-domain public API.
use std::path::Path;
use syn::{ImplItem, ItemImpl, Visibility, visit::Visit};

const FACTORIES: &[&str] = &[
    "candidates",
    "quality",
    "conversation_store",
    "task_intent_store",
    "review_store",
    "work_execution",
];

fn violations(source: &str) -> Vec<String> {
    #[derive(Default)]
    struct Check(Vec<String>);
    impl<'ast> Visit<'ast> for Check {
        fn visit_item_impl(&mut self, item: &'ast ItemImpl) {
            if item.trait_.is_none()
                && matches!(&*item.self_ty, syn::Type::Path(p) if p.path.is_ident("SqliteStoryRepository"))
            {
                for member in &item.items {
                    let ImplItem::Fn(method) = member else {
                        continue;
                    };
                    if matches!(method.vis, Visibility::Public(_))
                        && method.sig.receiver().is_some()
                        && !FACTORIES.contains(&method.sig.ident.to_string().as_str())
                    {
                        self.0.push(method.sig.ident.to_string());
                    }
                }
            }
            syn::visit::visit_item_impl(self, item);
        }
    }
    let mut check = Check::default();
    check.visit_file(&syn::parse_file(source).expect("valid Rust source"));
    check.0
}

fn walk(path: &Path, failures: &mut Vec<String>) {
    for entry in std::fs::read_dir(path).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            walk(&path, failures);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            for method in violations(&std::fs::read_to_string(&path).unwrap()) {
                failures.push(format!("{}: {method}", path.display()));
            }
        }
    }
}

#[test]
fn sqlite_owner_exposes_factories_and_explicit_capabilities_only() {
    let mut failures = Vec::new();
    walk(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
        &mut failures,
    );
    assert!(
        failures.is_empty(),
        "Declare a focused repository capability instead:\n{}",
        failures.join("\n")
    );
}

#[test]
fn rejects_new_business_methods_including_nested_modules() {
    assert_eq!(
        violations(
            r#"
        mod nested { impl SqliteStoryRepository {
            pub fn save_everything(&mut self) {}
            pub(crate) fn transaction_helper(&self) {}
        }}
    "#
        ),
        ["save_everything"]
    );
}

#[test]
fn accepts_explicit_capabilities_and_connection_construction() {
    assert!(
        violations(
            r#"
        impl SqliteStoryRepository {
            pub fn open(path: &str) -> Self { todo!() }
            pub fn candidates(&mut self) -> CandidateStore<'_> { todo!() }
        }
        impl ChapterRepository for SqliteStoryRepository {
            fn chapter(&self, id: &str) { todo!() }
        }
    "#
        )
        .is_empty()
    );
}
