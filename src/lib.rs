//! Angular Template Imports for OpenIDEA.
//! Lazy scanning discovers candidate origins. A warm cache exposes eager,
//! independently previewable code actions for every valid import origin.
mod angular;
mod metadata;
mod packages;
mod resolver;
mod template;
mod typescript;

use openidea_plugin::{host, register_plugin, text, Error, Invocation, PickItem, Plugin, Range, TextEdit};
use openidea_plugin::providers::{CodeAction, Document, Providers};
use std::collections::BTreeMap;
use angular::Dependency;

#[derive(Default)]
pub struct AngularTemplateImports {
    // In-memory only: reclaimed with plugin instance, cleared by package.json changes.
    cache: BTreeMap<String, CandidateCache>,
}

#[derive(Clone)]
struct CandidateCache {
    package_json: String,
    options: BTreeMap<String, Vec<Dependency>>,
}

impl Plugin for AngularTemplateImports {
    fn run_command(&mut self, id: &str, _ctx: &Invocation, _arg: Option<&str>) -> Result<Option<String>, Error> {
        Err(Error::NotFound(format!("comando desconhecido: {id}")))
    }
}

fn lazy_action(title: String, data: String) -> CodeAction {
    CodeAction { title, kind: Some("quickfix".into()), preferred: false, edits: Vec::new(), data: Some(data) }
}

fn edit_for(source: &str, ts_path: &str, dep: &Dependency, major: Option<u32>) -> Option<Vec<(String, Vec<TextEdit>)>> {
    let changed = typescript::add_imports(source, std::slice::from_ref(dep), major).ok()??;
    let range = Range::new(text::position(source, 0), text::position(source, source.len()));
    Some(vec![(ts_path.to_owned(), vec![TextEdit { range, text: changed }])])
}

fn pick_option(options: &[Dependency], label: &str) -> Result<Dependency, Error> {
    if options.is_empty() { return Err(Error::NotFound(format!("Nenhuma importação encontrada para {label}"))); }
    if options.len() == 1 { return Ok(options[0].clone()); }
    let items: Vec<_> = options.iter().enumerate().map(|(index, dep)| {
        let origin = format!("{} de {}", dep.symbol, dep.module);
        let hint = if packages::is_test_origin(&dep.module) { "Entrypoint de testes" } else { "Exportação da biblioteca/projeto" };
        PickItem::new(&index.to_string(), &origin).description(hint)
    }).collect();
    let chosen = host::pick(&format!("Escolha a origem para {label}"), &items)?
        .ok_or(Error::Cancelled)?;
    let index = chosen.parse::<usize>().map_err(|_| Error::Invalid("Opção inválida".into()))?;
    options.get(index).cloned().ok_or(Error::Stale)
}

impl Providers for AngularTemplateImports {
    fn code_actions(&mut self, doc: &Document, _selection: Range) -> Vec<CodeAction> {
        if !doc.path.ends_with(".html") { return Vec::new(); }
        let needs = template::detect(&doc.text);
        if needs.is_empty() { return Vec::new(); }
        let Some(project) = host::project_of(&doc.path) else { return Vec::new(); };
        let Ok((ts_path, component)) = resolver::owner(&project, &doc.path) else { return Vec::new(); };
        let major = resolver::angular_major(&project);
        if !typescript::supports_component_imports(&component, major) { return Vec::new(); }
        let satisfied = resolver::satisfied_needs(&project, &ts_path, &component, &needs);
        let missing: Vec<_> = needs.into_iter().filter(|need| !satisfied.contains(&need.id())).collect();
        if missing.is_empty() { return Vec::new(); }

        let package_json = project.read_file("package.json").unwrap_or_default();
        let cache = self.cache.get(&ts_path).filter(|entry| entry.package_json == package_json);
        let mut result = Vec::new();
        for need in &missing {
            let options = cache.and_then(|entry| entry.options.get(&need.id()));
            let mut displayed = 0;
            if let Some(options) = options {
                for dep in options.iter().take(12) {
                    if let Some(edits) = edit_for(&component, &ts_path, dep, major) {
                        result.push(CodeAction {
                            title: format!("Angular: importar {} de {}{}", dep.symbol, dep.module,
                                if packages::is_test_origin(&dep.module) { " (teste)" } else { "" }),
                            kind: Some("quickfix".into()), preferred: false, edits, data: None,
                        });
                        displayed += 1;
                    }
                }
            }
            if displayed == 0 {
                result.push(lazy_action(format!("Angular: escolher importação para {}", need.label()), need.id()));
            }
        }
        // Force a fresh scan and choose a different origin when package metadata changes
        // or when the cache doesn't contain every valid alternative.
        if cache.is_some() {
            result.push(lazy_action("Angular: atualizar opções de importação".into(), "refresh".into()));
        } else {
            // The initial scan cannot be performed in the 50ms provider budget.
            // Discover first, then reopen Intentions for native preview per origin.
            result.insert(0, lazy_action("Angular: localizar opções para visualizar prévias".into(), "inspect".into()));
        }
        if missing.len() > 1 {
            let mut all = lazy_action("Angular: importar todas as dependências ausentes".into(), "all".into());
            all.preferred = true;
            result.insert(0, all);
        }
        result
    }

    fn resolve_code_action(&mut self, doc: &Document, mut action: CodeAction) -> Result<CodeAction, Error> {
        let id = action.data.as_deref().ok_or_else(|| Error::Invalid("Ação sem identificador".into()))?;
        let needs = template::detect(&doc.text);
        let project = host::project_of(&doc.path).ok_or_else(|| Error::NotFound("Projeto Angular".into()))?;
        let (ts_path, source) = resolver::owner(&project, &doc.path)?;
        let major = resolver::angular_major(&project);
        if !typescript::supports_component_imports(&source, major) { return Err(Error::Stale); }
        let satisfied = resolver::satisfied_needs(&project, &ts_path, &source, &needs);
        let missing: Vec<_> = needs.into_iter().filter(|need| !satisfied.contains(&need.id())).collect();
        if missing.is_empty() { return Err(Error::Stale); }
        // Heavy metadata traversal happens only when an action is invoked.
        // Cache *all* candidate origins so subsequent intention lists can contain
        // individual eager actions, each with native diff preview.
        let options = resolver::candidate_options(&project, &ts_path, &missing);
        self.cache.insert(ts_path.clone(), CandidateCache {
            package_json: project.read_file("package.json").unwrap_or_default(),
            options: options.clone(),
        });
        let selected: Vec<_> = if id == "all" || id == "refresh" || id == "inspect" {
            missing.clone()
        } else {
            missing.into_iter().filter(|need| need.id() == id).collect()
        };
        if selected.is_empty() { return Err(Error::Stale); }

        // The refresh action updates the cached list without changing the file;
        // reopen Intentions to see all previewable variants.
        if id == "refresh" || id == "inspect" { action.data = None; return Ok(action); }

        let mut deps = Vec::<Dependency>::new();
        for need in &selected {
            let found = options.get(&need.id()).map(Vec::as_slice).unwrap_or(&[]);
            if found.is_empty() && id == "all" { continue; }
            let dep = pick_option(found, &need.label())?;
            if !deps.iter().any(|x| x.module == dep.module && x.symbol == dep.symbol) {
                deps.push(dep);
            }
        }
        if deps.is_empty() { return Err(Error::NotFound("Nenhuma dependência importável encontrada".into())); }
        let changed = typescript::add_imports(&source, &deps, major).map_err(Error::Invalid)?;
        if let Some(changed) = changed {
            let range = Range::new(text::position(&source, 0), text::position(&source, source.len()));
            action.edits = vec![(ts_path, vec![TextEdit { range, text: changed }])];
        }
        action.data = None;
        Ok(action)
    }
}

register_plugin!(AngularTemplateImports, providers);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eager_preview_is_only_built_for_valid_component_edits() {
        let src = "import { Component } from '@angular/core';\n@Component({standalone: true, imports: [], template: ''})\nexport class Test {}\n";
        let dep = Dependency::new("RouterLink", "@angular/router", None);
        let edits = edit_for(src, "/a/src/app/test.component.ts", &dep, Some(18)).unwrap();
        assert_eq!(edits.len(), 1);
        assert!(edits[0].1[0].text.contains("@angular/router"));
    }
}
