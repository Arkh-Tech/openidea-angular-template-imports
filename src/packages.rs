//! Discover exported Angular declarations in installed packages, not a mapping table.
//! Read exact files under node_modules: Project.find_files respects gitignore and
//! often does not index node_modules at all.
use crate::metadata::{self, Decl, Kind};
use openidea_plugin::Project;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug)]
pub struct ExportedDecl {
    pub declaration: Decl,
    pub public_symbol: String,
    pub module: String,
}

fn first_types(value: &Value) -> Option<String> {
    match value {
        Value::String(s) if s.ends_with(".d.ts") || s.ends_with(".d.mts") => Some(s.clone()),
        Value::Object(map) => {
            // Prefer the type condition to JS runtime paths.
            if let Some(x) = map.get("types").and_then(first_types) { return Some(x); }
            if let Some(x) = map.get("import").and_then(first_types) { return Some(x); }
            if let Some(x) = map.get("default").and_then(first_types) { return Some(x); }
            map.values().find_map(first_types)
        }
        _ => None,
    }
}
fn installed_entries(project: &Project, name: &str) -> Vec<(String, String)> {
    let Ok(text) = project.read_file(&format!("node_modules/{name}/package.json")) else { return vec![]; };
    let Ok(json) = serde_json::from_str::<Value>(&text) else { return vec![]; };
    let mut entries = Vec::new();
    let package_entry = json.get("exports").and_then(|e| e.get(".")).and_then(first_types)
        .or_else(|| json.get("types").and_then(first_types))
        .or_else(|| json.get("typings").and_then(first_types))
        .unwrap_or_else(|| "index.d.ts".into());
    entries.push((name.to_owned(), package_entry));
    if let Some(map) = json.get("exports").and_then(Value::as_object) {
        for (sub, detail) in map {
            if sub == "." || !sub.starts_with("./") || sub.contains('*') { continue; }
            if let Some(path) = first_types(detail) {
                entries.push((format!("{name}/{}", &sub[2..]), path));
            }
        }
    }
    entries.sort();
    entries.dedup();
    entries.truncate(100);
    entries
}
/// Used in regression tests only: recognizes complete test subpath segments.
#[cfg(test)]
fn test_only_entrypoint(package: &str, module: &str) -> bool {
    let Some(subpath) = module.strip_prefix(package).and_then(|s| s.strip_prefix('/')) else {
        return false;
    };
    subpath.split('/').any(is_test_dir)
}

pub(crate) fn is_test_dir(part: &str) -> bool {
    matches!(part, "testing" | "test" | "tests" | "__tests__" | "__mocks__"
        | "mocks" | "spec" | "specs" | "fixtures" | "e2e")
}

/// Test classification is informational in this version, not an exclusion.
#[cfg(test)]
fn test_only_source(path: &str) -> bool {
    let normalized = path.replace('\\', "/");
    let mut parts = normalized.split('/').peekable();
    while let Some(part) = parts.next() {
        if parts.peek().is_some() && is_test_dir(part) { return true; }
        if parts.peek().is_none() && (part.ends_with(".spec.ts") || part.ends_with(".test.ts")) {
            return true;
        }
    }
    false
}

fn package_names(project: &Project) -> Vec<String> {
    let Ok(text) = project.read_file("package.json") else { return vec![]; };
    let Ok(json) = serde_json::from_str::<Value>(&text) else { return vec![]; };
    let mut names = BTreeSet::new();
    for section in ["dependencies", "devDependencies", "peerDependencies"] {
        if let Some(map) = json.get(section).and_then(Value::as_object) {
            for name in map.keys() {
                // Angular declarations may exist in any dependency, not just @angular/*.
                if !name.is_empty() { names.insert(name.clone()); }
            }
        }
    }
    names.into_iter().take(150).collect()
}
fn parent(path: &str) -> &str { path.rsplit_once('/').map(|(a, _)| a).unwrap_or("") }
fn normalize_path(path: &str) -> Option<String> {
    let mut out = Vec::<&str>::new();
    for part in path.split('/') {
        match part {
            "" | "." => {},
            ".." => { out.pop()?; },
            _ => out.push(part),
        }
    }
    Some(out.join("/"))
}
fn internal_dts(base: &str, spec: &str) -> Option<String> {
    if !spec.starts_with("./") && !spec.starts_with("../") { return None; }
    let mut path = spec.to_owned();
    if path.ends_with(".d.js") { path.truncate(path.len() - 3); path.push_str(".ts"); }
    else if path.ends_with(".js") { path.truncate(path.len() - 3); path.push_str(".d.ts"); }
    else if !path.ends_with(".d.ts") && !path.ends_with(".d.mts") { path.push_str(".d.ts"); }
    normalize_path(&format!("{}/{}", parent(base), path))
}
type PublicNames = BTreeMap<String, BTreeSet<String>>;

fn local_public_names(text: &str, symbol: &str, exports: &[metadata::Export]) -> BTreeSet<String> {
    let mut public = BTreeSet::new();
    if text.contains(&format!("export declare class {symbol}"))
        || text.contains(&format!("export class {symbol}"))
        || text.contains(&format!("export declare abstract class {symbol}"))
    {
        public.insert(symbol.to_owned());
    }
    for edge in exports.iter().filter(|edge| edge.path.is_none()) {
        if let Some(aliases) = edge.symbols.get(symbol) {
            public.extend(aliases.iter().cloned());
        }
    }
    public
}

// Map the defining name inside a child declaration file to *all* public names
// exposed by the parent. Never throw away a second alias for the same symbol.
fn follow_reexport(edge: &metadata::Export, parent: Option<&PublicNames>) -> PublicNames {
    let mut symbols = PublicNames::new();
    for (inner, aliases) in &edge.symbols {
        for alias in aliases {
            if let Some(allowed) = parent {
                if let Some(public) = allowed.get(alias) {
                    symbols.entry(inner.clone()).or_default().extend(public.iter().cloned());
                }
            } else {
                symbols.entry(inner.clone()).or_default().insert(alias.clone());
            }
        }
    }
    symbols
}

// A re-export chain carries the public symbol alias down to its defining file.
// `allowed = None` means this is an entrypoint or `export *`, so its own exports
// decide public visibility. `Some(map)` means the parent explicitly reexported names.
fn visit(
    project: &Project, package_root: &str, path: &str,
    module: &str, allowed: Option<PublicNames>,
    visited: &mut BTreeSet<String>, depth: usize, out: &mut Vec<ExportedDecl>,
) {
    if depth > 7 || visited.len() >= 140 || path.contains("../node_modules") { return; }
    let Some(path) = normalize_path(path) else { return; };
    if !path.starts_with(&format!("{package_root}/")) { return; };
    let key = format!("{path}:{allowed:?}");
    if !visited.insert(key) { return; }
    let Ok(src) = project.read_file(&path) else { return; };
    if src.len() > 4_000_000 { return; }
    let exports = metadata::exports(&src);
    for decl in metadata::declarations(&src) {
        let names = match &allowed {
            Some(symbols) => symbols.get(&decl.symbol).cloned().unwrap_or_default(),
            None => local_public_names(&src, &decl.symbol, &exports),
        };
        for public_symbol in names {
            out.push(ExportedDecl {
                declaration: decl.clone(), public_symbol, module: module.into(),
            });
        }
    }
    for edge in &exports {
        let Some(spec) = &edge.path else { continue; };
        let Some(child) = internal_dts(&path, spec) else { continue; };
        let next_allowed = if edge.star {
            allowed.clone()
        } else {
            let symbols = follow_reexport(edge, allowed.as_ref());
            if symbols.is_empty() { continue; }
            Some(symbols)
        };
        visit(project, package_root, &child, module, next_allowed, visited, depth + 1, out);
    }
}

/// Identifies optional testing entrypoints for displaying a non-blocking hint.
/// Test entrypoints remain visible and selectable as requested.
pub(crate) fn is_test_origin(module: &str) -> bool {
    let path = module.replace('\\', "/");
    path.split('/').any(is_test_dir)
        || (module.starts_with('.') && path.rsplit('/').next().is_some_and(|name| {
            name.ends_with(".spec") || name.ends_with(".test")
        }))
}

pub fn scan_installed(project: &Project) -> Vec<ExportedDecl> {
    let mut out = Vec::new();
    for name in package_names(project) {
        let root = format!("node_modules/{name}");
        for (module, entry) in installed_entries(project, &name) {
            let path = format!("{root}/{}", entry.trim_start_matches("./"));
            visit(project, &root, &path, &module, None, &mut BTreeSet::new(), 0, &mut out);
        }
    }
    out.sort_by(|a,b| (&a.module,&a.public_symbol).cmp(&(&b.module,&b.public_symbol)));
    out.dedup_by(|a,b| a.module==b.module && a.public_symbol==b.public_symbol && a.declaration.kind==b.declaration.kind);
    out
}

/// Resolves only package entrypoints that the Component has *already registered*.
/// Avoid scanning all direct dependencies in the editor's code-action hot path.
/// For example, `@angular/material/button` belongs to `@angular/material` but
/// should scan only its `button` entrypoint (not all Material packages).
fn package_root(specifier: &str) -> &str {
    if specifier.starts_with('@') {
        let slash = specifier[1..].find('/').map(|i| i + 1);
        match slash {
            Some(pos) => specifier[pos + 1..].find('/')
                .map(|i| &specifier[..pos + 1 + i])
                .unwrap_or(specifier),
            None => specifier,
        }
    } else {
        specifier.split('/').next().unwrap_or(specifier)
    }
}

pub fn scan_modules(project: &Project, modules: &[String]) -> Vec<ExportedDecl> {
    let mut out = Vec::new();
    let mut unique = BTreeSet::new();
    for module in modules {
        if module.starts_with('.') || !unique.insert(module.clone()) { continue; }
        let package = package_root(module);
        let root = format!("node_modules/{package}");
        for (entrypoint, types) in installed_entries(project, package) {
            if entrypoint.as_str() != module.as_str() { continue; }
            let path = format!("{root}/{}", types.trim_start_matches("./"));
            visit(project, &root, &path, &entrypoint, None, &mut BTreeSet::new(), 0, &mut out);
        }
    }
    out.sort_by(|a,b| (&a.module,&a.public_symbol).cmp(&(&b.module,&b.public_symbol)));
    out.dedup_by(|a,b| a.module == b.module && a.public_symbol == b.public_symbol
        && a.declaration.kind == b.declaration.kind);
    out
}

/// Given a nonstandalone declaration, find an exported NgModule in the same
/// package entrypoint that exports that declaration (e.g. FormsModule -> NgModel).
pub fn exported_module<'a>(all: &'a [ExportedDecl], decl: &ExportedDecl) -> Option<&'a ExportedDecl> {
    let matches: Vec<_> = all.iter().filter(|candidate| {
        candidate.module == decl.module
            && candidate.declaration.kind == Kind::Module
            && candidate.declaration.exported_symbols.contains(&decl.declaration.symbol)
    }).collect();
    (matches.len() == 1).then(|| matches[0])
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn testing_entrypoints_can_be_labeled_not_silently_removed() {
        assert!(is_test_origin("@angular/router/testing"));
        assert!(is_test_origin("ngx-mask/testing/helpers"));
        assert!(!is_test_origin("@angular/router"));
        assert!(!is_test_origin("package/testingWidget"));
        assert!(is_test_origin("../testing/mock.directive"));
        assert!(is_test_origin("./button.spec"));
        assert!(!is_test_origin("./testimonials.pipe"));
    }
    #[test]
    fn identifies_only_test_secondary_entrypoints() {
        assert!(test_only_entrypoint("@angular/router", "@angular/router/testing"));
        assert!(test_only_entrypoint("@angular/material", "@angular/material/button/testing"));
        assert!(test_only_entrypoint("ngx-mask", "ngx-mask/__tests__/helpers"));
        assert!(!test_only_entrypoint("@angular/router", "@angular/router"));
        assert!(!test_only_entrypoint("@angular/router", "@angular/router/animations"));
        assert!(!test_only_entrypoint("@my/testing-utils", "@my/testing-utils"));
        assert!(!test_only_entrypoint("package", "package/my-testing-utils"));
        assert!(!test_only_entrypoint("pkg", "pkg/testingWidget"));
    }
    #[test]
    fn identifies_test_only_local_declarations() {
        assert!(test_only_source("src/app/widgets/button.spec.ts"));
        assert!(test_only_source("src/app/widgets/button.test.ts"));
        assert!(test_only_source("src/app/testing/mock.directive.ts"));
        assert!(test_only_source("src/app/__tests__/demo.pipe.ts"));
        assert!(!test_only_source("src/app/testimonials.pipe.ts"));
        assert!(!test_only_source("src/app/mask.directive.ts"));
    }
    #[test]
    fn carries_routerlink_aliases_across_reexport_chains() {
        let top = metadata::exports(
            "export { RouterLink, RouterLink as RouterLinkWithHref } from './middle.d.js';"
        );
        let allowed = follow_reexport(&top[0], None);
        assert_eq!(allowed["RouterLink"].len(), 2);
        let middle = metadata::exports(
            "export { InternalRouterLink as RouterLink } from './router_link.d.js';"
        );
        let final_names = follow_reexport(&middle[0], Some(&allowed));
        assert_eq!(final_names["InternalRouterLink"].len(), 2);
        assert!(final_names["InternalRouterLink"].contains("RouterLink"));
        assert!(final_names["InternalRouterLink"].contains("RouterLinkWithHref"));
    }

    #[test]
    fn keeps_local_aliases_without_inventing_unexported_symbols() {
        let source = "declare class RouterLink {} export { RouterLink as RouterLinkWithHref };";
        let aliases = metadata::exports(source);
        let names = local_public_names(source, "RouterLink", &aliases);
        assert_eq!(names.len(), 1);
        assert!(names.contains("RouterLinkWithHref"));
        assert!(!names.contains("RouterLink"));
    }

    #[test]
    fn preserves_both_names_for_directly_exported_class() {
        let source = "export declare class RouterLink {} export { RouterLink as RouterLinkWithHref };";
        let aliases = metadata::exports(source);
        let names = local_public_names(source, "RouterLink", &aliases);
        assert_eq!(names.len(), 2);
        assert!(names.contains("RouterLink"));
        assert!(names.contains("RouterLinkWithHref"));
    }

    #[test]
    fn dts_paths_from_js_reexports() {
        assert_eq!(internal_dts("node_modules/@angular/router/index.d.ts", "./router_module.d-ABC.js"),
            Some("node_modules/@angular/router/router_module.d-ABC.d.ts".into()));
        assert_eq!(internal_dts("node_modules/@angular/router/index.d.ts", "./router_module.d.js"),
            Some("node_modules/@angular/router/router_module.d.ts".into()));
        assert!(internal_dts("node_modules/pkg/index.d.ts", "../../../elsewhere.js").is_none());
    }
    #[test]
    fn extracts_entries_from_conditional_exports() {
        let v: Value = serde_json::from_str(r#"{"types":{"default":"./index.d.ts"},"default":"./index.mjs"}"#).unwrap();
        assert_eq!(first_types(&v).as_deref(),Some("./index.d.ts"));
    }
    #[test]
    fn package_root_for_scoped_entrypoints() {
        assert_eq!(package_root("@angular/router"), "@angular/router");
        assert_eq!(package_root("@angular/material/button"), "@angular/material");
        assert_eq!(package_root("ngx-mask"), "ngx-mask");
        assert_eq!(package_root("some-lib/pipes"), "some-lib");
    }

}
