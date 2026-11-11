# Angular Template Imports

**Smart Angular Quick Fixes for [OpenIDEA](https://github.com/l0ng-ai/openidea)**
Maintained by **[Arkh-Tech](https://github.com/Arkh-Tech)**.

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Version](https://img.shields.io/badge/version-0.3.5-orange.svg)](https://github.com/Arkh-Tech/openidea-angular-template-imports/releases)

Angular Template Imports helps you resolve missing Angular directives and pipes from an HTML template, choose the appropriate import when multiple public exports are available, and update the owning standalone component automatically.

The plugin discovers candidates from **your project's source code and locally installed Angular packages**, rather than relying on a hardcoded list of directives.

## Highlights

- **Quick Fixes from Angular templates:** detect missing attribute directives, structural directives, and pipes (for example, `routerLink`, `[formControl]`, `*ngIf`, and `{{ value | date }}`).
- **Multiple import choices (v0.3.5):** list alternative public class names and package entry points when more than one valid import is available.
- **Interactive import picker:** choose the intended declaration and module; production origins are prioritized over test-only origins.
- **Optional testing imports:** `test`/`testing` entry points can be shown as explicit alternatives marked **(teste)** instead of being silently excluded or chosen.
- **Native edit previews:** after on-demand discovery, each available import appears as an individual intention with a preview of the proposed `component.ts` change (when the host can generate it).
- **Public alias support:** retain multiple public names for a single underlying export. For example, depending on the installed Angular version, `RouterLink` and `RouterLinkWithHref` can both be offered from `@angular/router`.
- **Standalone component updates:** add the necessary TypeScript import and update `@Component({ imports: [...] })` together.
- **Safe import organization:** avoid duplicates and insert new statements after the existing TypeScript import block.
- **Bulk Quick Fixes:** import several missing dependencies in one operation, selecting an origin where necessary.

## Example: Angular Router

Suppose a template uses a router directive:

```html
<a routerLink="/animals">Animals</a>
```

Open the **Intentions / Quick Fix** menu. Once available origins have been discovered, you may see actions similar to:

```text
Angular: importar RouterLink de @angular/router
Angular: importar RouterLinkWithHref de @angular/router
Angular: importar ɵRouterLink de @angular/router/testing (teste)
```

The actual names depend on your installed Angular version and the package declarations the plugin can resolve. Select the appropriate origin; the plugin updates the owning TypeScript component, for example:

```typescript
import { Component } from '@angular/core';
import { RouterLink } from '@angular/router';

@Component({
  selector: 'app-animals',
  standalone: true,
  imports: [RouterLink],
  templateUrl: './animals.component.html',
})
export class AnimalsComponent {}
```

If the component already imports `RouterLink`, the plugin avoids adding it again.

## How to use import choices and previews

1. Open a component's `.html` template and place the caret on a missing directive or pipe.
2. Open **Intentions / Quick Fixes**.
3. On the first scan, select **Angular: localizar opções para visualizar prévias**.
4. Open **Intentions / Quick Fixes** again. The plugin now displays individually previewable import options discovered for the template.
5. Choose a specific import action, or use **Angular: escolher importação para ...** to open the interactive picker.
6. After installing or changing packages, use **Angular: atualizar opções de importação** to refresh the available choices.

The plugin intentionally performs the more expensive package scan on demand. This means the first previewable list may require opening Intentions twice. The picker and menu are provided by the OpenIDEA plugin API; they are **not** an exact replacement for the built-in TypeScript import submenu.

## Requirements

- [OpenIDEA](https://github.com/l0ng-ai/openidea) with the plugin API version declared by `engines.openidea = "^1.3"`.
- An Angular project using supported standalone components, with dependencies installed (`node_modules`) when resolving third-party libraries.
- Rust and the `openidea-plugin` CLI **only when building from source**.

## Installation

### From the marketplace or a release

If the release is available in the OpenIDEA Community Plugins marketplace, open **Settings → Plugins → Available** and search for **Angular Template Imports**.

Alternatively, download the signed `.oiplugin` file from the [GitHub Releases page](https://github.com/Arkh-Tech/openidea-angular-template-imports/releases) and select **Settings → Plugins → Install Plugin from Package File...**.

### Build from source

```bash
cargo test
openidea-plugin build
openidea-plugin check
openidea-plugin link
```

Reload the plugin from **Settings → Plugins** after rebuilding if the running IDE does not pick up the updated linked plugin.

For a reproducible release, keep `Cargo.toml` and `Cargo.lock` pinned to a tested OpenIDEA SDK revision. The v0.3.5 development baseline uses the OpenIDEA `v0.1.39` SDK tag.

## How it works

1. Detect directive and pipe usage in an Angular HTML template.
2. Find the owning TypeScript component and check existing standalone imports.
3. Inspect project declarations and installed package metadata for matching public symbols and entry points.
4. Preserve distinct public export aliases instead of collapsing them into one name.
5. Offer import alternatives, labeling testing origins and avoiding automatic selection of testing-only entries.
6. Apply editor-managed edits to the TypeScript imports and the component's `imports` array.

The plugin requests **`project.read`** to inspect local project files and declaration metadata. It does not need network access or permission to execute external processes.

## Current limitations

- **Angular component-tag auto-import** (for example, resolving `<app-my-component>`) is not a stable feature of this release.
- **Angular-specific semantic highlighting** of component tags and `signal` variables is not included. Syntax colors are determined by the IDE/theme and available language services.
- Resolution is based on supported project metadata and is not a substitute for the Angular Language Service or a complete TypeScript compiler.
- Complex selectors, unconventional package exports, path aliases, and indirect/non-literal component imports may require manual work.
- The current implementation is aimed at standalone components and editable `@Component.imports` arrays.
- Native previews are available for concrete code actions after metadata discovery; picker-based actions may not have a preview in the Intentions menu.
- Available options may differ between Angular versions and installed library versions. Inspect changes before applying them.

## Development and releases

The public source repository is:

**https://github.com/Arkh-Tech/openidea-angular-template-imports**

The plugin ID is `arkhtech.angular-template-imports`. Signed release packages are published under the `arkhtech` publisher ID. **Never commit private signing keys** such as `arkhtech.key`.

For the most recent release notes, see [What's New](docs/WHATS_NEW.md). For the community distribution requirements, see the [OpenIDEA plugin registry](https://github.com/l0ng-ai/openidea-plugins).

## Contributing

Bug reports and pull requests are welcome. When reporting a resolution issue, please include:

- The Angular and OpenIDEA versions.
- A minimal template and the corresponding `@Component` imports.
- The import choices shown and the result you expected.

Please remove secrets and proprietary code before sharing a reproduction.

## License

Copyright © 2026 **Arkh-Tech**. Licensed under the [MIT License](LICENSE).
