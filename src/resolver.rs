//! Resolve ownership and candidate directive/pipe imports from real Angular metadata.
use openidea_plugin::{Error, Project};
use std::collections::BTreeSet;
use crate::{angular::Dependency, metadata::{self, Kind}, packages, template::Need, typescript};

const MAX_FILES: u32 = 4000;
fn norm(input: &str) -> String {
    let input = input.replace('\\', "/");
    let absolute = input.starts_with('/');
    let mut parts = Vec::<&str>::new();
    for part in input.split('/') {
        match part {
            "" | "." => {},
            ".." => { if !parts.is_empty() && parts.last() != Some(&"..") { parts.pop(); } else if !absolute { parts.push(".."); } },
            other => parts.push(other),
        }
    }
    format!("{}{}", if absolute { "/" } else { "" }, parts.join("/"))
}
fn parent(path: &str) -> &str { path.rsplit_once('/').map(|(p, _)| p).unwrap_or("") }
fn resolve_from(owner: &str, other: &str) -> String {
    if other.starts_with('/') { norm(other) } else { norm(&format!("{}/{}", parent(owner), other)) }
}
fn matches_template(ts: &str, content: &str, html: &str) -> bool {
    let Some(meta) = typescript::component_meta(content) else { return false };
    let Some(template_url) = meta.template_url else { return false };
    resolve_from(ts, &template_url) == norm(html)
}
pub fn owner(project: &Project, html: &str) -> Result<(String, String), Error> {
    if let Some(stem) = html.strip_suffix(".html") {
        let guess = format!("{stem}.ts");
        if let Ok(text) = project.read_file(&guess) {
            if matches_template(&guess, &text, html) { return Ok((guess, text)); }
        }
    }
    let candidates = project.find_files("**/*.ts", MAX_FILES)?;
    for path in candidates {
        if path.contains("node_modules/") || path.ends_with(".d.ts") { continue; }
        let Ok(src) = project.read_file(&path) else { continue; };
        if src.contains("@Component") && matches_template(&path, &src, html) { return Ok((path, src)); }
    }
    Err(Error::NotFound(format!("Componente que usa o template {html}")))
}
pub fn angular_major(project: &Project) -> Option<u32> {
    let package = project.read_file("package.json").ok()?;
    let parsed: serde_json::Value = serde_json::from_str(&package).ok()?;
    let version = parsed.get("dependencies")?.get("@angular/core")?.as_str()?;
    let start = version.find(|c:char| c.is_ascii_digit())?;
    version[start..].chars().take_while(char::is_ascii_digit).collect::<String>().parse().ok()
}
fn relative_module(from_ts: &str, target_ts: &str) -> Option<String> {
    let from = norm(parent(from_ts));
    let target = norm(target_ts);
    let target = target.strip_suffix(".ts")?;
    let a: Vec<_> = from.split('/').filter(|p| !p.is_empty()).collect();
    let b: Vec<_> = target.split('/').filter(|p| !p.is_empty()).collect();
    let mut common = 0;
    while common < a.len() && common < b.len() && a[common] == b[common] { common += 1; }
    let ups = "../".repeat(a.len() - common);
    let rest = b[common..].join("/");
    Some(if ups.is_empty() { format!("./{rest}") } else { format!("{ups}{rest}") })
}
fn available(standalone: Option<bool>, angular_major: Option<u32>) -> bool {
    standalone == Some(true) || (standalone.is_none() && angular_major.is_some_and(|n| n >= 19))
}

/// Scans project-authored @Directive and @Pipe decorators. No custom element tags.
fn scan_local(project: &Project, from_ts: &str, major: Option<u32>, needs: &[Need]) -> Vec<(String, Dependency)> {
    let mut result = Vec::new();
    let Ok(files) = project.find_files("**/*.ts", MAX_FILES) else { return result; };
    for path in files {
        if path == from_ts || path.ends_with(".d.ts") || path.contains("node_modules/")
            || packages::test_only_source(&path) { continue; }
        let Ok(src) = project.read_file(&path) else { continue; };
        if !src.contains("@Directive") && !src.contains("@Pipe") { continue; }
        let Some(module) = relative_module(from_ts, &path) else { continue; };
        if src.contains("@Directive") {
            if let Some(meta) = typescript::directive_meta(&src) {
                if available(meta.standalone, major) {
                    if let Some(class) = meta.class_name {
                        for need in needs {
                            if let Need::Directive { name, tag, attrs } = need {
                                if metadata::selector_matches(&meta.selector, tag, attrs, name) {
                                    result.push((need.id(), Dependency::new(&class, &module, None)));
                                }
                            }
                        }
                    }
                }
            }
        }
        if src.contains("@Pipe") {
            if let Some(meta) = typescript::pipe_meta(&src) {
                if available(meta.standalone, major) {
                    if let Some(class) = meta.class_name {
                        for need in needs {
                            if matches!(need, Need::Pipe(name) if name == &meta.name) {
                                result.push((need.id(), Dependency::new(&class, &module, None)));
                            }
                        }
                    }
                }
            }
        }
    }
    result
}

fn scan_dependencies(project: &Project, needs: &[Need]) -> Vec<(String, Dependency)> {
    let all = packages::scan_installed(project);
    let mut result = Vec::new();
    for def in &all {
        if !matches!(def.declaration.kind, Kind::Directive | Kind::Pipe) { continue; }
        // Standalone declarations can be imported directly. Legacy directives/pipes
        // must be provided by an exported NgModule instead.
        let dep = if def.declaration.standalone {
            let covered = packages::exported_module(&all, def);
            Dependency::new(&def.public_symbol, &def.module, covered.map(|x| x.public_symbol.as_str()))
        } else {
            let Some(module) = packages::exported_module(&all, def) else { continue; };
            Dependency::new(&module.public_symbol, &module.module, None)
        };
        for need in needs {
            let found = match need {
                Need::Pipe(name) => def.declaration.kind == Kind::Pipe && def.declaration.name == *name,
                Need::Directive { name, tag, attrs } => def.declaration.kind == Kind::Directive
                    && metadata::selector_matches(&def.declaration.name, tag, attrs, name),
            };
            if found { result.push((need.id(), dep.clone())); }
        }
    }
    result
}

/// Check if a template need is already supplied by a named import registered
/// in `@Component.imports`. This only visits the imported library entrypoints,
/// never the entire dependency tree on every invocation of `code_actions`.
fn satisfied_by_exports(
    definitions: &[packages::ExportedDecl],
    active: &[typescript::ActiveImport],
    needs: &[Need],
) -> BTreeSet<String> {
    let mut satisfied = BTreeSet::new();
    for def in definitions {
        if !matches!(def.declaration.kind, Kind::Directive | Kind::Pipe) { continue; }
        let direct = def.declaration.standalone && active.iter().any(|item| {
            item.module == def.module && item.symbol == def.public_symbol
        });
        let module = packages::exported_module(definitions, def).is_some_and(|group| {
            active.iter().any(|item| item.module == group.module && item.symbol == group.public_symbol)
        });
        if !direct && !module { continue; }
        for need in needs {
            let matches = match need {
                Need::Pipe(name) => def.declaration.kind == Kind::Pipe && def.declaration.name == *name,
                Need::Directive { name, tag, attrs } => def.declaration.kind == Kind::Directive
                    && metadata::selector_matches(&def.declaration.name, tag, attrs, name),
            };
            if matches { satisfied.insert(need.id()); }
        }
    }
    satisfied
}

/// Best-effort, fast query for hiding already satisfied quick fixes. A dependency
/// which cannot be verified (package missing, unsupported .d.ts, complex imports)
/// stays visible. Resolution of *new* imports remains lazy on action selection.
pub fn satisfied_needs(project: &Project, from_ts: &str, source: &str, needs: &[Need]) -> BTreeSet<String> {
    let active = typescript::active_imports(source);
    if active.is_empty() { return BTreeSet::new(); }
    let external_modules: Vec<String> = active.iter()
        .filter(|item| !item.module.starts_with('.'))
        .map(|item| item.module.clone()).collect();
    let exported = packages::scan_modules(project, &external_modules);
    let mut satisfied = satisfied_by_exports(&exported, &active, needs);

    // Locally declared standalone pipes/directives can be resolved without a
    // full project search, because a registered TS import gives their file path.
    let major = angular_major(project);
    for item in &active {
        if !item.module.starts_with('.') { continue; }
        let path = resolve_from(from_ts, &item.module);
        let candidates = [format!("{path}.ts"), format!("{path}/index.ts")];
        let Some(src) = candidates.iter().find_map(|p| project.read_file(p).ok()) else { continue; };
        if let Some(meta) = typescript::directive_meta(&src) {
            if meta.class_name.as_deref() == Some(item.symbol.as_str()) && available(meta.standalone, major) {
                for need in needs {
                    if let Need::Directive { name, tag, attrs } = need {
                        if metadata::selector_matches(&meta.selector, tag, attrs, name) {
                            satisfied.insert(need.id());
                        }
                    }
                }
            }
        }
        if let Some(meta) = typescript::pipe_meta(&src) {
            if meta.class_name.as_deref() == Some(item.symbol.as_str()) && available(meta.standalone, major) {
                for need in needs {
                    if matches!(need, Need::Pipe(name) if name == &meta.name) {
                        satisfied.insert(need.id());
                    }
                }
            }
        }
    }
    satisfied
}

pub fn dependencies(project: &Project, from_ts: &str, needs: &[Need], strict: bool) -> Result<Vec<Dependency>, Error> {
    let major = angular_major(project);
    let mut possibilities = scan_local(project, from_ts, major, needs);
    possibilities.extend(scan_dependencies(project, needs));
    let mut resolved = Vec::<Dependency>::new();
    for need in needs {
        let mut options: Vec<_> = possibilities.iter().filter(|(id, _)| id == &need.id()).map(|(_,dep)|dep.clone()).collect();
        options.sort_by(|a,b| (&a.module,&a.symbol).cmp(&(&b.module,&b.symbol)));
        options.dedup_by(|a,b| a.module == b.module && a.symbol == b.symbol);
        if options.len() == 1 {
            if !resolved.iter().any(|x| x.module == options[0].module && x.symbol == options[0].symbol) {
                resolved.push(options.remove(0));
            }
        } else if strict {
            let message = if options.is_empty() { format!("Nenhuma diretiva/pipe exportado encontrado para {}", need.label()) }
                else {
                    let details = options.iter().take(6)
                        .map(|dep| format!("{} de {}", dep.symbol, dep.module))
                        .collect::<Vec<_>>().join("; ");
                    format!("Existem {} definicoes possiveis para {}: {}. Nao vou escolher automaticamente.",
                        options.len(), need.label(), details)
                };
            return Err(Error::NotFound(message));
        }
        // For an "import all" action, skip unknown/ambiguous candidates safely.
    }
    Ok(resolved)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn normalizes_paths() {
        assert_eq!(resolve_from("/a/src/app/card/card.component.ts", "../card.html"), "/a/src/app/card.html");
        assert_eq!(relative_module("/a/src/app/home/home.component.ts", "/a/src/app/shared/field.directive.ts").unwrap(), "../shared/field.directive");
    }
    #[test]
    fn hides_only_activated_directives_pipes_and_exporting_modules() {
        let defs = vec![
            packages::ExportedDecl { declaration: metadata::Decl {
                kind: Kind::Directive, symbol: "RouterLink".into(), name: "[routerLink]".into(),
                standalone: true, exported_symbols: vec![],
            }, public_symbol: "RouterLink".into(), module: "@angular/router".into() },
            packages::ExportedDecl { declaration: metadata::Decl {
                kind: Kind::Module, symbol: "RouterModule".into(), name: String::new(),
                standalone: false, exported_symbols: vec!["RouterLink".into()],
            }, public_symbol: "RouterModule".into(), module: "@angular/router".into() },
            packages::ExportedDecl { declaration: metadata::Decl {
                kind: Kind::Pipe, symbol: "NgxMaskPipe".into(), name: "mask".into(),
                standalone: true, exported_symbols: vec![],
            }, public_symbol: "NgxMaskPipe".into(), module: "ngx-mask".into() },
            packages::ExportedDecl { declaration: metadata::Decl {
                kind: Kind::Directive, symbol: "NgxMaskDirective".into(), name: "[mask]".into(),
                standalone: true, exported_symbols: vec![],
            }, public_symbol: "NgxMaskDirective".into(), module: "ngx-mask".into() },
        ];
        let needs = crate::template::detect(r#"<a routerLink="/ok">{{ v | mask }}</a><input mask="000">"#);
        let active = vec![
            typescript::ActiveImport { module: "@angular/router".into(), symbol: "RouterModule".into() },
            typescript::ActiveImport { module: "ngx-mask".into(), symbol: "NgxMaskPipe".into() },
        ];
        let satisfied = satisfied_by_exports(&defs, &active, &needs);
        assert!(satisfied.contains("directive:a:routerLink"));
        assert!(satisfied.contains("pipe:mask"));
        assert!(!satisfied.contains("directive:input:mask"));
    }

}
