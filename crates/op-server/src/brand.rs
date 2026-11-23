//! Company identity (corporate design) for the page chrome, as configuration.
//!
//! The defaults are the DATAGROUP corporate identity as report-rs ships it
//! (`templates/themes.css`, theme `datagroup`): DG red `#C8102E`, DG grey
//! `#4A4A4A`, DG black `#1A1A1A`, the Hub-blue navigation `#0D3A64`, border
//! `#E0E6EC`, radius 14px, the soft card shadow and a 2px DG-red focus ring.
//! The values are copied, not shared through a dependency.
//!
//! Another company sets its own identity as TOML, either in a file named by
//! `OP_BRAND_FILE` or inline in `OP_BRAND_TOML` (for hosts such as Railway
//! that pass configuration as variables); setting both is refused. Every key
//! is optional and falls back to the default:
//!
//! ```toml
//! [brand]
//! name = "OpenProject RS"
//! logo_url = "https://example.com/logo.svg"
//! accent = "#c8102e"
//! side_bg = "#0d3a64"
//! ```
//!
//! Values are written into a stylesheet and an `<img src>`, so they are
//! checked at startup: a colour or length may only use the characters CSS
//! values need, and the logo must be an `https://` address.

use serde::Deserialize;
use std::sync::OnceLock;

/// The company identity the pages render with.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Brand {
    /// The label in the top bar.
    pub name: String,
    /// The logo before the label, an `https://` address.
    pub logo_url: Option<String>,
    /// The font stack.
    pub font_family: String,
    /// Page background.
    pub bg: String,
    /// Body text.
    pub fg: String,
    /// Headlines.
    pub h1: String,
    /// Secondary text, table headings.
    pub muted: String,
    /// Links, primary buttons, the focus ring, the active menu item.
    pub accent: String,
    /// Text on the accent colour.
    pub on_accent: String,
    /// Lines and borders.
    pub border: String,
    /// Cards, tables, the page header.
    pub card_bg: String,
    /// Table headings and group rows.
    pub subtle_bg: String,
    /// Top bar and side menu background.
    pub side_bg: String,
    /// Top bar and side menu text.
    pub side_fg: String,
    /// Side menu hover.
    pub side_hover: String,
    /// Large corner radius (cards).
    pub radius: String,
    /// Small corner radius (inputs, buttons).
    pub radius_sm: String,
    /// Card shadow.
    pub elevation: String,
}

impl Default for Brand {
    fn default() -> Self {
        Self {
            name: "OpenProject RS".into(),
            logo_url: None,
            font_family: "-apple-system, BlinkMacSystemFont, \"Segoe UI\", Roboto, Helvetica, Arial, sans-serif".into(),
            bg: "#f5f5f5".into(),
            fg: "#1a1a1a".into(),
            h1: "#1a1a1a".into(),
            muted: "#4a4a4a".into(),
            accent: "#c8102e".into(),
            on_accent: "#ffffff".into(),
            border: "#e0e6ec".into(),
            card_bg: "#ffffff".into(),
            subtle_bg: "#eef1f4".into(),
            side_bg: "#0d3a64".into(),
            side_fg: "#ffffff".into(),
            side_hover: "rgba(255,255,255,.08)".into(),
            radius: "14px".into(),
            radius_sm: "10px".into(),
            elevation: "0 4px 16px rgba(0,0,0,.08)".into(),
        }
    }
}

/// Why a brand file was refused.
#[derive(Debug, PartialEq, Eq)]
pub enum BrandError {
    /// The file named by `OP_BRAND_FILE` could not be read.
    Read(String),
    /// Both `OP_BRAND_FILE` and `OP_BRAND_TOML` are set.
    Both,
    /// The file is not valid TOML for a `[brand]` table.
    Parse(String),
    /// A value would leave its CSS declaration or attribute.
    Value { key: &'static str, value: String },
}

impl std::fmt::Display for BrandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BrandError::Read(e) => write!(f, "OP_BRAND_FILE: {e}"),
            BrandError::Both => write!(f, "set OP_BRAND_FILE or OP_BRAND_TOML, not both"),
            BrandError::Parse(e) => write!(f, "brand: {e}"),
            BrandError::Value { key, value } => write!(f, "brand.{key} = {value:?} is not allowed"),
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    #[serde(default)]
    brand: Brand,
}

impl Brand {
    /// Parse a brand file's text and check every value.
    pub fn from_toml(text: &str) -> Result<Brand, BrandError> {
        let file: File = toml::from_str(text).map_err(|e| BrandError::Parse(e.to_string()))?;
        file.brand.checked()
    }

    /// The brand from `OP_BRAND_FILE` or `OP_BRAND_TOML`, or the default when
    /// neither is set.
    pub fn from_env(get: impl Fn(&str) -> Option<String>) -> Result<Brand, BrandError> {
        let set = |k: &str| get(k).filter(|v| !v.trim().is_empty());
        match (set("OP_BRAND_FILE"), set("OP_BRAND_TOML")) {
            (Some(_), Some(_)) => Err(BrandError::Both),
            (None, None) => Ok(Brand::default()),
            (None, Some(text)) => Brand::from_toml(&text),
            (Some(path), None) => {
                let text = std::fs::read_to_string(path.trim())
                    .map_err(|e| BrandError::Read(format!("{path}: {e}")))?;
                Brand::from_toml(&text)
            }
        }
    }

    fn tokens(&self) -> [(&'static str, &str); 16] {
        [
            ("font_family", &self.font_family),
            ("bg", &self.bg),
            ("fg", &self.fg),
            ("h1", &self.h1),
            ("muted", &self.muted),
            ("accent", &self.accent),
            ("on_accent", &self.on_accent),
            ("border", &self.border),
            ("card_bg", &self.card_bg),
            ("subtle_bg", &self.subtle_bg),
            ("side_bg", &self.side_bg),
            ("side_fg", &self.side_fg),
            ("side_hover", &self.side_hover),
            ("radius", &self.radius),
            ("radius_sm", &self.radius_sm),
            ("elevation", &self.elevation),
        ]
    }

    fn checked(self) -> Result<Brand, BrandError> {
        for (key, value) in self.tokens() {
            if !css_value_ok(value) {
                return Err(BrandError::Value {
                    key,
                    value: value.to_string(),
                });
            }
        }
        if let Some(url) = &self.logo_url {
            if !https_url_ok(url) {
                return Err(BrandError::Value {
                    key: "logo_url",
                    value: url.clone(),
                });
            }
        }
        Ok(self)
    }

    /// The tokens as CSS custom properties on `:root` (`--brand-accent`, …).
    #[must_use]
    pub fn css_vars(&self) -> String {
        let mut css = String::from("  :root {");
        for (key, value) in self.tokens() {
            css.push_str(&format!(" --brand-{}: {value};", key.replace('_', "-")));
        }
        css.push_str(" }\n");
        css
    }

    /// The top-bar mark: the logo (when set) and the label.
    #[must_use]
    pub fn topnav_brand_html(&self) -> String {
        let logo = self.logo_url.as_ref().map_or(String::new(), |u| {
            format!(
                "<img class=\"topnav-logo\" src=\"{}\" alt=\"\" decoding=\"async\">",
                esc(u)
            )
        });
        format!(
            "<a href=\"/\" class=\"topnav-brand\">{logo}{}</a>",
            esc(&self.name)
        )
    }
}

/// A CSS value that cannot close its declaration, block or style element:
/// no `;`, braces, `<`, `>`, backslash or line break.
fn css_value_ok(v: &str) -> bool {
    !v.trim().is_empty()
        && v.chars()
            .all(|c| c.is_ascii_alphanumeric() || " #(),.%-_+/\"'".contains(c))
}

/// An `https://` address with a host that cannot leave its attribute.
fn https_url_ok(raw: &str) -> bool {
    let Some(rest) = raw.strip_prefix("https://") else {
        return false;
    };
    let host = rest.split('/').next().unwrap_or("");
    !host.is_empty()
        && !host.contains('@')
        && raw
            .chars()
            .all(|c| c.is_ascii_graphic() && !"\"'<>`".contains(c))
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

static BRAND: OnceLock<Brand> = OnceLock::new();

/// Fix the brand for the process; the first call wins.
pub fn install(brand: Brand) {
    let _ = BRAND.set(brand);
}

/// The brand in force: the installed one, else the default.
pub fn current() -> &'static Brand {
    BRAND.get_or_init(Brand::default)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The default is report-rs's `datagroup` theme (templates/themes.css).
    #[test]
    fn the_default_is_the_datagroup_identity() {
        let b = Brand::default();
        assert_eq!(b.accent, "#c8102e", "DG red");
        assert_eq!(b.side_bg, "#0d3a64", "Hub-blue navigation");
        assert_eq!(b.muted, "#4a4a4a", "DG grey");
        assert_eq!(b.fg, "#1a1a1a", "DG black");
        assert_eq!(b.border, "#e0e6ec");
        assert_eq!((b.radius.as_str(), b.radius_sm.as_str()), ("14px", "10px"));
        assert_eq!(b.elevation, "0 4px 16px rgba(0,0,0,.08)");
        assert!(
            b.clone().checked().is_ok(),
            "the default passes its own checks"
        );
    }

    #[test]
    fn an_unset_or_empty_brand_file_keeps_the_default() {
        assert_eq!(Brand::from_env(|_| None).unwrap(), Brand::default());
        assert_eq!(
            Brand::from_env(|_| Some("  ".into())).unwrap(),
            Brand::default()
        );
    }

    #[test]
    fn a_brand_file_overrides_only_the_keys_it_sets() {
        let b = Brand::from_toml(
            "[brand]\nname = \"ACME Projekte\"\naccent = \"#0071e3\"\nlogo_url = \"https://acme.example/logo.svg\"\n",
        )
        .unwrap();
        assert_eq!(b.name, "ACME Projekte");
        assert_eq!(b.accent, "#0071e3");
        assert_eq!(
            b.side_bg,
            Brand::default().side_bg,
            "an unset key keeps the default"
        );
        assert!(b
            .topnav_brand_html()
            .contains("<img class=\"topnav-logo\" src=\"https://acme.example/logo.svg\""));
        assert!(b.topnav_brand_html().ends_with(">ACME Projekte</a>"));
    }

    #[test]
    fn a_brand_file_is_read_from_op_brand_file() {
        let dir = std::env::temp_dir().join(format!("op-brand-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("brand.toml");
        std::fs::write(&path, "[brand]\naccent = \"#123456\"\n").unwrap();
        let p = path.to_string_lossy().to_string();
        let b = Brand::from_env(|k| (k == "OP_BRAND_FILE").then(|| p.clone())).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(b.accent, "#123456");
        assert!(matches!(
            Brand::from_env(|k| (k == "OP_BRAND_FILE").then(|| "/nonexistent/brand.toml".into())),
            Err(BrandError::Read(_))
        ));
    }

    #[test]
    fn a_brand_can_come_inline_from_op_brand_toml_but_not_with_a_file_too() {
        let inline =
            |k: &str| (k == "OP_BRAND_TOML").then(|| "[brand]\naccent = \"#654321\"\n".to_string());
        assert_eq!(Brand::from_env(inline).unwrap().accent, "#654321");
        let both = |k: &str| match k {
            "OP_BRAND_TOML" => Some("[brand]\n".to_string()),
            "OP_BRAND_FILE" => Some("/etc/brand.toml".to_string()),
            _ => None,
        };
        assert_eq!(Brand::from_env(both), Err(BrandError::Both));
    }

    #[test]
    fn unknown_keys_and_bad_toml_are_refused() {
        assert!(matches!(
            Brand::from_toml("[brand]\naccnet = \"#fff\"\n"),
            Err(BrandError::Parse(_))
        ));
        assert!(matches!(
            Brand::from_toml("[brand\n"),
            Err(BrandError::Parse(_))
        ));
    }

    /// A value is written into a `<style>` element, so anything that could
    /// end its declaration, block or the element is refused at startup.
    #[test]
    fn values_that_could_leave_the_stylesheet_are_refused() {
        for bad in [
            "red; color: blue",
            "#fff}",
            "#fff</style><script>",
            "url(x)\\",
            "#fff\n",
            "",
        ] {
            let text = format!("[brand]\naccent = {bad:?}\n");
            assert!(
                matches!(
                    Brand::from_toml(&text),
                    Err(BrandError::Value { key: "accent", .. })
                ),
                "{bad:?} must be refused"
            );
        }
        for ok in [
            "#c8102e",
            "rgba(255,255,255,.08)",
            "0 4px 16px rgba(0,0,0,.08)",
            "14px",
        ] {
            assert!(
                Brand::from_toml(&format!("[brand]\naccent = {ok:?}\n")).is_ok(),
                "{ok:?}"
            );
        }
    }

    #[test]
    fn the_logo_must_be_an_https_address() {
        for bad in [
            "http://acme.example/l.svg",
            "javascript:alert(1)",
            "https://x\" onerror=\"y",
            "https:///l.svg",
        ] {
            let text = format!("[brand]\nlogo_url = {bad:?}\n");
            assert!(
                matches!(
                    Brand::from_toml(&text),
                    Err(BrandError::Value {
                        key: "logo_url",
                        ..
                    })
                ),
                "{bad:?} must be refused"
            );
        }
    }

    #[test]
    fn css_vars_carry_every_token() {
        let css = Brand::default().css_vars();
        for name in [
            "--brand-accent: #c8102e;",
            "--brand-side-bg: #0d3a64;",
            "--brand-radius-sm: 10px;",
            "--brand-font-family:",
        ] {
            assert!(css.contains(name), "{name} in {css}");
        }
        assert_eq!(css.matches("--brand-").count(), 16);
    }

    #[test]
    fn the_name_is_escaped_in_the_top_bar() {
        let b = Brand::from_toml("[brand]\nname = \"A <b>&</b> B\"\n").unwrap();
        assert!(b
            .topnav_brand_html()
            .contains(">A &lt;b&gt;&amp;&lt;/b&gt; B</a>"));
    }
}
