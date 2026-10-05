//! The page frame every COAT page shares: a sidebar (in the style of shadcn/ui's Sidebar),
//! a header with breadcrumbs, and a footer with the version.
//!
//! The look lives in `src/assets/coat.css` and the behaviour in `src/assets/*.js`; both are
//! compiled into the program, so a report is still one self-contained file.

use std::fmt::Write;
use std::sync::LazyLock;

use crate::update::{APP_NAME, CHANNEL, PREVIEW, REPO, VERSION};

pub const CSS: &str = include_str!("assets/coat.css");
pub const SHELL_JS: &str = include_str!("assets/shell.js");
pub const SERVED_JS: &str = include_str!("assets/served.js");

/// Who made COAT, shown in the hover card on the logo.
const DEVELOPER_NAME: &str = "Philip S";
const DEVELOPER_INITIALS: &str = "PS";
const DEVELOPER_GITHUB: &str = "archways404";

/// The app icon at favicon size; COAT Preview has its own green one.
pub const FAVICON_PNG: &[u8] = if PREVIEW {
    include_bytes!("../packaging/favicon-preview-64.png")
} else {
    include_bytes!("../packaging/favicon-64.png")
};
const LOGO_PNG: &[u8] = if PREVIEW {
    include_bytes!("../packaging/logo-preview-160.png")
} else {
    include_bytes!("../packaging/logo-160.png")
};

static FAVICON: LazyLock<String> = LazyLock::new(|| data_uri(FAVICON_PNG));
static LOGO: LazyLock<String> = LazyLock::new(|| data_uri(LOGO_PNG));

pub fn favicon_uri() -> &'static str {
    &FAVICON
}

pub fn logo_uri() -> &'static str {
    &LOGO
}

fn data_uri(png: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::from("data:image/png;base64,");
    for chunk in png.chunks(3) {
        let bytes = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = (u32::from(bytes[0]) << 16) | (u32::from(bytes[1]) << 8) | u32::from(bytes[2]);
        for (i, shift) in [18, 12, 6, 0].into_iter().enumerate() {
            out.push(if i <= chunk.len() { ALPHABET[(n >> shift & 63) as usize] as char } else { '=' });
        }
    }
    out
}

pub fn esc(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

/// Stroke icons in the style of lucide (shadcn's icon set), 24×24.
pub fn icon(name: &str) -> &'static str {
    match name {
        "panel" => "<svg viewBox='0 0 24 24'><rect x='3' y='3' width='18' height='18' rx='2'/><path d='M9 3v18'/></svg>",
        "search" => "<svg viewBox='0 0 24 24'><circle cx='11' cy='11' r='7'/><path d='m21 21-4.3-4.3'/></svg>",
        "overview" => "<svg viewBox='0 0 24 24'><rect x='3' y='3' width='7' height='9' rx='1'/><rect x='14' y='3' width='7' height='5' rx='1'/><rect x='14' y='12' width='7' height='9' rx='1'/><rect x='3' y='16' width='7' height='5' rx='1'/></svg>",
        "route" => "<svg viewBox='0 0 24 24'><circle cx='5' cy='12' r='2.5'/><circle cx='19' cy='6' r='2.5'/><circle cx='19' cy='18' r='2.5'/><path d='M7.5 12h4l5-5.2M11.5 12l5 5.2'/></svg>",
        "clock" => "<svg viewBox='0 0 24 24'><circle cx='12' cy='12' r='9'/><path d='M12 7v5l3 2'/></svg>",
        "steps" => "<svg viewBox='0 0 24 24'><path d='M10 6h11M10 12h11M10 18h11'/><path d='M4 6h1v4M4 10h2M6 18H4c0-1 2-2 2-3s-1-1.5-2-1'/></svg>",
        "alert" => "<svg viewBox='0 0 24 24'><path d='M10.3 3.9 1.8 18a2 2 0 0 0 1.7 3h17a2 2 0 0 0 1.7-3L13.7 3.9a2 2 0 0 0-3.4 0z'/><path d='M12 9v4M12 17h.01'/></svg>",
        "braces" => "<svg viewBox='0 0 24 24'><path d='M8 3H7a2 2 0 0 0-2 2v5a2 2 0 0 1-2 2 2 2 0 0 1 2 2v5a2 2 0 0 0 2 2h1M16 21h1a2 2 0 0 0 2-2v-5a2 2 0 0 1 2-2 2 2 0 0 1-2-2V5a2 2 0 0 0-2-2h-1'/></svg>",
        "ladder" => "<svg viewBox='0 0 24 24'><path d='m17 3 4 4-4 4M21 7H9M7 21l-4-4 4-4M3 17h12'/></svg>",
        "chart" => "<svg viewBox='0 0 24 24'><path d='M12 20V10M18 20V4M6 20v-4'/></svg>",
        "log" => "<svg viewBox='0 0 24 24'><path d='M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z'/><path d='M14 2v6h6M16 13H8M16 17H8M10 9H8'/></svg>",
        "plus" => "<svg viewBox='0 0 24 24'><path d='M12 5v14M5 12h14'/></svg>",
        "bookmark" => "<svg viewBox='0 0 24 24'><path d='m19 21-7-4-7 4V5a2 2 0 0 1 2-2h10a2 2 0 0 1 2 2z'/></svg>",
        "history" => "<svg viewBox='0 0 24 24'><path d='M3 12a9 9 0 1 0 3-6.7L3 8'/><path d='M3 3v5h5M12 7v5l3 3'/></svg>",
        "power" => "<svg viewBox='0 0 24 24'><path d='M18.4 6.6a9 9 0 1 1-12.8 0M12 2v10'/></svg>",
        "refresh" => "<svg viewBox='0 0 24 24'><path d='M3 12a9 9 0 0 1 15.5-6.3L21 8M21 3v5h-5M21 12a9 9 0 0 1-15.5 6.3L3 16M3 21v-5h5'/></svg>",
        "download" => "<svg viewBox='0 0 24 24'><path d='M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4M7 10l5 5 5-5M12 15V3'/></svg>",
        "check" => "<svg viewBox='0 0 24 24'><path d='M20 6 9 17l-5-5'/></svg>",
        "info" => "<svg viewBox='0 0 24 24'><circle cx='12' cy='12' r='9'/><path d='M12 16v-4M12 8h.01'/></svg>",
        "github" => "<svg viewBox='0 0 24 24'><path d='M15 22v-4a4.8 4.8 0 0 0-1-3.5c3 0 6-2 6-5.5.1-1.3-.3-2.5-1-3.5.3-1.2.3-2.4 0-3.5 0 0-1 0-3 1.5-2.6-.5-5.4-.5-8 0C6 2 5 2 5 2c-.3 1.1-.3 2.3 0 3.5-.7 1-1.1 2.2-1 3.5 0 3.5 3 5.5 6 5.5-.4.5-.7 1-.8 1.6-.2.6-.3 1.3-.2 1.9v4'/><path d='M9 18c-4.5 2-5-2-7-2'/></svg>",
        "chevron" => "<svg viewBox='0 0 24 24'><path d='m9 18 6-6-6-6'/></svg>",
        _ => "",
    }
}

pub struct NavItem {
    pub href: String,
    pub label: String,
    pub icon: &'static str,
    /// Id of the section this entry tracks while scrolling.
    pub spy: Option<String>,
    pub badge: Option<(String, bool)>,
    pub children: Vec<SubItem>,
}

pub struct SubItem {
    pub href: String,
    pub label: String,
    pub spy: String,
    pub kind: String,
}

pub struct NavGroup {
    pub label: String,
    pub items: Vec<NavItem>,
}

pub struct Page {
    pub title: String,
    /// Breadcrumb trail after "COAT"; the last one is the current page.
    pub crumbs: Vec<String>,
    pub header_right: String,
    pub nav: Vec<NavGroup>,
    pub body: String,
    pub script: String,
    /// Served by the running app: adds search, recent traces, updates and Quit.
    pub served: bool,
    /// Session id of the report on screen, so an update can re-open it.
    pub root: String,
}

pub fn render(page: &Page) -> String {
    let scripts = if page.served { format!("{SHELL_JS}{SERVED_JS}{}", page.script) } else { format!("{SHELL_JS}{}", page.script) };
    format!(
        "<!doctype html>\n<html lang=\"en\"><head><meta charset=\"utf-8\">\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\"><meta name=\"robots\" content=\"noindex\">\
         <link rel=\"icon\" type=\"image/png\" href=\"{favicon}\"><title>{title}</title><style>{CSS}</style></head>\
         <body data-version=\"{VERSION}\" data-channel=\"{CHANNEL}\" data-root=\"{root}\"><div class=\"shell\" id=\"shell\" data-sidebar=\"expanded\">\
         {sidebar}<div class=\"inset\">{header}<main id=\"top\">{body}</main>{footer}</div>\
         <div class=\"sb-backdrop\" id=\"sb-backdrop\"></div></div>{icons}<script>{scripts}</script></body></html>",
        favicon = favicon_uri(), root = esc(&page.root),
        title = esc(&if PREVIEW { page.title.replacen("COAT", "COAT Preview", 1) } else { page.title.clone() }),
        sidebar = sidebar(page), header = header(page), body = page.body, footer = footer(page),
        icons = if page.served { icon_templates() } else { String::new() })
}

fn sidebar(page: &Page) -> String {
    let home = if page.served { "/" } else { "#top" };
    let mut html = format!(
        "<aside class=\"sidebar\" id=\"sidebar\"><div class=\"sb-header\"><div class=\"hc\">\
         <a class=\"sb-brand\" href=\"{home}\" aria-describedby=\"dev-card\"><img src=\"{logo}\" alt=\"\">\
         <span class=\"sb-brand-text\"><b>COAT{pill}</b><small>{subtitle}</small></span></a>{card}</div></div>\
         <div class=\"sb-content\">",
        logo = favicon_uri(), card = developer_card(),
        pill = if PREVIEW { "<span class=\"sb-pill\">PREVIEW</span>" } else { "" },
        subtitle = if PREVIEW { "Preview build" } else { "Call overview &amp; timeline" });
    if page.served {
        let _ = write!(html,
            "<form class=\"sb-search\" action=\"/\" method=\"get\" title=\"Trace a call\">{}\
             <input name=\"q\" placeholder=\"Trace a call…\" autocomplete=\"off\" spellcheck=\"false\" aria-label=\"Simlog link or session id\">\
             <span class=\"sb-kbd\">/</span></form>", icon("search"));
    }
    for group in &page.nav {
        let _ = write!(html, "<div class=\"sb-group\"><div class=\"sb-label\">{}</div><ul class=\"sb-menu\">", esc(&group.label));
        for item in &group.items {
            html.push_str(&nav_item(item));
        }
        html.push_str("</ul></div>");
    }
    if page.served {
        let _ = write!(html,
            "<div class=\"sb-group sb-recent\"><div class=\"sb-label\">Recent</div><ul class=\"sb-menu\" id=\"recent-list\">\
             <li class=\"sb-empty\">…</li></ul></div>");
    }
    html.push_str("</div><div class=\"sb-footer\">");
    if page.served {
        let _ = write!(html,
            "<div id=\"update\"></div>\
             <button class=\"sb-btn upd-mini\" id=\"update-mini\" title=\"Updates\">{download}<span class=\"upd-dot\" hidden></span></button>\
             <button class=\"sb-btn\" id=\"quit\" title=\"Quit {APP_NAME}\">{power}<span class=\"sb-text\">Quit {APP_NAME}</span></button>",
            download = icon("download"), power = icon("power"));
    }
    let _ = write!(html,
        "<div class=\"sb-version\">{APP_NAME} v{VERSION}</div></div>\
         <button class=\"sb-rail\" data-sidebar-toggle aria-label=\"Toggle sidebar\" title=\"Toggle sidebar (⌘B / Ctrl+B)\"></button></aside>");
    html
}

fn nav_item(item: &NavItem) -> String {
    let spy = item.spy.as_ref().map_or(String::new(), |s| format!(" data-spy=\"{}\"", esc(s)));
    let parent = if item.children.is_empty() { "" } else { " data-spy-parent=\"steps\"" };
    let badge = item.badge.as_ref().map_or(String::new(), |(text, warn)| {
        format!("<span class=\"sb-badge{}\">{}</span>", if *warn { " warn" } else { "" }, esc(text))
    });
    let mut html = format!(
        "<li><a class=\"sb-btn\" href=\"{href}\"{spy}{parent} title=\"{label}\">{icon}<span class=\"sb-text\">{label}</span>{badge}</a>",
        href = esc(&item.href), label = esc(&item.label), icon = icon(item.icon));
    if !item.children.is_empty() {
        html.push_str("<ul class=\"sb-sub\">");
        for child in &item.children {
            let _ = write!(html,
                "<li><a href=\"{href}\" data-spy=\"{spy}\" class=\"k-{kind}\" title=\"{label}\"><span class=\"kd\"></span>\
                 <span class=\"sb-text\">{label}</span></a></li>",
                href = esc(&child.href), spy = esc(&child.spy), kind = esc(&child.kind), label = esc(&child.label));
        }
        html.push_str("</ul>");
    }
    html.push_str("</li>");
    html
}

/// shadcn-style hover card on the logo: who made COAT.
fn developer_card() -> String {
    format!(
        "<div class=\"hc-card\" id=\"dev-card\" role=\"tooltip\"><div class=\"hc-top\"><div class=\"hc-avatar\">{initials}</div><div>\
         <div class=\"hc-name\">{name}</div>\
         <a class=\"hc-link\" href=\"https://github.com/{github}\" target=\"_blank\" rel=\"noopener\">{gh}github.com/{github}</a></div></div>\
         <p>Made COAT. Ideas, bugs and pull requests are welcome on GitHub.</p>\
         <div class=\"hc-meta\"><img src=\"{logo}\" alt=\"\">{APP_NAME} v{VERSION}\
         <a href=\"https://github.com/{REPO}\" target=\"_blank\" rel=\"noopener\" style=\"margin-left:auto\">Source</a></div></div>",
        initials = DEVELOPER_INITIALS, name = DEVELOPER_NAME, github = DEVELOPER_GITHUB, gh = icon("github"), logo = favicon_uri())
}

fn header(page: &Page) -> String {
    let mut crumbs = if page.served { format!("<a href=\"/\">{APP_NAME}</a>") } else { format!("<span>{APP_NAME}</span>") };
    for (index, crumb) in page.crumbs.iter().enumerate() {
        let last = index + 1 == page.crumbs.len();
        let _ = write!(crumbs, "{}{}", icon("chevron"), if last { format!("<b>{}</b>", esc(crumb)) } else { format!("<span>{}</span>", esc(crumb)) });
    }
    format!(
        "<header class=\"inset-header\"><button class=\"icon-btn\" data-sidebar-toggle title=\"Toggle sidebar (⌘B / Ctrl+B)\" \
         aria-label=\"Toggle sidebar\">{panel}</button><span class=\"vsep\"></span><nav class=\"crumbs\" aria-label=\"Breadcrumb\">{crumbs}</nav>\
         <div class=\"header-right\">{right}</div></header>",
        panel = icon("panel"), right = page.header_right)
}

fn footer(page: &Page) -> String {
    let status = if page.served { "<span id=\"foot-update\"></span>" } else { "" };
    format!(
        "<footer class=\"site-footer\"><span>{APP_NAME} v{VERSION}{status}</span>{preview}<span class=\"grow\"></span>\
         <a href=\"https://github.com/{REPO}/releases\" target=\"_blank\" rel=\"noopener\">Releases</a>\
         <a href=\"https://github.com/{REPO}#documentation\" target=\"_blank\" rel=\"noopener\">Documentation</a>\
         <span>Made by <a href=\"https://github.com/{DEVELOPER_GITHUB}\" target=\"_blank\" rel=\"noopener\">{DEVELOPER_NAME}</a></span></footer>",
        preview = if PREVIEW { "<span class=\"sb-pill\" title=\"A preview build from the preview branch\">PREVIEW</span>" } else { "" })
}

/// Icons the browser code reuses when it redraws the update widget.
fn icon_templates() -> String {
    ["info", "check", "download", "refresh", "alert"].iter()
        .map(|name| format!("<template id=\"icon-{name}\">{}</template>", icon(name)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_known_values() {
        assert_eq!(data_uri(b"Man"), "data:image/png;base64,TWFu");
        assert_eq!(data_uri(b"Ma"), "data:image/png;base64,TWE=");
        assert_eq!(data_uri(b"M"), "data:image/png;base64,TQ==");
        assert!(favicon_uri().starts_with("data:image/png;base64,iVBORw0KGgo"));
    }

    #[test]
    fn every_icon_used_exists() {
        for name in ["panel", "search", "overview", "route", "clock", "steps", "alert", "braces", "ladder", "chart",
                     "log", "plus", "bookmark", "history", "power", "refresh", "download", "check", "info", "github", "chevron"] {
            assert!(icon(name).starts_with("<svg"), "missing icon {name}");
        }
    }

    #[test]
    fn static_pages_have_no_app_only_parts() {
        let page = Page { title: "t".into(), crumbs: vec!["x".into()], header_right: String::new(), nav: vec![],
                          body: String::new(), script: String::new(), served: false, root: String::new() };
        let html = render(&page);
        assert!(!html.contains("id=\"quit\"") && !html.contains("recent-list") && !html.contains("id=\"update\""));
        assert!(html.contains("Philip S") && html.contains(&format!("{APP_NAME} v{VERSION}")));
        assert!(html.contains(&format!("data-channel=\"{CHANNEL}\"")));
        assert_eq!(html.contains("PREVIEW"), PREVIEW);
    }
}
