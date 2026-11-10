# Angular Template Imports

**Angular Quick Fixes for OpenIDEA** — maintained by [Arkh-Tech](https://github.com/Arkh-Tech).

Angular Template Imports helps developers resolve missing **directives and pipes** in Angular HTML templates. Instead of manually finding the right declaration, adding a TypeScript import, and registering it in a standalone component, you can apply a Quick Fix directly from the template.

> **Release status:** v0.3.3 is the recommended baseline for publication. Angular component-tag auto-import and Angular-specific semantic coloring are **not** included in this stable release.

## Features

- **Quick Fixes from templates:** detect candidates for attribute directives (`routerLink`, `[formControl]`), structural directives (`*ngIf`), and pipes (`{{ value | date }}`).
- **Metadata-based discovery:** read Angular declarations from local project sources and installed package metadata, including common Angular Ivy `.d.ts` declarations and exported package entry points.
- **Standalone component integration:** update the TypeScript `import` statements and the `@Component({ imports: [...] })` array together.
- **Existing dependency checks:** avoid suggesting dependencies already registered in the owning standalone component.
- **Safer suggestions:** skip test/spec/fixture entry points while discovering *new* dependencies, reducing false suggestions such as testing-only router symbols.
- **Predictable import ordering:** insert new import statements after existing imports; preserve common multiline, side-effect, and type-only import forms.
- **Bulk actions:** offer an import-all action when more than one unresolved template dependency has a compatible fix.
- **Conservative resolution:** avoid silently choosing between genuinely ambiguous exported declarations.

## Example

In `dashboard.component.html`:

```html
<a routerLink="/dashboard">Dashboard</a>
```

If `RouterLink` is missing from the owning standalone component, select the Angular Quick Fix. For a matching Angular Router dependency already installed in the project, the plugin can produce an update similar to:

```ts
import { Component } from '@angular/core';
import { RouterLink } from '@angular/router';

@Component({
  selector: 'app-dashboard',
  standalone: true,
  imports: [RouterLink],
  templateUrl: './dashboard.component.html',
})
export class DashboardComponent {}
```

The exact suggestion depends on the declarations and public exports found in your project.

## Requirements

- [OpenIDEA](https://github.com/l0ng-ai/openidea) with support for the plugin's `engines.openidea = "^1.3"` API requirement.
- An Angular project, using supported standalone components and with dependencies installed (`node_modules`).
- Rust toolchain and the OpenIDEA plugin CLI **only if building from source**.

## Install

Once this plugin is listed in the community marketplace, open **OpenIDEA → Settings → Plugins → Available** and search for **Angular Template Imports**.

Until then, build and link it locally:

```bash
cargo test
openidea-plugin build
openidea-plugin check
openidea-plugin link
```

Reload the plugin in **Settings → Plugins** if your running OpenIDEA session does not pick up changes.

## Build and publish

The source code lives in this repository. For a signed community release, the plugin must use a publisher-prefixed ID (`arkhtech.angular-template-imports`) and be built from this public source.

```bash
cargo test
openidea-plugin build
openidea-plugin check
# Generate the publisher key ONCE and keep it private:
openidea-plugin keygen arkhtech.key
# Create a signed .oiplugin package:
openidea-plugin pack . --key arkhtech.key --publisher arkhtech
```

**Never commit `arkhtech.key` or other private signing keys.** The packaged `.oiplugin` is attached to a GitHub Release, then registered with a pull request to [OpenIDEA Community Plugins](https://github.com/l0ng-ai/openidea-plugins). See its README for registry and SHA-256 requirements.

The Git dependency in `Cargo.toml` currently follows OpenIDEA `main`. Before a public release, pin it to a compatible tested revision and verify the build against the intended OpenIDEA version.

## How it works

1. The plugin reads the current HTML template and identifies directive/pipe candidates.
2. It finds the owning TypeScript component and checks already registered standalone imports.
3. When you choose a Quick Fix, it resolves public declarations from the project and installed dependencies.
4. If a suitable unique declaration is found, it prepares an edit to the TypeScript import block and the component's `imports` array.

The plugin requests `project.read` access to inspect the owning component, Angular source declarations, `package.json`, and installed declaration metadata. Its code actions return editor-managed text edits; it does not request network or process execution permissions.

## Limitations

- **This release does not auto-import Angular component tags** (such as `<app-navbar>`), nor does it implement reliable Angular-specific semantic coloring of component tags or `signal` variables.
- Dependencies must be installed and expose metadata the plugin can understand; no online package lookup is performed.
- The Quick Fix targets standalone components with an editable literal `imports: []` array. Non-standalone components and indirect arrays may require manual changes.
- The scanner is deliberately conservative about complex selectors, reexports, aliases and ambiguous declarations. It is not a full Angular compiler or Angular Language Service replacement.
- Review the proposed TypeScript edits before accepting them in production code.

## Documentation

- [Overview](docs/OVERVIEW.md) — concise marketplace description and capability list.
- [What's New](docs/WHATS_NEW.md) — v0.3.3 release notes.
- [OpenIDEA community registry](https://github.com/l0ng-ai/openidea-plugins) — publishing requirements.

## Feedback and contributions

Bug reports and pull requests are welcome through this repository's **Issues** and **Pull requests** pages. When reporting a resolution issue, include an anonymized template snippet, the affected `@Component` declaration, and relevant Angular/package versions. Please do not include secrets or proprietary project files.

## Maintainers

Published by **[Arkh-Tech](https://github.com/Arkh-Tech)**, with [tiagodiana](https://github.com/tiagodiana) as the proposed maintainer.

## License

Released under the [MIT License](LICENSE).
