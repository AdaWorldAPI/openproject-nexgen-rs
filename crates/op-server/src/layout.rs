//! Layer 0 of the UI-as-ClassView architecture: **a page is a ClassView of
//! regions** with an on/off [`WideFieldMask`].
//!
//! See `.claude/designs/2026-07-08-ui-as-classview-shape-types.md`. The page
//! chrome — header, left menu (the *side menu* that was "completely missing"),
//! right menu, content, footer — is not bespoke HTML: it is a masked region
//! set. Bit `i` of the region mask on ⇒ region `i` renders. Each region is a
//! `ViewRegistry` node stacked under the page node (the drill-down / stack
//! half of the topology kit), so the layout tree is a stacked-ClassView tree,
//! not a new container type.
//!
//! This module is op-local and needs no upstream change: it mints the region
//! mask with the same `from_universe_present` brick every skin uses, composes
//! the regions it selects, and proves the region tree interns acyclically +
//! bijectively via [`crate::viewfilter::ViewRegistry`]. Steps 3–5 (the Rails
//! menu-DSL harvest that *fills* the left menu, the widget ClassView types,
//! and the OGAR codegen emit) are the documented follow-ups.

use lance_graph_contract::class_view::WideFieldMask;

use crate::nav;
use crate::viewfilter::ViewRegistry;

/// The page-region universe (display order). A region's bit position is its
/// index here — the same position convention as every other mask.
pub const REGION_UNIVERSE: &[&str] = &[
    "global",     // 0 — the always-on page wrapper (<body>)
    "header",     // 1 — the top nav bar
    "left_menu",  // 2 — the side menu
    "right_menu", // 3 — aux panel (off by default)
    "main",       // 4 — the primary content region (board / detail / form)
    "footer",     // 5 — the footer
];

/// The regions a standard page presents today: everything except
/// `right_menu` (off — demonstrating the on/off bitmask is real, not
/// cosmetic). `global` + `main` are structural (a page with no `main` has
/// nothing to show); the rest are chrome a page can toggle.
pub const DEFAULT_REGIONS: &[&str] = &["global", "header", "left_menu", "main", "footer"];

/// Mint the region mask from a `present` region set — the sanctioned
/// membership brick, identical rule to every field mask.
///
/// # Panics
///
/// Never: [`REGION_UNIVERSE`] has 6 entries, far under the 256-field cap.
#[must_use]
pub fn region_mask(present: &[&str]) -> WideFieldMask {
    WideFieldMask::from_universe_present(REGION_UNIVERSE, present)
        .expect("6-region universe is within the 256-field SoC cap")
}

/// The default page region mask.
#[must_use]
pub fn default_region_mask() -> WideFieldMask {
    region_mask(DEFAULT_REGIONS)
}

/// Whether region `name` is present in `mask` (position = its
/// [`REGION_UNIVERSE`] index). Unknown names are never present.
#[must_use]
pub fn region_on(mask: &WideFieldMask, name: &str) -> bool {
    match REGION_UNIVERSE.iter().position(|r| *r == name) {
        #[allow(clippy::cast_possible_truncation)] // index < 6
        Some(i) => mask.has(i as u8),
        None => false,
    }
}

/// Intern the page's regions under a single page node in a [`ViewRegistry`]
/// — the drill-down/stack proof (children interned before the parent ⇒
/// acyclic by construction). Each present region is a leaf node; the page
/// node stacks them. Returns the registry so the caller can run
/// `verify_stack_order` / `verify_bijective` (done at boot + in tests).
#[must_use]
pub fn region_registry(mask: &WideFieldMask) -> ViewRegistry {
    let mut reg = ViewRegistry::new();
    let mut children = Vec::new();
    for name in REGION_UNIVERSE {
        if region_on(mask, name) && *name != "global" {
            // Each region is a leaf ClassView node (its own basis/mask would
            // be minted in the drill-down; here the node identity is enough
            // to prove the stack composes). `region_mask(&[name])` is a
            // distinct single-bit mask per region, so nodes stay distinct.
            let node = reg
                .intern(region_concept(name), region_mask(&[name]), vec![])
                .expect("leaf region has no children");
            children.push(node);
        }
    }
    // The page node stacks every present region (children-first — the
    // construction order IS the topological order).
    reg.intern("Page", mask.clone(), children)
        .expect("region children are interned above the page node");
    reg
}

/// Stable concept name for a region (what the `ViewRegistry` node carries).
fn region_concept(name: &str) -> &'static str {
    match name {
        "header" => "RegionHeader",
        "left_menu" => "RegionLeftMenu",
        "right_menu" => "RegionRightMenu",
        "main" => "RegionMain",
        "footer" => "RegionFooter",
        _ => "Region",
    }
}

/// The **side menu** HTML — the left-menu region. Primary screens
/// ([`nav::menu`]) render as live links; the deferred dead lanes
/// ([`nav::NOT_YET_NAVIGABLE`]) render as greyed, disabled items so the side
/// menu *shows the full nav surface*, including what is not yet built (honest
/// about the structure). When the Rails menu-DSL harvest (step 3) lands, this
/// list is generated from the harvested menu shape rather than `nav::menu`.
#[must_use]
pub fn left_menu_html() -> String {
    let mut out = String::from("<nav class=\"sidemenu\" aria-label=\"Main\">");
    out.push_str("<ul class=\"sidemenu-primary\">");
    for (label, href) in nav::menu() {
        out.push_str(&format!(
            "<li><a href=\"{href}\">{label}</a></li>",
            href = esc(href),
            label = esc(label),
        ));
    }
    out.push_str("</ul>");
    // Deferred lanes — visible but disabled, so the menu is honest about the
    // whole surface (not silently hiding the not-yet-navigable screens).
    if !nav::NOT_YET_NAVIGABLE.is_empty() {
        out.push_str("<div class=\"sidemenu-soon-label\">Coming soon</div>");
        out.push_str("<ul class=\"sidemenu-soon\">");
        for concept in nav::NOT_YET_NAVIGABLE {
            out.push_str(&format!(
                "<li aria-disabled=\"true\"><span>{}</span></li>",
                esc(&humanize(concept)),
            ));
        }
        out.push_str("</ul>");
    }
    out.push_str("</nav>");
    out
}

/// The footer region HTML.
#[must_use]
pub fn footer_html() -> String {
    "<div class=\"footer-inner\">OpenProject RS · render-bake preview</div>".to_string()
}

/// Turn a PascalCase concept (`ProjectStatus`) into a menu label
/// (`Project statuses`) — best-effort: split camel humps, lower the tail,
/// naive pluralize.
fn humanize(concept: &str) -> String {
    let mut words: Vec<String> = Vec::new();
    let mut cur = String::new();
    for c in concept.chars() {
        if c.is_uppercase() && !cur.is_empty() {
            words.push(std::mem::take(&mut cur));
        }
        cur.push(c);
    }
    if !cur.is_empty() {
        words.push(cur);
    }
    let mut label = words.join(" ");
    // Pluralize the tail (naive English rules — a real inflector is overkill
    // for a menu label): `-s/-x/-ch/-sh` → `+es`, `-y` → `-ies`, else `+s`.
    if label.ends_with('s')
        || label.ends_with('x')
        || label.ends_with("ch")
        || label.ends_with("sh")
    {
        label.push_str("es");
    } else if label.ends_with('y') {
        label.pop();
        label.push_str("ies");
    } else {
        label.push('s');
    }
    // Lowercase all but the first character.
    let mut chars = label.chars();
    match chars.next() {
        Some(first) => first.to_string() + &chars.as_str().to_lowercase(),
        None => label,
    }
}

/// Minimal HTML-attribute/text escape (mirrors `board::esc`, kept local so
/// `layout` has no cross-module coupling for one helper).
fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_mask_has_chrome_but_not_right_menu() {
        let m = default_region_mask();
        assert!(region_on(&m, "global"));
        assert!(region_on(&m, "header"));
        assert!(region_on(&m, "left_menu"));
        assert!(region_on(&m, "main"));
        assert!(region_on(&m, "footer"));
        assert!(!region_on(&m, "right_menu"), "right_menu is off by default");
        assert!(!region_on(&m, "not_a_region"));
    }

    #[test]
    fn region_mask_toggles_are_real() {
        // Turning right_menu on and left_menu off is just a different bit set.
        let m = region_mask(&["global", "header", "right_menu", "main"]);
        assert!(region_on(&m, "right_menu"));
        assert!(!region_on(&m, "left_menu"));
        assert!(!region_on(&m, "footer"));
    }

    #[test]
    fn region_tree_interns_acyclic_and_bijective() {
        let mut reg = region_registry(&default_region_mask());
        // global is the wrapper (not a child node); 4 present regions
        // (header/left_menu/main/footer) + the page node = 5 nodes.
        assert_eq!(reg.len(), 5, "header/left_menu/main/footer + Page");
        assert!(reg.verify_stack_order(), "regions interned before the page");
        assert!(reg.verify_bijective(), "distinct region nodes, round trip");
    }

    #[test]
    fn side_menu_shows_primary_and_deferred() {
        let html = left_menu_html();
        // Primary screens are live links.
        assert!(html.contains("href=\"/\""), "Board link present");
        assert!(html.contains("href=\"/projects\""), "Projects link present");
        // Deferred lanes are shown, disabled — honest about the full surface.
        assert!(html.contains("Coming soon"));
        assert!(html.contains("aria-disabled=\"true\""));
        // A dead-lane concept is humanized (ProjectStatus -> "Project statuses").
        assert!(html.contains("Project statuses"), "{html}");
    }

    #[test]
    fn humanize_splits_and_pluralizes() {
        assert_eq!(humanize("ProjectStatus"), "Project statuses");
        assert_eq!(humanize("Priority"), "Priorities");
        assert_eq!(humanize("BillableWorkEntry"), "Billable work entries");
    }
}
